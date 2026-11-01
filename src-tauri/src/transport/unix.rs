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
            fs::{FileTypeExt, PermissionsExt},
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
            let Ok(conn) = conn else { continue };
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
                handle_conn(&broker, conn, read_timeout);
                active.fetch_sub(1, SeqCst);
            });
        }
    });
    Ok(ServerHandle {
        path: endpoint,
        stop,
        thread,
    })
}

fn bind(path: &std::path::Path) -> Result<UnixListener, Error> {
    if UnixStream::connect(path).is_ok() {
        return Err(Error::Io("another instance is running".into()));
    }
    match std::fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_socket() => std::fs::remove_file(path)?,
        Ok(_) => {
            return Err(Error::Io(format!(
                "{} exists and is not a socket",
                path.display()
            )));
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    // Create it private from the start; umask only ever narrows, so concurrent file creation is safe.
    // SAFETY: umask has no preconditions.
    let old = unsafe { libc::umask(0o177) };
    let bound = UnixListener::bind(path);
    unsafe { libc::umask(old) };
    let listener = bound?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
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
