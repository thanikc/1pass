//! Puts the bundled `op` on `PATH` for formats that can't run install scripts (macOS, AppImage).
use onepass_core::Error;
use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
};

/// Where to install `op`, or `None` where the installer already handled `PATH`.
pub fn target_for(os: &str, appimage: Option<&OsStr>, home: Option<&Path>) -> Option<PathBuf> {
    match os {
        "macos" => Some("/usr/local/bin/op".into()),
        "linux" if appimage.is_some() => Some(home?.join(".local/bin/op")),
        _ => None,
    }
}

/// A mounted `.dmg` or translocated app: a link into it would break.
pub fn ephemeral(exe: &Path) -> bool {
    exe.starts_with("/Volumes")
        || exe
            .components()
            .any(|c| c.as_os_str() == "AppTranslocation")
}

#[derive(Debug, PartialEq)]
pub enum Existing {
    Absent,
    Ours,
    Foreign,
}

fn io(e: std::io::Error) -> Error {
    Error::Io(e.to_string())
}

pub fn classify(dest: &Path, source: &Path) -> Result<Existing, Error> {
    let meta = match fs::symlink_metadata(dest) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Existing::Absent),
        Err(e) => return Err(io(e)),
    };
    if meta.is_symlink() && fs::read_link(dest).map_err(io)? == source {
        return Ok(Existing::Ours);
    }
    match fs::metadata(dest) {
        Err(_) => return Ok(Existing::Absent), // dangling symlink
        Ok(m) if !m.is_file() => return Ok(Existing::Foreign), // never read a FIFO or device
        Ok(_) => {}
    }
    Ok(
        if fs::read(dest).map_err(io)? == fs::read(source).map_err(io)? {
            Existing::Ours
        } else {
            Existing::Foreign
        },
    )
}

pub fn install(exe: &Path, dest: &Path) -> Result<(), Error> {
    if cfg!(target_os = "macos") && ephemeral(exe) {
        return Err(Error::InvalidInput(
            "move the app to Applications first, then try again".into(),
        ));
    }
    let source = exe
        .parent()
        .ok_or_else(|| Error::Io("app path has no parent".into()))?
        .join(format!("op{}", std::env::consts::EXE_SUFFIX))
        .canonicalize()
        .map_err(io)?;
    match classify(dest, &source)? {
        Existing::Foreign => Err(Error::InvalidInput(format!(
            "{} already exists and isn't this app's op. Remove it and try again.",
            dest.display()
        ))),
        Existing::Ours => Ok(()),
        Existing::Absent => place(&source, dest),
    }
}

#[cfg(target_os = "linux")]
fn place(source: &Path, dest: &Path) -> Result<(), Error> {
    use std::{fs::OpenOptions, os::unix::fs::OpenOptionsExt};
    let dir = dest
        .parent()
        .ok_or_else(|| Error::Io("bad destination".into()))?;
    fs::create_dir_all(dir).map_err(io)?;
    let tmp = dir.join(format!("op.tmp-{}", std::process::id()));
    let write = || -> std::io::Result<()> {
        let mut out = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o755)
            .open(&tmp)?;
        std::io::copy(&mut fs::File::open(source)?, &mut out)?;
        if fs::symlink_metadata(dest).is_ok_and(|m| m.is_symlink()) && !dest.exists() {
            fs::remove_file(dest)?; // dangling
        }
        // hard_link fails if dest appeared since classify, unlike rename.
        fs::hard_link(&tmp, dest)?;
        fs::remove_file(&tmp)
    };
    // shortcut: an older copy from a previous AppImage counts as Foreign, so upgrades need a manual delete; upgrade if AppImage users hit this.
    write().map_err(|e| {
        let _ = fs::remove_file(&tmp);
        io(e)
    })
}

#[cfg(target_os = "macos")]
fn place(source: &Path, dest: &Path) -> Result<(), Error> {
    let out = std::process::Command::new("/usr/bin/osascript")
        .args(["-e", "on run argv"])
        // Re-checks under root: only a dangling link is removed, and no -f, so a real `op` that appeared since classify is kept.
        .args(["-e", r#"do shell script "mkdir -p /usr/local/bin && sh -c 'if [ -L \"$2\" ] && [ ! -e \"$2\" ]; then rm \"$2\"; fi; ln -sn \"$1\" \"$2\"' sh " & quoted form of item 1 of argv & " " & quoted form of item 2 of argv with administrator privileges"#])
        .args(["-e", "end run", "--"])
        .arg(source)
        .arg(dest)
        .output()
        .map_err(io)?;
    if out.status.success() {
        Ok(())
    } else {
        Err(Error::Io(
            String::from_utf8_lossy(&out.stderr).trim().to_string(),
        ))
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn place(_: &Path, _: &Path) -> Result<(), Error> {
    Err(Error::Io("this platform is not supported yet".into()))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};

    fn tmp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("op-cli-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn source(dir: &Path) -> PathBuf {
        let s = dir.join("op");
        fs::write(&s, b"ours").unwrap();
        s
    }

    #[test]
    fn targets() {
        let home = Path::new("/home/u");
        let img = Some(OsStr::new("/x/one-pass.AppImage"));
        assert_eq!(
            target_for("macos", None, Some(home)),
            Some("/usr/local/bin/op".into())
        );
        assert_eq!(
            target_for("linux", img, Some(home)),
            Some("/home/u/.local/bin/op".into())
        );
        assert_eq!(target_for("linux", None, Some(home)), None); // .deb/.rpm
        assert_eq!(target_for("linux", img, None), None);
        assert_eq!(target_for("windows", None, Some(home)), None);
    }

    #[test]
    fn ephemeral_paths() {
        assert!(ephemeral(Path::new(
            "/Volumes/one-pass/one-pass.app/Contents/MacOS/one-pass"
        )));
        assert!(ephemeral(Path::new(
            "/private/var/folders/a/AppTranslocation/b/d/one-pass.app/Contents/MacOS/one-pass"
        )));
        assert!(!ephemeral(Path::new(
            "/Applications/one-pass.app/Contents/MacOS/one-pass"
        )));
    }

    #[test]
    fn classify_absent() {
        let d = tmp("absent");
        let s = source(&d);
        assert_eq!(classify(&d.join("none"), &s).unwrap(), Existing::Absent);
        symlink(d.join("missing"), d.join("dangling")).unwrap();
        assert_eq!(classify(&d.join("dangling"), &s).unwrap(), Existing::Absent);
    }

    #[test]
    fn classify_ours() {
        let d = tmp("ours");
        let s = source(&d);
        symlink(&s, d.join("link")).unwrap();
        assert_eq!(classify(&d.join("link"), &s).unwrap(), Existing::Ours);
        fs::write(d.join("copy"), b"ours").unwrap();
        assert_eq!(classify(&d.join("copy"), &s).unwrap(), Existing::Ours);
    }

    #[test]
    fn classify_foreign() {
        let d = tmp("foreign");
        let s = source(&d);
        fs::write(d.join("file"), b"theirs").unwrap();
        assert_eq!(classify(&d.join("file"), &s).unwrap(), Existing::Foreign);
        symlink(d.join("file"), d.join("link")).unwrap();
        assert_eq!(classify(&d.join("link"), &s).unwrap(), Existing::Foreign);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn copy_creates_parent_and_is_executable() {
        let d = tmp("copy");
        source(&d);
        let dest = d.join("new/dir/op");
        install(&d.join("one-pass"), &dest).unwrap();
        assert_eq!(fs::read(&dest).unwrap(), b"ours");
        assert_eq!(
            fs::metadata(&dest).unwrap().permissions().mode() & 0o777,
            0o755
        );
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn copy_replaces_dangling_link_without_following() {
        let d = tmp("dangling");
        source(&d);
        let (dest, target) = (d.join("bin-op"), d.join("missing"));
        symlink(&target, &dest).unwrap();
        install(&d.join("one-pass"), &dest).unwrap();
        assert!(!fs::symlink_metadata(&dest).unwrap().is_symlink());
        assert!(!target.exists());
    }

    #[test]
    fn refuses_foreign_and_leaves_it() {
        let d = tmp("refuse");
        source(&d);
        let dest = d.join("dest-op");
        fs::write(&dest, b"theirs").unwrap();
        assert!(install(&d.join("one-pass"), &dest).is_err());
        assert_eq!(fs::read(&dest).unwrap(), b"theirs");
    }

    #[test]
    fn classify_directory_is_foreign() {
        let d = tmp("dir");
        let s = source(&d);
        fs::create_dir(d.join("sub")).unwrap();
        assert_eq!(classify(&d.join("sub"), &s).unwrap(), Existing::Foreign);
    }

    #[test]
    fn missing_bundled_op_errors() {
        let d = tmp("missing");
        assert!(install(&d.join("one-pass"), &d.join("dest-op")).is_err());
    }
}
