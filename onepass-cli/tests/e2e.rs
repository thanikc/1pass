//! Real `op` binary against a socket served from a real (in-memory) vault.
use onepass_core::{
    protocol::Request,
    vault::{Entry, KdfParams, Vault},
};
use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixListener,
    process::Command,
};

fn vault() -> Vault {
    let mut v = Vault::create_with(
        b"pw",
        KdfParams {
            m_cost: 8,
            t_cost: 1,
            p_cost: 1,
        },
    )
    .unwrap();
    v.body
        .projects
        .entry("demo".into())
        .or_default()
        .entry("dev".into())
        .or_default()
        .insert("TEST_KEY".into(), Entry::secret("test-value-123"));
    v
}

fn run(env: &str) -> std::process::Output {
    let dir = std::env::temp_dir().join(format!(
        "op-e2e-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let (sock, envf) = (dir.join("s.sock"), dir.join(".env"));
    std::fs::write(&envf, env).unwrap();
    let l = UnixListener::bind(&sock).unwrap();
    std::thread::spawn(move || {
        let v = vault();
        if let Ok((s, _)) = l.accept() {
            let mut line = String::new();
            BufReader::new(&s).read_line(&mut line).unwrap();
            let resp = v.resolve(&serde_json::from_str::<Request>(&line).unwrap());
            writeln!(&s, "{}", serde_json::to_string(&resp).unwrap()).unwrap();
        }
    });
    let out = Command::new(env!("CARGO_BIN_EXE_op"))
        .env("ONEPASS_SOCKET", &sock)
        .args(["run", "--env-file"])
        .arg(&envf)
        .args(["--", "sh", "-c", "echo \"$TEST_KEY|$PLAIN\""])
        .output()
        .unwrap();
    std::fs::remove_dir_all(dir).unwrap();
    out
}

#[test]
fn resolves_reference_and_passes_literal() {
    let o = run("TEST_KEY=op://demo/dev/TEST_KEY\nPLAIN=literal\n");
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(
        String::from_utf8_lossy(&o.stdout),
        "test-value-123|literal\n"
    );
}

#[test]
fn missing_entry_fails_without_running_child() {
    let o = run("TEST_KEY=op://demo/dev/NOPE\n");
    assert!(!o.status.success());
    assert!(o.stdout.is_empty());
    assert!(String::from_utf8_lossy(&o.stderr).contains("entry not found"));
}

fn run_plain(script: &str) -> std::process::Output {
    let envf = std::env::temp_dir().join(format!(
        "op-plain-{}-{:?}.env",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::write(&envf, "PLAIN=literal\n").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_op"))
        .args(["run", "--env-file"])
        .arg(&envf)
        .args(["--", "sh", "-c", script])
        .output()
        .unwrap();
    std::fs::remove_file(envf).unwrap();
    out
}

#[test]
fn exit_code_is_forwarded() {
    assert_eq!(run_plain("exit 7").status.code(), Some(7));
}

#[test]
fn signal_death_is_forwarded() {
    use std::os::unix::process::ExitStatusExt;
    assert_eq!(run_plain("kill -TERM $$").status.signal(), Some(15));
}
