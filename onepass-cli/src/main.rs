mod dotenv;

use onepass_core::{
    Error,
    protocol::{PROTOCOL_VERSION, REF_PREFIX, Reference, Request, Response, endpoint},
};
use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader, Read, Write},
    process::{Command, exit},
    time::Duration,
};

const USAGE: &str = "usage: op run [--env-file <path>]... -- <command> [args...]";
// The server owns the real deadlines (unlock 60 s, prompt 120 s, request 180 s); this only backstops a dead app.
const WAIT: Duration = Duration::from_secs(185);
const MAX_RESPONSE: u64 = 16 << 20;

fn die(msg: impl std::fmt::Display) -> ! {
    eprintln!("op: {msg}");
    exit(1)
}

/// Send the request as one JSON line, read one JSON line back.
fn exchange<S: Read + Write>(mut s: S, req: &Request) -> Result<BTreeMap<String, String>, Error> {
    let mut line = serde_json::to_vec(req).map_err(|e| Error::Io(e.to_string()))?;
    line.push(b'\n');
    s.write_all(&line)?;
    let mut buf = String::new();
    BufReader::new(s.take(MAX_RESPONSE)).read_line(&mut buf)?;
    match serde_json::from_str(&buf).map_err(|e| Error::Io(format!("bad response: {e}")))? {
        Response::Ok(values) => Ok(values),
        Response::Err(e) => Err(e),
    }
}

#[cfg(unix)]
fn connect() -> Result<std::os::unix::net::UnixStream, Error> {
    let path = endpoint()?;
    let s = std::os::unix::net::UnixStream::connect(&path).map_err(|e| {
        Error::Io(format!(
            "cannot reach the one-pass app at {}: {e}",
            path.display()
        ))
    })?;
    s.set_read_timeout(Some(WAIT))?;
    Ok(s)
}

#[cfg(not(unix))]
fn connect() -> Result<std::net::TcpStream, Error> {
    Err(Error::Io("this platform is not supported yet".into()))
}

/// `None` when no variable is a reference. Literal values never enter the request.
fn build_request(
    vars: &BTreeMap<String, String>,
    command: &[String],
    project_dir: String,
) -> Result<Option<Request>, Error> {
    let mut variables = BTreeMap::new();
    for (name, v) in vars.iter().filter(|(_, v)| v.starts_with(REF_PREFIX)) {
        Reference::parse(v)?;
        variables.insert(name.clone(), v.clone());
    }
    let mut references: Vec<String> = variables.values().cloned().collect();
    references.sort();
    references.dedup();
    if references.is_empty() {
        return Ok(None);
    }
    Ok(Some(Request {
        version: PROTOCOL_VERSION,
        project_dir,
        command: command.to_vec(),
        references,
        variables,
    }))
}

fn main() {
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() != Some("run") {
        die(USAGE);
    }
    let mut files = Vec::new();
    loop {
        match args.next().as_deref() {
            Some("--env-file") => files.push(args.next().unwrap_or_else(|| die(USAGE))),
            Some("--") => break,
            _ => die(USAGE),
        }
    }
    let command: Vec<String> = args.collect();
    if command.is_empty() {
        die(USAGE);
    }
    let explicit = !files.is_empty();
    if !explicit {
        files.push(".env".into());
    }

    // Later files override earlier ones.
    let mut vars: BTreeMap<String, String> = BTreeMap::new();
    for f in &files {
        let text = match std::fs::read_to_string(f) {
            Ok(t) => t,
            Err(e) if !explicit && e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => die(format!("{f}: {e}")),
        };
        vars.extend(dotenv::parse(&text).unwrap_or_else(|e| die(format!("{f}: {e}"))));
    }

    for (name, v) in &vars {
        if !v.starts_with(REF_PREFIX) && dotenv::looks_secret(name, v) {
            eprintln!("op: warning: {name} looks like a literal secret, store it in the vault");
        }
    }

    let project_dir = std::env::current_dir()
        .and_then(|d| d.canonicalize())
        .unwrap_or_else(|e| die(e));
    let req = build_request(&vars, &command, project_dir.to_string_lossy().into_owned())
        .unwrap_or_else(|e| die(e));
    if let Some(req) = req {
        let values = connect()
            .and_then(|s| exchange(s, &req))
            .unwrap_or_else(|e| die(e));
        for v in vars.values_mut() {
            if v.starts_with(REF_PREFIX) {
                *v = values
                    .get(v.as_str())
                    .cloned()
                    .unwrap_or_else(|| die(Error::EntryNotFound(v.clone())));
            }
        }
    }

    let mut child = Command::new(&command[0]);
    child.args(&command[1..]).envs(&vars);
    // Unix: replace this process so signals reach the child and its status is ours.
    #[cfg(unix)]
    die(format!(
        "{}: {}",
        command[0],
        std::os::unix::process::CommandExt::exec(&mut child)
    ));
    #[cfg(not(unix))]
    {
        let status = child
            .status()
            .unwrap_or_else(|e| die(format!("{}: {e}", command[0])));
        exit(status.code().unwrap_or(1));
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::net::UnixStream;

    fn serve(reply: Response) -> UnixStream {
        let (client, server) = UnixStream::pair().unwrap();
        std::thread::spawn(move || {
            let mut line = String::new();
            BufReader::new(&server).read_line(&mut line).unwrap();
            let req: Request = serde_json::from_str(&line).unwrap();
            assert_eq!(req.version, PROTOCOL_VERSION);
            (&server)
                .write_all(serde_json::to_string(&reply).unwrap().as_bytes())
                .unwrap();
            (&server).write_all(b"\n").unwrap();
        });
        client
    }

    fn req() -> Request {
        Request {
            version: PROTOCOL_VERSION,
            project_dir: "/p".into(),
            command: vec!["x".into()],
            references: vec!["op://a/b/c".into()],
            variables: Default::default(),
        }
    }

    #[test]
    fn allow() {
        let r = BTreeMap::from([("op://a/b/c".to_string(), "v".to_string())]);
        assert_eq!(exchange(serve(Response::Ok(r.clone())), &req()).unwrap(), r);
    }

    #[test]
    fn deny() {
        assert_eq!(
            exchange(serve(Response::Err(Error::Denied)), &req()).unwrap_err(),
            Error::Denied
        );
    }

    fn vars(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn build_request_maps_variables_and_omits_literals() {
        let v = vars(&[("A", "op://p/e/n"), ("B", "op://p/e/n"), ("C", "literal")]);
        let r = build_request(&v, &["x".into()], "/p".into())
            .unwrap()
            .unwrap();
        assert_eq!(r.references, ["op://p/e/n"]);
        assert_eq!(r.variables.keys().collect::<Vec<_>>(), ["A", "B"]);
        assert!(!serde_json::to_string(&r).unwrap().contains("literal"));
    }

    #[test]
    fn build_request_none_without_references() {
        assert!(
            build_request(&vars(&[("C", "x")]), &["x".into()], "/p".into())
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn build_request_rejects_malformed_reference() {
        let e = build_request(&vars(&[("A", "op://bad")]), &["x".into()], "/p".into());
        assert!(matches!(e, Err(Error::BadReference(_))));
    }
}
