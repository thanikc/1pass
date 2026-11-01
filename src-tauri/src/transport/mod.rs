//! Local IPC: one request line in, one response line out, then close.
use crate::{
    broker::{Broker, Peer},
    session::Cancel,
};
use onepass_core::{Error, protocol::Response};
use std::{
    io::{self, Read, Write},
    time::{Duration, Instant},
};
use zeroize::{Zeroize, Zeroizing};

#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub use unix::{ServerHandle, serve, serve_with};

pub const MAX_REQUEST: usize = 64 * 1024;
pub const MAX_CONNECTIONS: usize = 32;
/// Total time a client has to deliver its request line.
pub const READ_TIMEOUT: Duration = Duration::from_secs(5);

pub trait Conn: Read + Write + Send + 'static {
    fn peer(&self) -> Peer;
    /// Turns true once the client has hung up.
    fn client_gone(&self) -> Cancel;
    fn set_read_timeout(&self, d: Option<Duration>) -> io::Result<()>;
}

/// `None` means close silently (EOF, timeout, I/O error).
fn read_request<C: Conn>(conn: &mut C, timeout: Duration) -> Option<Result<Vec<u8>, Error>> {
    let deadline = Instant::now() + timeout;
    let (mut buf, mut chunk) = (Vec::new(), [0u8; 4096]);
    loop {
        let left = deadline
            .checked_duration_since(Instant::now())
            .filter(|d| !d.is_zero())?;
        conn.set_read_timeout(Some(left)).ok()?;
        match conn.read(&mut chunk) {
            Ok(0) => return None,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => return None,
        }
        let too_big = || Some(Err(Error::InvalidInput("request too large".into())));
        match buf.iter().position(|b| *b == b'\n') {
            Some(i) if i > MAX_REQUEST => return too_big(),
            Some(i) => {
                buf.truncate(i);
                return Some(Ok(buf));
            }
            None if buf.len() > MAX_REQUEST => return too_big(),
            None => {}
        }
    }
}

fn zeroize_values(resp: Response) {
    if let Response::Ok(map) = resp {
        map.into_values().for_each(|mut v| v.zeroize());
    }
}

fn write_response<C: Conn>(conn: &mut C, resp: Response) -> io::Result<()> {
    // Capacity covers the worst case (6 bytes per escaped byte) so the buffer never
    // reallocates and leaves an unzeroized copy of a value behind.
    let cap = match &resp {
        Response::Ok(m) => {
            m.iter()
                .map(|(k, v)| 6 * (k.len() + v.len()) + 8)
                .sum::<usize>()
                + 16
        }
        Response::Err(_) => 256,
    };
    let mut bytes = Zeroizing::new(Vec::with_capacity(cap));
    if let Err(e) = serde_json::to_writer(&mut *bytes, &resp) {
        zeroize_values(resp);
        return Err(io::Error::other(e));
    }
    bytes.push(b'\n');
    let out = conn.write_all(&bytes).and_then(|()| conn.flush());
    zeroize_values(resp);
    out
}

pub fn handle_conn<C: Conn>(broker: &Broker, mut conn: C, read_timeout: Duration) {
    let Some(read) = read_request(&mut conn, read_timeout) else {
        return;
    };
    let _ = conn.set_read_timeout(None);
    let resp = match read {
        Ok(raw) => broker.handle(&raw, conn.peer(), conn.client_gone()),
        Err(e) => Response::Err(e),
    };
    let _ = write_response(&mut conn, resp);
}
