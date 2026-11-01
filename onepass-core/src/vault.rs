//! Vault file: `header | wrap_nonce | wrapped_data_key | body_nonce | body`.
//! The header (magic, Argon2id params, salt) is authenticated as AAD for both layers.
//! A random data key encrypts the body; the password-derived key only wraps the data key.
use crate::{
    Error,
    protocol::{PROTOCOL_VERSION, Reference, Request, Response, validate_name},
};
use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::{
    KeyInit, XChaCha20Poly1305, XNonce,
    aead::{Aead, OsRng, Payload, rand_core::RngCore},
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt, io::Write, path::Path};
use zeroize::{Zeroize, Zeroizing};

const MAGIC: &[u8; 8] = b"ONEPASS\x01";
const HEADER_LEN: usize = 8 + 12 + 16;
const NONCE_LEN: usize = 24;
const KEY_LEN: usize = 32;
const WRAPPED_LEN: usize = KEY_LEN + 16;
// Header is unauthenticated until the KEK is derived, so cap what it can make us allocate.
const MAX_M_COST: u32 = 1 << 20; // KiB
const MAX_T_COST: u32 = 16;
const MAX_P_COST: u32 = 16;

#[derive(Clone, Copy)]
pub struct KdfParams {
    pub m_cost: u32,
    pub t_cost: u32,
    pub p_cost: u32,
}

impl Default for KdfParams {
    fn default() -> Self {
        Self {
            m_cost: 64 * 1024,
            t_cost: 3,
            p_cost: 1,
        }
    }
}

fn default_type() -> String {
    "secret".into()
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Entry {
    /// Defaults to `secret`; leaves room for logins and notes without migration.
    #[serde(rename = "type", default = "default_type")]
    pub kind: String,
    pub value: Zeroizing<String>,
    #[serde(default)]
    pub notes: String,
}

impl Entry {
    pub fn secret(value: &str) -> Self {
        Self {
            kind: default_type(),
            value: Zeroizing::new(value.into()),
            notes: String::new(),
        }
    }
}

impl fmt::Debug for Entry {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Entry({}, <redacted>)", self.kind)
    }
}

/// project -> environment -> entry name -> entry
pub type Projects = BTreeMap<String, BTreeMap<String, BTreeMap<String, Entry>>>;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalMode {
    PerRun,
    #[default]
    TimeBoxed,
    Remember,
}

/// A remembered grant: the hash plus what the grants list needs to display it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct GrantRecord {
    pub hash: String,
    pub project_dir: String,
    pub command_name: String,
    pub references: Vec<String>,
    pub created_at: u64,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuditOutcome {
    Approved,
    ApprovedByGrant,
    Denied,
    TimedOut,
    Cancelled,
    RateLimited,
    Busy,
    Locked,
    EntryNotFound,
    Invalid,
}

/// Metadata only: never values.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AuditRecord {
    pub at: u64,
    pub project_dir: String,
    pub entries: Vec<String>,
    pub omitted: usize,
    pub outcome: AuditOutcome,
}

const MAX_AUDIT: usize = 1000;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    pub auto_lock_minutes: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            auto_lock_minutes: 15,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct EntryMeta {
    pub project: String,
    pub environment: String,
    pub name: String,
    pub kind: String,
    pub notes: String,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct Body {
    #[serde(default)]
    pub projects: Projects,
    #[serde(default)]
    pub remembered: Vec<GrantRecord>,
    /// Keyed by canonical project directory.
    #[serde(default)]
    pub project_modes: BTreeMap<String, ApprovalMode>,
    #[serde(default)]
    pub settings: Settings,
    #[serde(default)]
    pub audit: Vec<AuditRecord>,
}

fn invalid(m: &str) -> Error {
    Error::InvalidInput(m.into())
}

impl Body {
    pub fn create_environment(&mut self, project: &str, environment: &str) -> Result<(), Error> {
        validate_name(project)?;
        validate_name(environment)?;
        self.projects
            .entry(project.into())
            .or_default()
            .entry(environment.into())
            .or_default();
        Ok(())
    }

    /// `value: None` keeps the existing value. `rename_from` moves an existing entry.
    pub fn save_entry(
        &mut self,
        project: &str,
        environment: &str,
        name: &str,
        rename_from: Option<&str>,
        value: Option<Zeroizing<String>>,
        notes: String,
    ) -> Result<(), Error> {
        validate_name(project)?;
        validate_name(environment)?;
        validate_name(name)?;
        let old = rename_from.filter(|o| *o != name);
        let env = self.projects.get(project).and_then(|p| p.get(environment));
        if old.is_some() && env.is_some_and(|e| e.contains_key(name)) {
            return Err(invalid("an entry with that name already exists"));
        }
        let existing = env.and_then(|e| e.get(old.unwrap_or(name))).cloned();
        let mut entry = match (existing, value) {
            (_, Some(v)) => Entry {
                kind: default_type(),
                value: v,
                notes: String::new(),
            },
            (Some(e), None) => e,
            (None, None) => return Err(invalid("a new entry needs a value")),
        };
        entry.notes = notes;
        let env = self
            .projects
            .entry(project.into())
            .or_default()
            .entry(environment.into())
            .or_default();
        if let Some(o) = old {
            env.remove(o);
        }
        env.insert(name.into(), entry);
        Ok(())
    }

    pub fn delete_entry(
        &mut self,
        project: &str,
        environment: &str,
        name: &str,
    ) -> Result<(), Error> {
        self.projects
            .get_mut(project)
            .and_then(|p| p.get_mut(environment))
            .and_then(|e| e.remove(name))
            .map(|_| ())
            .ok_or_else(|| Error::EntryNotFound(format!("op://{project}/{environment}/{name}")))
    }

    pub fn delete_environment(&mut self, project: &str, environment: &str) -> Result<(), Error> {
        let p = self
            .projects
            .get_mut(project)
            .ok_or_else(|| invalid("no such project"))?;
        p.remove(environment)
            .ok_or_else(|| invalid("no such environment"))?;
        if p.is_empty() {
            self.projects.remove(project);
        }
        Ok(())
    }

    pub fn metadata(&self) -> Vec<EntryMeta> {
        let mut out = vec![];
        for (project, envs) in &self.projects {
            for (environment, entries) in envs {
                for (name, e) in entries {
                    out.push(EntryMeta {
                        project: project.clone(),
                        environment: environment.clone(),
                        name: name.clone(),
                        kind: e.kind.clone(),
                        notes: e.notes.clone(),
                    });
                }
            }
        }
        out
    }

    pub fn value(
        &self,
        project: &str,
        environment: &str,
        name: &str,
    ) -> Result<&Zeroizing<String>, Error> {
        self.projects
            .get(project)
            .and_then(|p| p.get(environment))
            .and_then(|e| e.get(name))
            .map(|e| &e.value)
            .ok_or_else(|| Error::EntryNotFound(format!("op://{project}/{environment}/{name}")))
    }

    pub fn push_audit(&mut self, record: AuditRecord) {
        self.audit.push(record);
        if self.audit.len() > MAX_AUDIT {
            self.audit.drain(..self.audit.len() - MAX_AUDIT);
        }
    }
}

pub struct Vault {
    pub body: Body,
    params: KdfParams,
    salt: [u8; 16],
    kek: Zeroizing<[u8; KEY_LEN]>,
    data_key: Zeroizing<[u8; KEY_LEN]>,
}

impl fmt::Debug for Vault {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("Vault(<redacted>)")
    }
}

fn derive(
    password: &[u8],
    salt: &[u8; 16],
    p: KdfParams,
) -> Result<Zeroizing<[u8; KEY_LEN]>, Error> {
    if p.m_cost > MAX_M_COST || p.t_cost > MAX_T_COST || p.p_cost > MAX_P_COST {
        return Err(Error::VaultCorrupt);
    }
    let params = Params::new(p.m_cost, p.t_cost, p.p_cost, Some(KEY_LEN))
        .map_err(|_| Error::VaultCorrupt)?;
    let mut out = Zeroizing::new([0u8; KEY_LEN]);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(password, salt, &mut *out)
        .map_err(|_| Error::VaultCorrupt)?;
    Ok(out)
}

fn random<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    OsRng.fill_bytes(&mut b);
    b
}

fn cipher(key: &[u8; KEY_LEN]) -> XChaCha20Poly1305 {
    XChaCha20Poly1305::new(key.into())
}

impl Vault {
    pub fn create(password: &[u8]) -> Result<Self, Error> {
        Self::create_with(password, KdfParams::default())
    }

    pub fn create_with(password: &[u8], params: KdfParams) -> Result<Self, Error> {
        let salt = random();
        Ok(Self {
            body: Body::default(),
            params,
            salt,
            kek: derive(password, &salt, params)?,
            data_key: Zeroizing::new(random()),
        })
    }

    /// Re-wraps the data key under a new password on the next `seal`; the body is not re-keyed.
    pub fn change_password(&mut self, new_password: &[u8]) -> Result<(), Error> {
        let salt = random();
        self.kek = derive(new_password, &salt, self.params)?;
        self.salt = salt;
        Ok(())
    }

    fn header(&self) -> [u8; HEADER_LEN] {
        let mut h = [0u8; HEADER_LEN];
        h[..8].copy_from_slice(MAGIC);
        h[8..12].copy_from_slice(&self.params.m_cost.to_le_bytes());
        h[12..16].copy_from_slice(&self.params.t_cost.to_le_bytes());
        h[16..20].copy_from_slice(&self.params.p_cost.to_le_bytes());
        h[20..].copy_from_slice(&self.salt);
        h
    }

    pub fn seal(&self) -> Result<Vec<u8>, Error> {
        let header = self.header();
        let (wn, bn) = (random::<NONCE_LEN>(), random::<NONCE_LEN>());
        let wrapped = cipher(&self.kek)
            .encrypt(
                XNonce::from_slice(&wn),
                Payload {
                    msg: &*self.data_key,
                    aad: &header,
                },
            )
            .map_err(|_| Error::VaultCorrupt)?;
        let plain =
            Zeroizing::new(serde_json::to_vec(&self.body).map_err(|e| Error::Io(e.to_string()))?);
        let ct = cipher(&self.data_key)
            .encrypt(
                XNonce::from_slice(&bn),
                Payload {
                    msg: &plain,
                    aad: &header,
                },
            )
            .map_err(|_| Error::VaultCorrupt)?;
        let mut out = Vec::with_capacity(HEADER_LEN + 2 * NONCE_LEN + WRAPPED_LEN + ct.len());
        for part in [&header[..], &wn, &wrapped, &bn, &ct] {
            out.extend_from_slice(part);
        }
        Ok(out)
    }

    pub fn open(bytes: &[u8], password: &[u8]) -> Result<Self, Error> {
        if bytes.len() < HEADER_LEN + 2 * NONCE_LEN + WRAPPED_LEN + 16 || &bytes[..8] != MAGIC {
            return Err(Error::VaultCorrupt);
        }
        let (header, rest) = bytes.split_at(HEADER_LEN);
        let u32_at = |i: usize| u32::from_le_bytes(header[i..i + 4].try_into().unwrap());
        let params = KdfParams {
            m_cost: u32_at(8),
            t_cost: u32_at(12),
            p_cost: u32_at(16),
        };
        let salt: [u8; 16] = header[20..].try_into().unwrap();
        let kek = derive(password, &salt, params)?;

        let (wn, rest) = rest.split_at(NONCE_LEN);
        let (wrapped, rest) = rest.split_at(WRAPPED_LEN);
        let (bn, ct) = rest.split_at(NONCE_LEN);
        // Failure here can't distinguish a wrong password from a tampered header or wrap.
        let mut dk = cipher(&kek)
            .decrypt(
                XNonce::from_slice(wn),
                Payload {
                    msg: wrapped,
                    aad: header,
                },
            )
            .map_err(|_| Error::WrongPassword)?;
        let data_key: Zeroizing<[u8; KEY_LEN]> =
            Zeroizing::new(dk.as_slice().try_into().map_err(|_| Error::VaultCorrupt)?);
        dk.zeroize();
        let plain = Zeroizing::new(
            cipher(&data_key)
                .decrypt(
                    XNonce::from_slice(bn),
                    Payload {
                        msg: ct,
                        aad: header,
                    },
                )
                .map_err(|_| Error::VaultCorrupt)?,
        );
        let body = serde_json::from_slice(&plain).map_err(|_| Error::VaultCorrupt)?;
        Ok(Self {
            body,
            params,
            salt,
            kek,
            data_key,
        })
    }

    /// Looks up every requested reference. Approval is the caller's job.
    pub fn resolve(&self, req: &Request) -> Response {
        if req.version != PROTOCOL_VERSION {
            return Response::Err(Error::VersionMismatch {
                expected: PROTOCOL_VERSION,
                got: req.version,
            });
        }
        let mut out = BTreeMap::new();
        for r in &req.references {
            let found = Reference::parse(r).and_then(|p| {
                self.body
                    .projects
                    .get(p.project)
                    .and_then(|e| e.get(p.environment))
                    .and_then(|n| n.get(p.entry))
                    .map(|e| e.value.to_string())
                    .ok_or_else(|| Error::EntryNotFound(r.clone()))
            });
            match found {
                Ok(v) => out.insert(r.clone(), v),
                Err(e) => return Response::Err(e),
            };
        }
        Response::Ok(out)
    }

    /// Atomic: temp file in the same directory, fsync, rename.
    pub fn save(&self, path: &Path) -> Result<(), Error> {
        let bytes = self.seal()?;
        let tmp = path.with_extension("tmp");
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut opts, 0o600);
        let mut f = opts.open(&tmp)?;
        f.write_all(&bytes)?;
        f.sync_all()?;
        // shortcut: directory fsync skipped, add it if crash-durability of the rename matters.
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    pub fn load(path: &Path, password: &[u8]) -> Result<Self, Error> {
        Self::open(&std::fs::read(path)?, password)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FAST: KdfParams = KdfParams {
        m_cost: 8,
        t_cost: 1,
        p_cost: 1,
    };

    fn sample() -> Vault {
        let mut v = Vault::create_with(b"pw", FAST).unwrap();
        v.body
            .projects
            .entry("app".into())
            .or_default()
            .entry("prod".into())
            .or_default()
            .insert("KEY".into(), Entry::secret("s3cret"));
        v
    }

    #[test]
    fn round_trip() {
        let v = Vault::open(&sample().seal().unwrap(), b"pw").unwrap();
        assert_eq!(*v.body.projects["app"]["prod"]["KEY"].value, "s3cret");
        assert_eq!(v.body.projects["app"]["prod"]["KEY"].kind, "secret");
    }

    #[test]
    fn wrong_password() {
        assert_eq!(
            Vault::open(&sample().seal().unwrap(), b"nope").unwrap_err(),
            Error::WrongPassword
        );
    }

    #[test]
    fn tamper_is_detected_everywhere() {
        let good = sample().seal().unwrap();
        for i in 0..good.len() {
            let mut b = good.clone();
            b[i] ^= 1;
            assert!(Vault::open(&b, b"pw").is_err(), "byte {i} flip accepted");
        }
        assert!(Vault::open(&good[..good.len() - 1], b"pw").is_err());
    }

    #[test]
    fn password_change_rewraps_only() {
        let mut v = sample();
        let old_key = *v.data_key;
        v.change_password(b"new").unwrap();
        let opened = Vault::open(&v.seal().unwrap(), b"new").unwrap();
        assert_eq!(*opened.data_key, old_key);
        assert_eq!(
            Vault::open(&v.seal().unwrap(), b"pw").unwrap_err(),
            Error::WrongPassword
        );
    }

    #[test]
    fn oversized_kdf_params_rejected() {
        let mut b = sample().seal().unwrap();
        b[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(Vault::open(&b, b"pw").unwrap_err(), Error::VaultCorrupt);
    }

    #[test]
    fn save_and_load() {
        let path = std::env::temp_dir().join(format!("onepass-test-{}.vault", std::process::id()));
        sample().save(&path).unwrap();
        assert!(Vault::load(&path, b"pw").is_ok());
        std::fs::remove_file(path).unwrap();
    }

    fn body_with(name: &str, v: &str) -> Body {
        let mut b = Body::default();
        b.save_entry(
            "p",
            "e",
            name,
            None,
            Some(Zeroizing::new(v.into())),
            String::new(),
        )
        .unwrap();
        b
    }

    #[test]
    fn save_entry_creates_path_and_keeps_value_when_none() {
        let mut b = body_with("A", "v");
        b.save_entry("p", "e", "A", None, None, "n2".into())
            .unwrap();
        assert_eq!(**b.value("p", "e", "A").unwrap(), "v");
        assert_eq!(b.projects["p"]["e"]["A"].notes, "n2");
    }

    #[test]
    fn save_entry_rename_moves_and_refuses_collision() {
        let mut b = body_with("A", "1");
        b.save_entry("p", "e", "B", Some("A"), None, String::new())
            .unwrap();
        assert!(b.value("p", "e", "A").is_err());
        assert_eq!(**b.value("p", "e", "B").unwrap(), "1");

        let mut b = body_with("A", "1");
        b.save_entry(
            "p",
            "e",
            "B",
            None,
            Some(Zeroizing::new("2".into())),
            String::new(),
        )
        .unwrap();
        let before = serde_json::to_string(&b).unwrap();
        let err = b.save_entry("p", "e", "B", Some("A"), None, String::new());
        assert!(matches!(err, Err(Error::InvalidInput(_))));
        assert_eq!(serde_json::to_string(&b).unwrap(), before);
    }

    #[test]
    fn save_entry_rejects_bad_names() {
        let mut b = Body::default();
        for n in ["a/b", ""] {
            let r = b.save_entry(
                "p",
                "e",
                n,
                None,
                Some(Zeroizing::new("v".into())),
                String::new(),
            );
            assert!(matches!(r, Err(Error::InvalidInput(_))), "{n}");
        }
        assert!(b.projects.is_empty());
    }

    #[test]
    fn delete_environment_prunes_empty_project() {
        let mut b = body_with("A", "1");
        b.delete_environment("p", "e").unwrap();
        assert!(b.projects.is_empty());
    }

    #[test]
    fn metadata_has_no_values() {
        let b = body_with("A", "s3cret");
        let json = serde_json::to_string(&b.metadata()).unwrap();
        assert!(!json.contains("s3cret") && json.contains("\"A\""));
    }

    #[test]
    fn audit_is_capped_at_1000() {
        let mut b = Body::default();
        for i in 0..1001 {
            b.push_audit(AuditRecord {
                at: i,
                project_dir: String::new(),
                entries: vec![],
                omitted: 0,
                outcome: AuditOutcome::Approved,
            });
        }
        assert_eq!((b.audit.len(), b.audit[0].at), (1000, 1));
    }

    #[test]
    fn body_without_new_fields_opens() {
        let b: Body = serde_json::from_str(r#"{"projects":{}}"#).unwrap();
        assert_eq!(b.settings.auto_lock_minutes, 15);
    }

    proptest::proptest! {
        #[test]
        fn prop_seal_open_round_trip(
            n in "[A-Za-z0-9._-]{1,16}", val in ".{0,64}", notes in ".{0,32}",
        ) {
            let mut v = Vault::create_with(b"pw", FAST).unwrap();
            v.body.save_entry("p", "e", &n, None, Some(Zeroizing::new(val.clone())), notes).unwrap();
            let o = Vault::open(&v.seal().unwrap(), b"pw").unwrap();
            proptest::prop_assert_eq!(&**o.body.value("p", "e", &n).unwrap(), &val);
        }

        #[test]
        fn prop_open_never_panics(bytes in proptest::collection::vec(0u8..=255, 0..200), cut in 0usize..400) {
            let _ = Vault::open(&bytes, b"pw");
            let good = sample().seal().unwrap();
            let _ = Vault::open(&good[..cut.min(good.len())], b"pw");
        }
    }
}
