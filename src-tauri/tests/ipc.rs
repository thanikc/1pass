//! Real socket, fake host.
#![cfg(unix)]
use one_pass_lib::{
    broker::{Broker, Decision, Host, Limits, Peer, grants::GrantDuration, view::PromptView},
    session::{Cancel, Session},
    transport::{MAX_CONNECTIONS, MAX_REQUEST, ServerHandle, serve, serve_with},
};
use onepass_core::{
    Error,
    protocol::{Request, Response},
    vault::{ApprovalMode, KdfParams},
};
use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader, Read, Write},
    os::unix::{fs::MetadataExt, net::UnixStream},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering::SeqCst},
    },
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

const PW: &[u8] = b"password1";
const REF: &str = "op://p/e/A";
const ALLOW: Decision = Decision::Allow {
    mode: ApprovalMode::TimeBoxed,
    duration: GrantDuration::Min15,
};

struct FakeHost {
    decision: Mutex<Decision>,
    prompts: AtomicUsize,
    hang: AtomicBool,
    saw_cancel: AtomicBool,
    views: Mutex<Vec<PromptView>>,
}

impl Host for FakeHost {
    fn request_unlock(&self) {}
    fn prompt(&self, view: PromptView, timeout: Duration, cancel: Cancel) -> Decision {
        self.prompts.fetch_add(1, SeqCst);
        self.views.lock().unwrap().push(view);
        let start = Instant::now();
        while self.hang.load(SeqCst) {
            if cancel() {
                self.saw_cancel.store(true, SeqCst);
                return Decision::Cancelled;
            }
            if start.elapsed() > timeout {
                return Decision::TimedOut;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        *self.decision.lock().unwrap()
    }
}

struct Env {
    dir: PathBuf,
    sock: PathBuf,
    session: Arc<Session>,
    host: Arc<FakeHost>,
    broker: Arc<Broker>,
    server: Option<ServerHandle>,
}

fn env(tag: &str, decision: Decision, read_timeout: Option<Duration>) -> Env {
    let dir = std::env::temp_dir().join(format!("op-ipc-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let dir = dir.canonicalize().unwrap();
    let session = Arc::new(Session::new(
        dir.join("v.onepass"),
        KdfParams {
            m_cost: 8,
            t_cost: 1,
            p_cost: 1,
        },
    ));
    session.create(PW).unwrap();
    session
        .mutate(|b| {
            b.save_entry(
                "p",
                "e",
                "A",
                None,
                Some(Zeroizing::new("alpha".into())),
                String::new(),
            )
        })
        .unwrap();
    let host = Arc::new(FakeHost {
        decision: Mutex::new(decision),
        prompts: AtomicUsize::new(0),
        hang: AtomicBool::new(false),
        saw_cancel: AtomicBool::new(false),
        views: Mutex::default(),
    });
    let limits = Limits {
        unlock_wait: Duration::from_millis(200),
        approval_wait: Duration::from_secs(2),
        request_deadline: Duration::from_secs(5),
    };
    let broker = Arc::new(Broker::new(session.clone(), host.clone(), limits));
    let sock = dir.join("s.sock");
    let server = Some(match read_timeout {
        Some(t) => serve_with(broker.clone(), sock.clone(), t).unwrap(),
        None => serve(broker.clone(), sock.clone()).unwrap(),
    });
    Env {
        dir,
        sock,
        session,
        host,
        broker,
        server,
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        if let Some(s) = self.server.take() {
            s.shutdown();
        }
    }
}

fn request(dir: &Path, refs: &[&str]) -> Request {
    Request {
        version: 1,
        project_dir: dir.to_string_lossy().into_owned(),
        command: vec!["node".into()],
        references: refs.iter().map(|s| s.to_string()).collect(),
        variables: refs
            .iter()
            .enumerate()
            .map(|(i, r)| (format!("V{i}"), r.to_string()))
            .collect(),
    }
}

fn send_raw(sock: &Path, bytes: &[u8]) -> Response {
    let mut s = UnixStream::connect(sock).unwrap();
    s.write_all(bytes).unwrap();
    let mut line = String::new();
    BufReader::new(s).read_line(&mut line).unwrap();
    serde_json::from_str(&line).unwrap()
}

fn send(e: &Env, refs: &[&str]) -> Response {
    let mut b = serde_json::to_vec(&request(&e.dir, refs)).unwrap();
    b.push(b'\n');
    send_raw(&e.sock, &b)
}

fn ok(r: Response) -> BTreeMap<String, String> {
    match r {
        Response::Ok(m) => m,
        Response::Err(e) => panic!("expected ok, got {e}"),
    }
}

fn err(r: Response) -> Error {
    match r {
        Response::Err(e) => e,
        Response::Ok(_) => panic!("expected error"),
    }
}

#[test]
fn allow() {
    let e = env("allow", ALLOW, None);
    assert_eq!(ok(send(&e, &[REF]))[REF], "alpha");
}

#[test]
fn deny() {
    let e = env("deny", Decision::Deny, None);
    assert_eq!(err(send(&e, &[REF])), Error::Denied);
}

#[test]
fn locked() {
    let e = env("locked", ALLOW, None);
    e.session.lock();
    assert_eq!(err(send(&e, &[REF])), Error::VaultLocked);
    assert_eq!(e.host.prompts.load(SeqCst), 0);
}

#[test]
fn rate_limit() {
    let e = env("rate", Decision::Deny, None);
    for _ in 0..3 {
        assert_eq!(err(send(&e, &[REF])), Error::Denied);
    }
    assert_eq!(err(send(&e, &[REF])), Error::RateLimited);
}

#[test]
fn changed_reference_set() {
    let e = env("changed", ALLOW, None);
    ok(send(&e, &[REF]));
    ok(send(&e, &[REF]));
    assert_eq!(e.host.prompts.load(SeqCst), 1);
    e.session
        .mutate(|b| {
            b.save_entry(
                "p",
                "e",
                "B",
                None,
                Some(Zeroizing::new("beta".into())),
                String::new(),
            )
        })
        .unwrap();
    ok(send(&e, &[REF, "op://p/e/B"]));
    assert_eq!(e.host.prompts.load(SeqCst), 2);
}

#[test]
fn socket_is_0600() {
    let e = env("mode", ALLOW, None);
    assert_eq!(std::fs::metadata(&e.sock).unwrap().mode() & 0o777, 0o600);
}

#[test]
fn peer_pid_and_exe_reach_prompt() {
    let e = env("peer", ALLOW, None);
    ok(send(&e, &[REF]));
    let v = e.host.views.lock().unwrap();
    assert_eq!(v[0].peer_pid, Some(std::process::id()));
    assert!(v[0].peer_exe.as_deref().is_some_and(|x| !x.is_empty()));
}

#[test]
fn oversized_request_rejected_without_prompt() {
    let e = env("big", ALLOW, None);
    let r = send_raw(&e.sock, &vec![b'a'; MAX_REQUEST + 1]);
    assert!(matches!(err(r), Error::InvalidInput(_)));
    assert_eq!(e.host.prompts.load(SeqCst), 0);
}

#[test]
fn slow_client_does_not_wedge_server() {
    let e = env("slow", ALLOW, Some(Duration::from_millis(300)));
    let mut idle = UnixStream::connect(&e.sock).unwrap();
    assert_eq!(ok(send(&e, &[REF]))[REF], "alpha");
    idle.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    let mut b = [0u8; 1];
    assert!(matches!(idle.read(&mut b), Ok(0) | Err(_)));
    assert!(!matches!(idle.read(&mut b), Err(ref x) if x.kind() == std::io::ErrorKind::WouldBlock));
}

#[test]
fn connection_cap() {
    let e = env("cap", ALLOW, Some(Duration::from_secs(30)));
    let idle: Vec<UnixStream> = (0..MAX_CONNECTIONS)
        .map(|_| UnixStream::connect(&e.sock).unwrap())
        .collect();
    std::thread::sleep(Duration::from_millis(200));
    let mut extra = UnixStream::connect(&e.sock).unwrap();
    extra
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let mut b = [0u8; 1];
    assert!(
        matches!(extra.read(&mut b), Ok(0) | Err(_)),
        "over-cap connection got a response"
    );
    drop(idle);
    let start = Instant::now();
    loop {
        let mut line = serde_json::to_vec(&request(&e.dir, &[REF])).unwrap();
        line.push(b'\n');
        let mut s = UnixStream::connect(&e.sock).unwrap();
        s.write_all(&line).unwrap();
        let mut out = String::new();
        let _ = BufReader::new(s).read_line(&mut out);
        if out.contains("alpha") {
            break;
        }
        assert!(
            start.elapsed() < Duration::from_secs(3),
            "server did not recover"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn stale_socket_replaced() {
    let dir = std::env::temp_dir().join(format!("op-ipc-stale-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let sock = dir.join("s.sock");
    drop(std::os::unix::net::UnixListener::bind(&sock).unwrap());
    assert!(sock.exists());
    let e = env("stale2", ALLOW, None);
    let h = serve(e.broker.clone(), sock).unwrap();
    h.shutdown();
}

#[test]
fn live_socket_refused() {
    let e = env("live", ALLOW, None);
    assert!(serve(e.broker.clone(), e.sock.clone()).is_err());
    assert_eq!(ok(send(&e, &[REF]))[REF], "alpha");
}

#[test]
fn regular_file_at_endpoint_is_not_deleted() {
    let e = env("file", ALLOW, None);
    let f = e.dir.join("precious");
    std::fs::write(&f, b"x").unwrap();
    assert!(serve(e.broker.clone(), f.clone()).is_err());
    assert_eq!(std::fs::read(f).unwrap(), b"x");
}

#[test]
fn client_exit_cancels_prompt() {
    let e = env("exit", ALLOW, None);
    e.host.hang.store(true, SeqCst);
    let mut s = UnixStream::connect(&e.sock).unwrap();
    let mut line = serde_json::to_vec(&request(&e.dir, &[REF])).unwrap();
    line.push(b'\n');
    s.write_all(&line).unwrap();
    while e.host.prompts.load(SeqCst) == 0 {
        std::thread::sleep(Duration::from_millis(10));
    }
    drop(s);
    let start = Instant::now();
    while !e.host.saw_cancel.load(SeqCst) {
        assert!(
            start.elapsed() < Duration::from_secs(1),
            "cancel not observed"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn connection_closes_after_response() {
    let e = env("close", ALLOW, None);
    let mut line = serde_json::to_vec(&request(&e.dir, &[REF])).unwrap();
    line.push(b'\n');
    let mut s = UnixStream::connect(&e.sock).unwrap();
    s.write_all(&line).unwrap();
    let mut r = BufReader::new(s);
    let mut first = String::new();
    r.read_line(&mut first).unwrap();
    let mut rest = Vec::new();
    r.read_to_end(&mut rest).unwrap();
    assert!(rest.is_empty());
    let _ = Peer::default();
}
