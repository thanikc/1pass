use crate::Error;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf};

pub const PROTOCOL_VERSION: u32 = 1;
pub const REF_PREFIX: &str = "op://";

/// CLI -> app. Never carries secrets. `command` and `project_dir` are client-claimed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Request {
    pub version: u32,
    pub project_dir: String,
    pub command: Vec<String>,
    pub references: Vec<String>,
    /// Variable name -> reference, so the approval dialog can show names. Values are never sent.
    #[serde(default)]
    pub variables: BTreeMap<String, String>,
}

/// App -> CLI. On success maps each requested reference to its value.
#[derive(Serialize, Deserialize)]
pub enum Response {
    Ok(BTreeMap<String, String>),
    Err(Error),
}

/// Entry, environment and project names: `[A-Za-z0-9._-]{1,128}`, so references always round-trip.
pub fn validate_name(s: &str) -> Result<(), Error> {
    let ok = (1..=128).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'));
    if ok {
        Ok(())
    } else {
        Err(Error::InvalidInput(format!(
            "name must be 1-128 characters of A-Z a-z 0-9 . _ - (got {} bytes)",
            s.len()
        )))
    }
}

/// Where the app listens and `op` connects.
pub fn endpoint() -> Result<PathBuf, Error> {
    if let Some(p) = std::env::var_os("ONEPASS_SOCKET") {
        return Ok(p.into());
    }
    #[cfg(windows)]
    {
        let user = std::env::var("USERNAME").map_err(|_| Error::Io("USERNAME not set".into()))?;
        Ok(PathBuf::from(format!(r"\\.\pipe\onepass-{user}")))
    }
    #[cfg(target_os = "macos")]
    {
        let dir = std::env::var_os("TMPDIR")
            .ok_or_else(|| Error::Io("set ONEPASS_SOCKET or TMPDIR".into()))?;
        Ok(PathBuf::from(dir).join("onepass.sock"))
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        // No fallback: never put the socket in a world-writable directory.
        let dir = std::env::var_os("XDG_RUNTIME_DIR")
            .ok_or_else(|| Error::Io("set ONEPASS_SOCKET or XDG_RUNTIME_DIR".into()))?;
        Ok(PathBuf::from(dir).join("onepass.sock"))
    }
}

/// A parsed `op://<project>/<environment>/<entry>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference<'a> {
    pub project: &'a str,
    pub environment: &'a str,
    pub entry: &'a str,
}

impl<'a> Reference<'a> {
    /// `s` must start with `op://`; callers decide that, anything else is a literal.
    pub fn parse(s: &'a str) -> Result<Self, Error> {
        let bad = || Error::BadReference(s.to_string());
        let rest = s.strip_prefix(REF_PREFIX).ok_or_else(bad)?;
        let mut it = rest.split('/');
        match (it.next(), it.next(), it.next(), it.next()) {
            (Some(p), Some(e), Some(n), None)
                if [p, e, n].iter().all(|x| validate_name(x).is_ok()) =>
            {
                Ok(Self {
                    project: p,
                    environment: e,
                    entry: n,
                })
            }
            _ => Err(bad()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid() {
        let r = Reference::parse("op://app/prod/API_KEY").unwrap();
        assert_eq!(
            (r.project, r.environment, r.entry),
            ("app", "prod", "API_KEY")
        );
    }

    #[test]
    fn rejects_malformed() {
        for s in [
            "op://",
            "op://a/b",
            "op://a/b/",
            "op://a//c",
            "op://a/b/c/d",
            "http://a/b/c",
        ] {
            assert!(Reference::parse(s).is_err(), "{s}");
        }
    }

    #[test]
    fn names_outside_charset_rejected() {
        for s in ["op://a b/c/d", "op://a#b/c/d"] {
            assert_eq!(
                Reference::parse(s),
                Err(Error::BadReference(s.into())),
                "{s}"
            );
        }
        assert!(Reference::parse(&format!("op://{}/c/d", "a".repeat(129))).is_err());
        assert!(Reference::parse("op://my.app/dev-1/API_KEY").is_ok());
    }

    #[test]
    fn request_without_variables_still_parses() {
        let r: Request = serde_json::from_str(
            r#"{"version":1,"project_dir":"/x","command":["a"],"references":[]}"#,
        )
        .unwrap();
        assert!(r.variables.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn endpoint_prefers_env_override() {
        // SAFETY: only test touching this var.
        unsafe { std::env::set_var("ONEPASS_SOCKET", "/x/y") };
        assert_eq!(endpoint().unwrap(), std::path::PathBuf::from("/x/y"));
        unsafe { std::env::remove_var("ONEPASS_SOCKET") };
    }
}
