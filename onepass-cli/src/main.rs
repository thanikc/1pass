mod dotenv;

use onepass_core::{
    Error,
    protocol::{PROTOCOL_VERSION, REF_PREFIX, Reference, Request, Response},
};
use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader, Read, Write},
    process::{Command, exit},
    time::Duration,
};

const USAGE: &str = "usage: op run [--env-file <path>]... -- <command> [args...]";
const WAIT: Duration = Duration::from_secs(60);
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
    // shortcut: unix sockets only, Windows named pipe is still to do.
    let path = match std::env::var_os("ONEPASS_SOCKET") {
        Some(p) => p.into(),
        None => std::path::PathBuf::from(
            std::env::var_os("XDG_RUNTIME_DIR")
                .ok_or_else(|| Error::Io("set ONEPASS_SOCKET or XDG_RUNTIME_DIR".into()))?,
        )
        .join("onepass.sock"),
    };
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

    let mut refs = Vec::new();
    for (name, v) in &vars {
        if v.starts_with(REF_PREFIX) {
            Reference::parse(v).unwrap_or_else(|e| die(format!("{name}: {e}")));
            refs.push(v.clone());
        } else if dotenv::looks_secret(name, v) {
            eprintln!("op: warning: {name} looks like a literal secret, store it in the vault");
        }
    }
    refs.sort();
    refs.dedup();

    if !refs.is_empty() {
        let project_dir = std::env::current_dir()
            .and_then(|d| d.canonicalize())
            .unwrap_or_else(|e| die(e));
        let req = Request {
            version: PROTOCOL_VERSION,
            project_dir: project_dir.to_string_lossy().into_owned(),
            command: command.clone(),
            references: refs,
        };
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

    let status = Command::new(&command[0])
        .args(&command[1..])
        .envs(&vars)
        .status()
        .unwrap_or_else(|e| die(format!("{}: {e}", command[0])));
    #[cfg(unix)]
    if let Some(sig) = std::os::unix::process::ExitStatusExt::signal(&status) {
        exit(128 + sig); // shortcut: reports the shell-style code instead of re-raising the signal.
    }
    exit(status.code().unwrap_or(1));
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
}
