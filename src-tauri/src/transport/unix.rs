//! Unix domain socket: 0600, same-uid peers only, peer pid/exe looked up for the dialog.
use super::{Conn, MAX_CONNECTIONS, READ_TIMEOUT, handle_conn};
use crate::{
    broker::{Broker, Peer},
    session::Cancel,
};
use onepass_core::Error;
use std::{
    os::{
        fd::AsRawFd,
        unix::{
            fs::{DirBuilderExt, FileTypeExt, PermissionsExt},
            net::{UnixListener, UnixStream},
        },
    },
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering::SeqCst},
    },
    thread::JoinHandle,
    time::Duration,
};

pub struct ServerHandle {
    path: PathBuf,
    stop: Arc<AtomicBool>,
    thread: JoinHandle<()>,
}

impl ServerHandle {
    /// Stops accepting and removes the socket file.
    pub fn shutdown(self) {
        self.stop.store(true, SeqCst);
        let _ = UnixStream::connect(&self.path); // wake the blocked accept
        let _ = self.thread.join();
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Releases a connection slot even if the handler panics.
struct ConnSlot(Arc<AtomicUsize>);

impl Drop for ConnSlot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_is_released_on_panic() {
        let n = Arc::new(AtomicUsize::new(1));
        let n2 = n.clone();
        let r = std::thread::spawn(move || {
            let _slot = ConnSlot(n2);
            panic!("handler panic");
        })
        .join();
        assert!(r.is_err());
        assert_eq!(n.load(SeqCst), 0);
    }

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("onepass-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir(&d).unwrap();
        d
    }

    #[test]
    fn bind_does_not_change_umask_for_other_threads() {
        let dir = scratch("umask");
        let (stop, made) = (Arc::new(AtomicBool::new(false)), dir.join("made"));
        std::fs::create_dir(&made).unwrap();
        let (stop2, made2) = (stop.clone(), made.clone());
        let t = std::thread::spawn(move || {
            let mut n = 0;
            while !stop2.load(SeqCst) {
                std::fs::create_dir(made2.join(n.to_string())).unwrap();
                n += 1;
            }
        });
        let ep = dir.join("ep.sock");
        for _ in 0..2000 {
            drop(bind(&ep).unwrap());
        }
        stop.store(true, SeqCst);
        t.join().unwrap();
        for e in std::fs::read_dir(&made).unwrap() {
            let d = e.unwrap().path();
            std::fs::write(d.join("f"), b"x").unwrap_or_else(|e| panic!("{d:?} unusable: {e}"));
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn nothing_is_left_beside_the_endpoint() {
        let dir = scratch("clean");
        let ep = dir.join("ep.sock");
        let _l = bind(&ep).unwrap();
        let names: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, ["ep.sock"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn leftover_temp_dir_makes_bind_fail() {
        let dir = scratch("leftover");
        let ep = dir.join("ep.sock");
        std::fs::create_dir(temp_dir_for(&ep)).unwrap();
        assert!(bind(&ep).is_err());
        assert!(!ep.exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

pub fn serve(broker: Arc<Broker>, endpoint: PathBuf) -> Result<ServerHandle, Error> {
    serve_with(broker, endpoint, READ_TIMEOUT)
}

pub fn serve_with(
    broker: Arc<Broker>,
    endpoint: PathBuf,
    read_timeout: Duration,
) -> Result<ServerHandle, Error> {
    let listener = bind(&endpoint)?;
    let (stop, active) = (
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicUsize::new(0)),
    );
    let stop2 = stop.clone();
    let thread = std::thread::spawn(move || {
        for conn in listener.incoming() {
            if stop2.load(SeqCst) {
                break;
            }
            let Ok(conn) = conn else {
                std::thread::sleep(Duration::from_millis(50)); // e.g. EMFILE: don't spin
                continue;
            };
            if !same_user(&conn) {
                continue;
            }
            // Over the cap: close without a response.
            if active.fetch_add(1, SeqCst) >= MAX_CONNECTIONS {
                active.fetch_sub(1, SeqCst);
                continue;
            }
            let (broker, active) = (broker.clone(), active.clone());
            std::thread::spawn(move || {
                let _slot = ConnSlot(active);
                handle_conn(&broker, conn, read_timeout);
            });
        }
    });
    Ok(ServerHandle {
        path: endpoint,
        stop,
        thread,
    })
}

/// Private staging directory next to the endpoint (same filesystem, so the rename is atomic).
fn temp_dir_for(path: &std::path::Path) -> PathBuf {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    path.with_file_name(format!(".{name}.tmp"))
}

/// Removes the staging directory (and anything in it) on every exit path.
struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn bind(path: &std::path::Path) -> Result<UnixListener, Error> {
    // Bind inside a 0700 directory so the socket is never reachable with a wider mode,
    // without touching the process-wide umask. create fails if the name already exists:
    // a leftover or planted directory is not trusted.
    let dir = temp_dir_for(path);
    std::fs::DirBuilder::new().mode(0o700).create(&dir)?;
    let _guard = TempDir(dir.clone());
    let staged = dir.join("s");
    let listener = UnixListener::bind(&staged)?;
    std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o600))?;

    // Checked as late as possible: rename would silently replace a regular file.
    if UnixStream::connect(path).is_ok() {
        return Err(Error::Io("another instance is running".into()));
    }
    match std::fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_socket() => {} // stale: rename replaces it atomically
        Ok(_) => {
            return Err(Error::Io(format!(
                "{} exists and is not a socket",
                path.display()
            )));
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    std::fs::rename(&staged, path)?;
    Ok(listener)
}

#[cfg(target_os = "linux")]
fn peer_cred(s: &UnixStream) -> Option<(u32, Option<u32>)> {
    let mut c: libc::ucred = unsafe { std::mem::zeroed() };
    let mut len = size_of::<libc::ucred>() as libc::socklen_t;
    // SAFETY: valid fd, `c`/`len` are correctly sized for SO_PEERCRED.
    let r = unsafe {
        libc::getsockopt(
            s.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&raw mut c).cast(),
            &mut len,
        )
    };
    (r == 0).then_some((c.uid, Some(c.pid as u32)))
}

#[cfg(target_os = "macos")]
fn peer_cred(s: &UnixStream) -> Option<(u32, Option<u32>)> {
    let fd = s.as_raw_fd();
    let (mut uid, mut gid) = (0, 0);
    // SAFETY: valid fd and out-pointers.
    if unsafe { libc::getpeereid(fd, &mut uid, &mut gid) } != 0 {
        return None;
    }
    let mut pid: libc::pid_t = 0;
    let mut len = size_of::<libc::pid_t>() as libc::socklen_t;
    let r = unsafe {
        libc::getsockopt(
            fd,
            0, /* SOL_LOCAL */
            libc::LOCAL_PEERPID,
            (&raw mut pid).cast(),
            &mut len,
        )
    };
    Some((uid, (r == 0 && pid > 0).then_some(pid as u32)))
}

fn same_user(s: &UnixStream) -> bool {
    // SAFETY: geteuid has no preconditions.
    peer_cred(s).is_some_and(|(uid, _)| uid == unsafe { libc::geteuid() })
}

#[cfg(target_os = "linux")]
fn exe_of(pid: u32) -> Option<String> {
    std::fs::read_link(format!("/proc/{pid}/exe"))
        .ok()
        .map(|p| p.to_string_lossy().into_owned())
}

#[cfg(target_os = "macos")]
fn exe_of(pid: u32) -> Option<String> {
    let mut buf = vec![0u8; 4096];
    // SAFETY: buffer length passed matches its size.
    let n = unsafe { libc::proc_pidpath(pid as i32, buf.as_mut_ptr().cast(), buf.len() as u32) };
    (n > 0).then(|| String::from_utf8_lossy(&buf[..n as usize]).into_owned())
}

impl Conn for UnixStream {
    fn peer(&self) -> Peer {
        let pid = peer_cred(self).and_then(|(_, pid)| pid);
        Peer {
            pid,
            exe: pid.and_then(exe_of),
        }
    }

    fn client_gone(&self) -> Cancel {
        let Ok(s) = self.try_clone() else {
            return Arc::new(|| true);
        };
        Arc::new(move || {
            let mut b = 0u8;
            // SAFETY: valid fd owned by the clone; 1-byte buffer; non-blocking peek leaves data queued.
            let n = unsafe {
                libc::recv(
                    s.as_raw_fd(),
                    (&raw mut b).cast(),
                    1,
                    libc::MSG_PEEK | libc::MSG_DONTWAIT,
                )
            };
            match n {
                0 => true,
                n if n > 0 => false,
                _ => !matches!(
                    std::io::Error::last_os_error().kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ),
            }
        })
    }

    fn set_read_timeout(&self, d: Option<Duration>) -> std::io::Result<()> {
        UnixStream::set_read_timeout(self, d)
    }
}
