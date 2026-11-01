use onepass_core::vault::GrantRecord;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    time::{Duration, SystemTime},
};

const DENIAL_WINDOW: Duration = Duration::from_secs(60);
const DENIAL_LIMIT: usize = 3;

/// File name of the executable, ignoring its directory and arguments.
pub fn command_name(command: &[String]) -> Option<String> {
    let first = command.first()?;
    let name = first.rsplit(['/', '\\']).next().unwrap_or(first);
    (!name.is_empty()).then(|| name.to_string())
}

/// Lowercase hex SHA-256 over length-prefixed fields: dir, exec name, sorted unique references.
pub fn grant_key(project_dir: &str, references: &[String], command: &[String]) -> String {
    let mut h = Sha256::new();
    let mut field = |b: &[u8]| {
        h.update((b.len() as u64).to_le_bytes());
        h.update(b);
    };
    field(project_dir.as_bytes());
    field(command_name(command).unwrap_or_default().as_bytes());
    let mut refs: Vec<&String> = references.iter().collect();
    refs.sort();
    refs.dedup();
    for r in refs {
        field(r.as_bytes());
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GrantDuration {
    Min15,
    Hour1,
    UntilLock,
}

#[derive(Clone, Debug)]
pub struct ActiveGrant {
    pub record: GrantRecord,
    pub expires_at: Option<SystemTime>,
}

/// Wall-clock expiry, so a grant also expires while the machine sleeps.
/// A clock that moved backwards counts as not expired.
fn expired(g: &ActiveGrant, now: SystemTime) -> bool {
    g.expires_at.is_some_and(|e| now.duration_since(e).is_ok())
}

#[derive(Default)]
pub struct GrantStore(BTreeMap<String, ActiveGrant>);

impl GrantStore {
    pub fn is_granted(&self, key: &str, now: SystemTime) -> bool {
        self.0.get(key).is_some_and(|g| !expired(g, now))
    }

    pub fn insert(&mut self, record: GrantRecord, duration: GrantDuration, now: SystemTime) {
        let expires_at = match duration {
            GrantDuration::Min15 => Some(now + 15 * Duration::from_secs(60)),
            GrantDuration::Hour1 => Some(now + 60 * Duration::from_secs(60)),
            GrantDuration::UntilLock => None,
        };
        self.0
            .insert(record.hash.clone(), ActiveGrant { record, expires_at });
    }

    pub fn revoke(&mut self, hash: &str) -> bool {
        self.0.remove(hash).is_some()
    }

    pub fn revoke_project(&mut self, project_dir: &str) {
        self.0.retain(|_, g| g.record.project_dir != project_dir);
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }

    pub fn list(&self, now: SystemTime) -> Vec<ActiveGrant> {
        self.0
            .values()
            .filter(|g| !expired(g, now))
            .cloned()
            .collect()
    }
}

/// Sliding window of denials per project directory.
#[derive(Default)]
pub struct DenialTracker(HashMap<String, VecDeque<SystemTime>>);

fn recent(t: SystemTime, now: SystemTime) -> bool {
    now.duration_since(t)
        .map_or(true, |age| age < DENIAL_WINDOW)
}

impl DenialTracker {
    pub fn record(&mut self, project_dir: &str, now: SystemTime) {
        let q = self.0.entry(project_dir.into()).or_default();
        q.retain(|t| recent(*t, now));
        q.push_back(now);
    }

    pub fn is_limited(&self, project_dir: &str, now: SystemTime) -> bool {
        self.0
            .get(project_dir)
            .is_some_and(|q| q.iter().filter(|t| recent(**t, now)).count() >= DENIAL_LIMIT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime};

    const MIN: Duration = Duration::from_secs(60);

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    fn t0() -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000)
    }

    fn rec(hash: &str, dir: &str) -> GrantRecord {
        GrantRecord {
            hash: hash.into(),
            project_dir: dir.into(),
            command_name: "node".into(),
            references: s(&["op://a/b/c"]),
            created_at: 0,
        }
    }

    #[test]
    fn key_ignores_reference_order_and_duplicates() {
        let k = |r: &[&str]| grant_key("/p", &s(r), &s(&["node"]));
        assert_eq!(k(&["a", "b"]), k(&["b", "a"]));
        assert_eq!(k(&["a", "b"]), k(&["a", "b", "a"]));
    }

    #[test]
    fn key_changes_with_each_input() {
        let base = grant_key("/p", &s(&["a"]), &s(&["node"]));
        assert_ne!(base, grant_key("/q", &s(&["a"]), &s(&["node"])));
        assert_ne!(base, grant_key("/p", &s(&["a", "b"]), &s(&["node"])));
        assert_ne!(base, grant_key("/p", &s(&["a"]), &s(&["python"])));
        assert_eq!(base, grant_key("/p", &s(&["a"]), &s(&["/usr/bin/node"])));
        assert_eq!(base, grant_key("/p", &s(&["a"]), &s(&["node", "--flag"])));
    }

    #[test]
    fn key_fields_are_unambiguous() {
        let k = |r: &[&str]| grant_key("/p", &s(r), &s(&["node"]));
        assert_ne!(k(&["ab", "c"]), k(&["a", "bc"]));
    }

    #[test]
    fn min15_expires_by_wall_clock() {
        let mut g = GrantStore::default();
        g.insert(rec("h", "/p"), GrantDuration::Min15, t0());
        assert!(g.is_granted("h", t0() + 14 * MIN));
        assert!(!g.is_granted("h", t0() + 15 * MIN));
    }

    #[test]
    fn grant_expired_after_sleep() {
        let mut g = GrantStore::default();
        g.insert(rec("h", "/p"), GrantDuration::Hour1, t0());
        let later = t0() + 8 * 60 * MIN;
        assert!(!g.is_granted("h", later));
        assert!(g.list(later).is_empty());
    }

    #[test]
    fn until_lock_lasts_until_clear() {
        let mut g = GrantStore::default();
        g.insert(rec("h", "/p"), GrantDuration::UntilLock, t0());
        assert!(g.is_granted("h", t0() + 30 * 24 * 60 * MIN));
        g.clear();
        assert!(!g.is_granted("h", t0()));
    }

    #[test]
    fn revoke_project_only_touches_that_project() {
        let mut g = GrantStore::default();
        g.insert(rec("a", "/p"), GrantDuration::UntilLock, t0());
        g.insert(rec("b", "/q"), GrantDuration::UntilLock, t0());
        g.revoke_project("/p");
        assert!(!g.is_granted("a", t0()) && g.is_granted("b", t0()));
        assert!(g.revoke("b") && !g.revoke("b"));
    }

    #[test]
    fn third_denial_limits_within_a_minute() {
        let mut d = DenialTracker::default();
        let sec = Duration::from_secs(1);
        d.record("/p", t0());
        d.record("/p", t0() + sec);
        assert!(!d.is_limited("/p", t0() + sec));
        d.record("/p", t0() + 2 * sec);
        assert!(d.is_limited("/p", t0() + 59 * sec));
        assert!(!d.is_limited("/p", t0() + 61 * sec));
    }

    #[test]
    fn limit_is_per_project() {
        let mut d = DenialTracker::default();
        for _ in 0..3 {
            d.record("/p", t0());
        }
        assert!(d.is_limited("/p", t0()) && !d.is_limited("/q", t0()));
    }
}
