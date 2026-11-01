use crate::Error;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const PROTOCOL_VERSION: u32 = 1;
pub const REF_PREFIX: &str = "op://";

/// CLI -> app. Never carries secrets. `command` and `project_dir` are client-claimed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Request {
    pub version: u32,
    pub project_dir: String,
    pub command: Vec<String>,
    pub references: Vec<String>,
}

/// App -> CLI. On success maps each requested reference to its value.
#[derive(Serialize, Deserialize)]
pub enum Response {
    Ok(BTreeMap<String, String>),
    Err(Error),
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
            (Some(p), Some(e), Some(n), None) if ![p, e, n].iter().any(|x| x.is_empty()) => {
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
}
