//! Vault lifecycle. Memory always equals disk: a failed save restores the previous body.
use crate::broker::grants::{ActiveGrant, GrantDuration, GrantStore};
use onepass_core::{
    Error,
    vault::{ApprovalMode, AuditRecord, Body, GrantRecord, KdfParams, Vault},
};
use serde::Serialize;
use std::{
    path::PathBuf,
    sync::{Arc, Condvar, Mutex, MutexGuard},
    time::{Duration, SystemTime},
};

pub type Cancel = Arc<dyn Fn() -> bool + Send + Sync>;

const MIN_PASSWORD: usize = 8;
const MAX_PENDING_AUDIT: usize = 1000;
const POLL: Duration = Duration::from_millis(100);

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VaultStatus {
    Missing,
    Locked,
    Unlocked,
}

#[derive(Default)]
struct Inner {
    vault: Option<Vault>,
    timeboxed: GrantStore,
    /// Audit records made while locked; lost if the app quits before the next unlock.
    pending_audit: Vec<AuditRecord>,
}

pub struct Session {
    path: PathBuf,
    kdf: KdfParams,
    inner: Mutex<Inner>,
    unlocked: Condvar,
}

/// Run `f` on the body and save; restore the old body if either step fails.
fn apply<T>(
    v: &mut Vault,
    path: &std::path::Path,
    f: impl FnOnce(&mut Body) -> Result<T, Error>,
) -> Result<T, Error> {
    let backup = v.body.clone();
    let out = f(&mut v.body).and_then(|t| v.save(path).map(|()| t));
    if out.is_err() {
        v.body = backup;
    }
    out
}

fn check_password(p: &[u8]) -> Result<(), Error> {
    if p.len() < MIN_PASSWORD {
        return Err(Error::InvalidInput(format!(
            "password must be at least {MIN_PASSWORD} characters"
        )));
    }
    Ok(())
}

impl Session {
    pub fn new(path: PathBuf, kdf: KdfParams) -> Self {
        Self {
            path,
            kdf,
            inner: Mutex::default(),
            unlocked: Condvar::new(),
        }
    }

    fn inner(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn status(&self) -> VaultStatus {
        if self.inner().vault.is_some() {
            VaultStatus::Unlocked
        } else if self.path.exists() {
            VaultStatus::Locked
        } else {
            VaultStatus::Missing
        }
    }

    /// Install an opened vault and flush audit records buffered while locked.
    fn install(&self, mut v: Vault) {
        let mut g = self.inner();
        let pending = std::mem::take(&mut g.pending_audit);
        if !pending.is_empty() {
            // A failed save drops the buffered records but keeps the vault usable.
            let _ = apply(&mut v, &self.path, |b| {
                pending.into_iter().for_each(|r| b.push_audit(r));
                Ok(())
            });
        }
        g.vault = Some(v);
        self.unlocked.notify_all();
    }

    pub fn create(&self, password: &[u8]) -> Result<(), Error> {
        check_password(password)?;
        if self.path.exists() {
            return Err(Error::InvalidInput("a vault file already exists".into()));
        }
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let v = Vault::create_with(password, self.kdf)?;
        v.save(&self.path)?;
        self.install(v);
        Ok(())
    }

    pub fn unlock(&self, password: &[u8]) -> Result<(), Error> {
        let v = Vault::load(&self.path, password)?;
        self.install(v);
        Ok(())
    }

    pub fn lock(&self) {
        let mut g = self.inner();
        g.vault = None;
        g.timeboxed.clear();
    }

    pub fn change_password(&self, current: &[u8], new: &[u8]) -> Result<(), Error> {
        check_password(new)?;
        let mut g = self.inner();
        if g.vault.is_none() {
            return Err(Error::VaultLocked);
        }
        // Re-open from disk to verify `current`; memory is only swapped after a successful save.
        let mut v = Vault::load(&self.path, current)?;
        v.change_password(new)?;
        v.save(&self.path)?;
        g.vault = Some(v);
        Ok(())
    }

    pub fn read<T>(&self, f: impl FnOnce(&Vault) -> T) -> Result<T, Error> {
        self.inner().vault.as_ref().map(f).ok_or(Error::VaultLocked)
    }

    pub fn mutate<T>(&self, f: impl FnOnce(&mut Body) -> Result<T, Error>) -> Result<T, Error> {
        let mut g = self.inner();
        let v = g.vault.as_mut().ok_or(Error::VaultLocked)?;
        apply(v, &self.path, f)
    }

    /// True once unlocked; false on timeout or when `cancel` fires. Polls so `cancel` is honoured.
    pub fn wait_unlocked(&self, timeout: Duration, cancel: &Cancel) -> bool {
        let deadline = std::time::Instant::now() + timeout;
        let mut g = self.inner();
        loop {
            if g.vault.is_some() {
                return true;
            }
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            if left.is_zero() || cancel() {
                return false;
            }
            g = self
                .unlocked
                .wait_timeout(g, left.min(POLL))
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
    }

    pub fn is_granted(&self, key: &str, now: SystemTime) -> bool {
        let g = self.inner();
        g.vault.as_ref().is_some_and(|v| {
            g.timeboxed.is_granted(key, now) || v.body.remembered.iter().any(|r| r.hash == key)
        })
    }

    pub fn add_timeboxed(&self, record: GrantRecord, duration: GrantDuration, now: SystemTime) {
        self.inner().timeboxed.insert(record, duration, now);
    }

    pub fn remember(&self, record: GrantRecord) -> Result<(), Error> {
        self.mutate(|b| {
            if !b.remembered.iter().any(|r| r.hash == record.hash) {
                b.remembered.push(record);
            }
            Ok(())
        })
    }

    pub fn revoke_grant(&self, hash: &str) -> Result<(), Error> {
        self.mutate(|b| {
            b.remembered.retain(|r| r.hash != hash);
            Ok(())
        })?;
        self.inner().timeboxed.revoke(hash);
        Ok(())
    }

    pub fn list_grants(&self, now: SystemTime) -> Result<Vec<ActiveGrant>, Error> {
        let g = self.inner();
        let v = g.vault.as_ref().ok_or(Error::VaultLocked)?;
        let mut out: Vec<ActiveGrant> = v
            .body
            .remembered
            .iter()
            .map(|r| ActiveGrant {
                record: r.clone(),
                expires_at: None,
            })
            .collect();
        out.extend(g.timeboxed.list(now));
        Ok(out)
    }

    pub fn project_mode(&self, dir: &str) -> Result<ApprovalMode, Error> {
        self.read(|v| v.body.project_modes.get(dir).copied().unwrap_or_default())
    }

    /// A change of mode revokes the project's grants; setting the current mode keeps them.
    pub fn set_project_mode(&self, dir: &str, mode: ApprovalMode) -> Result<(), Error> {
        if self.project_mode(dir)? == mode {
            return Ok(());
        }
        self.mutate(|b| {
            b.project_modes.insert(dir.into(), mode);
            b.remembered.retain(|r| r.project_dir != dir);
            Ok(())
        })?;
        self.inner().timeboxed.revoke_project(dir);
        Ok(())
    }

    /// Never fails the caller: a save error just loses the record.
    pub fn record_audit(&self, record: AuditRecord) {
        let mut g = self.inner();
        let Inner {
            vault,
            pending_audit,
            ..
        } = &mut *g;
        match vault {
            Some(v) => {
                let _ = apply(v, &self.path, |b| {
                    b.push_audit(record);
                    Ok(())
                });
            }
            None => {
                if pending_audit.len() >= MAX_PENDING_AUDIT {
                    pending_audit.remove(0);
                }
                pending_audit.push(record);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zeroize::Zeroizing;
    use crate::broker::grants::command_name;
    use onepass_core::vault::{AuditOutcome, GrantRecord};
    use std::sync::atomic::{AtomicBool, Ordering};

    const FAST: KdfParams = KdfParams {
        m_cost: 8,
        t_cost: 1,
        p_cost: 1,
    };
    const PW: &[u8] = b"password1";

    fn new(tag: &str) -> (Session, PathBuf) {
        let dir = std::env::temp_dir().join(format!("op-session-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("v").join("vault.onepass");
        (Session::new(path.clone(), FAST), path)
    }

    fn unlocked(tag: &str) -> (Session, PathBuf) {
        let (s, p) = new(tag);
        s.create(PW).unwrap();
        (s, p)
    }

    fn add(s: &Session, name: &str) -> Result<(), Error> {
        s.mutate(|b| {
            b.save_entry(
                "p",
                "e",
                name,
                None,
                Some(Zeroizing::new("v".into())),
                String::new(),
            )
        })
    }

    fn has(s: &Session, name: &str) -> bool {
        s.read(|v| v.body.value("p", "e", name).is_ok()).unwrap()
    }

    fn rec(hash: &str, dir: &str) -> GrantRecord {
        GrantRecord {
            hash: hash.into(),
            project_dir: dir.into(),
            command_name: command_name(&["node".into()]).unwrap(),
            references: vec![],
            created_at: 0,
        }
    }

    fn never() -> Cancel {
        Arc::new(|| false)
    }

    #[test]
    fn status_transitions() {
        let (s, _) = new("status");
        assert_eq!(s.status(), VaultStatus::Missing);
        s.create(PW).unwrap();
        assert_eq!(s.status(), VaultStatus::Unlocked);
        s.lock();
        assert_eq!(s.status(), VaultStatus::Locked);
        s.unlock(PW).unwrap();
        assert_eq!(s.status(), VaultStatus::Unlocked);
    }

    #[test]
    fn create_refuses_existing_file() {
        let (s, p) = new("existing");
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, b"garbage").unwrap();
        assert!(matches!(s.create(PW), Err(Error::InvalidInput(_))));
        assert_eq!(std::fs::read(&p).unwrap(), b"garbage");
        assert_eq!(s.status(), VaultStatus::Locked);
        assert_eq!(s.unlock(PW).unwrap_err(), Error::VaultCorrupt);
    }

    #[test]
    fn create_refuses_short_password() {
        let (s, _) = new("short");
        assert!(matches!(s.create(b"1234567"), Err(Error::InvalidInput(_))));
        assert_eq!(s.status(), VaultStatus::Missing);
    }

    #[test]
    fn read_and_mutate_when_locked_are_typed() {
        let (s, _) = unlocked("locked");
        s.lock();
        assert_eq!(s.read(|_| ()).unwrap_err(), Error::VaultLocked);
        assert_eq!(s.mutate(|_| Ok(())).unwrap_err(), Error::VaultLocked);
    }

    #[test]
    fn mutate_persists() {
        let (s, _) = unlocked("persist");
        add(&s, "A").unwrap();
        s.lock();
        s.unlock(PW).unwrap();
        assert!(has(&s, "A"));
    }

    #[test]
    fn failed_save_rolls_back_memory() {
        let (s, p) = unlocked("rollback");
        add(&s, "A").unwrap();
        let tmp = p.with_extension("tmp");
        std::fs::create_dir(&tmp).unwrap();
        assert!(matches!(add(&s, "B"), Err(Error::Io(_))));
        assert!(has(&s, "A") && !has(&s, "B"));
        std::fs::remove_dir(&tmp).unwrap();
        add(&s, "B").unwrap();
        assert!(has(&s, "B"));
    }

    #[test]
    fn failed_change_password_keeps_old() {
        let (s, p) = unlocked("chpw-fail");
        let tmp = p.with_extension("tmp");
        std::fs::create_dir(&tmp).unwrap();
        assert!(s.change_password(PW, b"newpassword").is_err());
        s.lock();
        assert_eq!(s.unlock(b"newpassword").unwrap_err(), Error::WrongPassword);
        s.unlock(PW).unwrap();
    }

    #[test]
    fn change_password_works_and_requires_current() {
        let (s, _) = unlocked("chpw");
        assert_eq!(
            s.change_password(b"wrongwrong", b"newpassword")
                .unwrap_err(),
            Error::WrongPassword
        );
        assert!(matches!(
            s.change_password(PW, b"short"),
            Err(Error::InvalidInput(_))
        ));
        s.change_password(PW, b"newpassword").unwrap();
        s.lock();
        assert_eq!(s.unlock(PW).unwrap_err(), Error::WrongPassword);
        s.unlock(b"newpassword").unwrap();
    }

    #[test]
    fn lock_clears_timeboxed_grants_and_keeps_remembered() {
        let (s, _) = unlocked("grants");
        let now = SystemTime::now();
        s.add_timeboxed(rec("t", "/p"), GrantDuration::UntilLock, now);
        s.remember(rec("r", "/p")).unwrap();
        assert!(s.is_granted("t", now) && s.is_granted("r", now));
        s.lock();
        assert!(!s.is_granted("r", now));
        s.unlock(PW).unwrap();
        assert!(!s.is_granted("t", now) && s.is_granted("r", now));
        let l = s.list_grants(now).unwrap();
        assert_eq!((l.len(), l[0].expires_at), (1, None));
        s.revoke_grant("r").unwrap();
        assert!(!s.is_granted("r", now));
    }

    #[test]
    fn set_project_mode_revokes_project_grants() {
        let (s, _) = unlocked("modes");
        let now = SystemTime::now();
        s.add_timeboxed(rec("t", "/p"), GrantDuration::UntilLock, now);
        s.add_timeboxed(rec("o", "/other"), GrantDuration::UntilLock, now);
        s.remember(rec("r", "/p")).unwrap();
        assert_eq!(s.project_mode("/p").unwrap(), ApprovalMode::TimeBoxed);
        s.set_project_mode("/p", ApprovalMode::PerRun).unwrap();
        assert!(!s.is_granted("t", now) && !s.is_granted("r", now) && s.is_granted("o", now));
        assert_eq!(s.project_mode("/p").unwrap(), ApprovalMode::PerRun);
    }

    #[test]
    fn audit_while_locked_is_flushed_on_unlock() {
        let (s, _) = unlocked("audit");
        s.lock();
        s.record_audit(AuditRecord {
            at: 1,
            project_dir: "/p".into(),
            entries: vec![],
            omitted: 0,
            outcome: AuditOutcome::Locked,
        });
        s.unlock(PW).unwrap();
        assert_eq!(s.read(|v| v.body.audit.len()).unwrap(), 1);
        s.lock();
        s.unlock(PW).unwrap();
        assert_eq!(s.read(|v| v.body.audit.len()).unwrap(), 1);
    }

    #[test]
    fn wait_unlocked_wakes_on_unlock() {
        let (s, _) = unlocked("wake");
        let s = Arc::new(s);
        s.lock();
        let s2 = s.clone();
        let h = std::thread::spawn(move || s2.wait_unlocked(Duration::from_secs(5), &never()));
        std::thread::sleep(Duration::from_millis(100));
        s.unlock(PW).unwrap();
        assert!(h.join().unwrap());
    }

    #[test]
    fn wait_unlocked_times_out() {
        let (s, _) = unlocked("timeout");
        s.lock();
        assert!(!s.wait_unlocked(Duration::from_millis(150), &never()));
    }

    #[test]
    fn wait_unlocked_stops_on_cancel() {
        let (s, _) = unlocked("cancel");
        s.lock();
        let flag = Arc::new(AtomicBool::new(false));
        let f = flag.clone();
        let cancel: Cancel = Arc::new(move || f.load(Ordering::SeqCst));
        flag.store(true, Ordering::SeqCst);
        let t = std::time::Instant::now();
        assert!(!s.wait_unlocked(Duration::from_secs(5), &cancel));
        assert!(t.elapsed() < Duration::from_secs(2));
    }
}
