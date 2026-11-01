//! Dev-only stand-in for the desktop app: auto-allows everything, throwaway in-memory vault.
//! Run: `cargo run -p onepass-core --example dev_server`, then `op run --env-file .env.example -- env`.
use onepass_core::{
    protocol::Request,
    vault::{Entry, KdfParams, Vault},
};
use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixListener,
};

fn main() {
    let mut v = Vault::create_with(
        b"dev",
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

    let path = std::env::var("ONEPASS_SOCKET").unwrap_or_else(|_| {
        format!(
            "{}/onepass.sock",
            std::env::var("XDG_RUNTIME_DIR").expect("set ONEPASS_SOCKET")
        )
    });
    let _ = std::fs::remove_file(&path);
    let l = UnixListener::bind(&path).unwrap();
    eprintln!("dev_server listening on {path}");
    for s in l.incoming().flatten() {
        let mut line = String::new();
        if BufReader::new(&s).take_line(&mut line) {
            let resp = match serde_json::from_str::<Request>(&line) {
                Ok(req) => v.resolve(&req),
                Err(e) => {
                    onepass_core::protocol::Response::Err(onepass_core::Error::Io(e.to_string()))
                }
            };
            let _ = writeln!(&s, "{}", serde_json::to_string(&resp).unwrap());
        }
    }
}

trait TakeLine {
    fn take_line(&mut self, buf: &mut String) -> bool;
}
impl<R: BufRead> TakeLine for R {
    fn take_line(&mut self, buf: &mut String) -> bool {
        self.read_line(buf).is_ok_and(|n| n > 0)
    }
}
