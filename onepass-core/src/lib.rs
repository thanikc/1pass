pub mod protocol;
#[cfg(feature = "vault")]
pub mod vault;

use serde::{Deserialize, Serialize};

/// Typed error shared by every boundary (vault, IPC, CLI). Carries no secrets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
pub enum Error {
    #[error("wrong password")]
    WrongPassword,
    #[error("vault is corrupt or tampered with")]
    VaultCorrupt,
    #[error("vault is locked")]
    VaultLocked,
    #[error("request denied")]
    Denied,
    #[error("entry not found: {0}")]
    EntryNotFound(String),
    #[error("protocol version mismatch (expected {expected}, got {got})")]
    VersionMismatch { expected: u32, got: u32 },
    #[error("invalid reference: {0}")]
    BadReference(String),
    #[error("io: {0}")]
    Io(String),
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e.to_string())
    }
}
