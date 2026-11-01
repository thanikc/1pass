//! Request pipeline: validate, rate-limit, unlock, grant check, approval, resolve, audit.
//! Tauri-free: the UI is reached through the `Host` trait.
pub mod grants;
pub mod view;

use crate::session::{Cancel, Session};
use grants::{DenialTracker, GrantDuration, command_name, grant_key};
use onepass_core::{
    Error,
    protocol::{PROTOCOL_VERSION, Reference, Request, Response},
    vault::{ApprovalMode, AuditOutcome, AuditRecord, GrantRecord},
};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Condvar, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use view::PromptView;

pub const MAX_PENDING: usize = 8;
pub const MAX_AUDIT_ENTRIES: usize = 64;
const MAX_AUDIT_DIR: usize = 1024;
const MAX_AUDIT_ENTRY: usize = 256;
const POLL: Duration = Duration::from_millis(100);

#[derive(Clone, Debug, Default)]
pub struct Peer {
    pub pid: Option<u32>,
    pub exe: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    Allow {
        mode: ApprovalMode,
        duration: GrantDuration,
    },
    Deny,
    TimedOut,
    Cancelled,
}

pub trait Host: Send + Sync + 'static {
    /// Bring the unlock screen to the front.
    fn request_unlock(&self);
    /// Block until the user decides, `timeout` passes, or `cancel` turns true.
    fn prompt(&self, view: PromptView, timeout: Duration, cancel: Cancel) -> Decision;
}

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub unlock_wait: Duration,
    pub approval_wait: Duration,
    pub request_deadline: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            unlock_wait: Duration::from_secs(60),
            approval_wait: Duration::from_secs(120),
            request_deadline: Duration::from_secs(180),
        }
    }
}

#[derive(Default)]
struct State {
    denials: DenialTracker,
    /// Requests waiting for or holding the single prompt slot.
    pending: usize,
    prompting: bool,
}

pub struct Broker {
    session: Arc<Session>,
    host: Arc<dyn Host>,
    limits: Limits,
    state: Mutex<State>,
    gate: Condvar,
}

type Fail = (Error, AuditOutcome);
type Outcome = Result<(BTreeMap<String, String>, AuditOutcome), Fail>;

enum Admit<'a> {
    ByGrant,
    /// Holds the queue slot until the grant is stored, so queued twins see it.
    Allowed {
        mode: ApprovalMode,
        duration: GrantDuration,
        _slot: Slot<'a>,
    },
}

/// Frees the prompt queue slot on every exit path.
struct Slot<'a> {
    broker: &'a Broker,
    prompting: bool,
}

impl Drop for Slot<'_> {
    fn drop(&mut self) {
        let mut st = self.broker.state();
        st.pending -= 1;
        if self.prompting {
            st.prompting = false;
        }
        self.broker.gate.notify_all();
    }
}

fn unix(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

fn left(deadline: Instant) -> Duration {
    deadline.saturating_duration_since(Instant::now())
}

impl Broker {
    pub fn new(session: Arc<Session>, host: Arc<dyn Host>, limits: Limits) -> Self {
        Self {
            session,
            host,
            limits,
            state: Mutex::default(),
            gate: Condvar::new(),
        }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    #[cfg(test)]
    fn pending(&self) -> usize {
        self.state().pending
    }

    pub fn handle(&self, raw: &[u8], peer: Peer, client_gone: Cancel) -> Response {
        let mut req = match parse(raw) {
            Ok(r) => r,
            Err(e) => return Response::Err(e),
        };
        // Not audited: cheap invalid requests would otherwise force a vault save each.
        if let Err(e) = validate(&mut req) {
            return Response::Err(e);
        }
        if req.references.is_empty() {
            return Response::Ok(BTreeMap::new());
        }
        let (resp, outcome) = match self.process(&req, &peer, &client_gone) {
            Ok((values, outcome)) => (Response::Ok(values), outcome),
            Err((e, outcome)) => (Response::Err(e), outcome),
        };
        // Not persisted: floods of these would evict real history and force a save each.
        if !matches!(outcome, AuditOutcome::RateLimited | AuditOutcome::Busy) {
            self.audit(&req, outcome);
        }
        resp
    }

    fn process(&self, req: &Request, peer: &Peer, gone: &Cancel) -> Outcome {
        let deadline = Instant::now() + self.limits.request_deadline;
        let dir = req.project_dir.as_str();
        let key = grant_key(dir, &req.references, &req.command);

        if self.state().denials.is_limited(dir, SystemTime::now()) {
            return Err((Error::RateLimited, AuditOutcome::RateLimited));
        }
        if !self.session.is_unlocked() {
            self.host.request_unlock();
            let wait = self.limits.unlock_wait.min(left(deadline));
            if !self.session.wait_unlocked(wait, gone) {
                let outcome = if gone() {
                    AuditOutcome::Cancelled
                } else {
                    AuditOutcome::Locked
                };
                return Err((Error::VaultLocked, outcome));
            }
        }

        let (outcome, chosen) = match self.admit(req, peer, &key, deadline, gone)? {
            Admit::ByGrant => (AuditOutcome::ApprovedByGrant, None),
            Admit::Allowed {
                mode,
                duration,
                _slot,
            } => (AuditOutcome::Approved, Some((mode, duration, _slot))),
        };

        // Lookup only after approval, so entry names cannot be probed without a prompt.
        let values = match self.session.read(|v| v.resolve(req)) {
            Ok(Response::Ok(values)) => values,
            Ok(Response::Err(e)) => return Err(fail(e)),
            Err(e) => return Err(fail(e)),
        };
        if let Some((mode, duration, _slot)) = chosen {
            // A grant that cannot be stored just means the next run prompts again.
            let _ = self.store_grant(req, key, mode, duration);
        }
        Ok((values, outcome))
    }

    fn mode(&self, dir: &str) -> Result<ApprovalMode, Fail> {
        self.session.project_mode(dir).map_err(fail)
    }

    fn admit(
        &self,
        req: &Request,
        peer: &Peer,
        key: &str,
        deadline: Instant,
        gone: &Cancel,
    ) -> Result<Admit<'_>, Fail> {
        let dir = req.project_dir.as_str();
        let granted = |mode: ApprovalMode| {
            mode != ApprovalMode::PerRun && self.session.is_granted(key, SystemTime::now())
        };

        let mode = self.mode(dir)?;
        if granted(mode) {
            return Ok(Admit::ByGrant);
        }
        {
            let mut st = self.state();
            if st.pending >= MAX_PENDING {
                return Err((Error::Busy, AuditOutcome::Busy));
            }
            st.pending += 1;
        }
        let mut slot = Slot {
            broker: self,
            prompting: false,
        };
        let mut st = self.state();
        // Queue for the single prompt; a request that waited re-checks limit and grant on its turn.
        let mode = loop {
            if gone() {
                return Err((Error::Denied, AuditOutcome::Cancelled));
            }
            let remaining = left(deadline);
            if remaining.is_zero() {
                return Err((Error::Denied, AuditOutcome::TimedOut));
            }
            if st.denials.is_limited(dir, SystemTime::now()) {
                return Err((Error::RateLimited, AuditOutcome::RateLimited));
            }
            let mode = self.mode(dir)?;
            if granted(mode) {
                return Ok(Admit::ByGrant);
            }
            if !st.prompting {
                st.prompting = true;
                slot.prompting = true;
                break mode;
            }
            st = self
                .gate
                .wait_timeout(st, remaining.min(POLL))
                .unwrap_or_else(|e| e.into_inner())
                .0;
        };
        drop(st);

        let (session, g) = (self.session.clone(), gone.clone());
        let cancel: Cancel = Arc::new(move || g() || !session.is_unlocked());
        let timeout = self.limits.approval_wait.min(left(deadline));
        match self
            .host
            .prompt(PromptView::new(req, peer, mode), timeout, cancel)
        {
            Decision::Allow { mode, duration } => Ok(Admit::Allowed {
                mode,
                duration,
                _slot: slot,
            }),
            Decision::Deny => Err(self.denied(dir, AuditOutcome::Denied)),
            Decision::TimedOut => Err(self.denied(dir, AuditOutcome::TimedOut)),
            Decision::Cancelled if gone() => Err((Error::Denied, AuditOutcome::Cancelled)),
            Decision::Cancelled if !self.session.is_unlocked() => {
                Err((Error::VaultLocked, AuditOutcome::Locked))
            }
            Decision::Cancelled => Err((Error::Denied, AuditOutcome::Cancelled)),
        }
    }

    fn denied(&self, dir: &str, outcome: AuditOutcome) -> Fail {
        self.state().denials.record(dir, SystemTime::now());
        (Error::Denied, outcome)
    }

    fn store_grant(
        &self,
        req: &Request,
        hash: String,
        mode: ApprovalMode,
        duration: GrantDuration,
    ) -> Result<(), Error> {
        let now = SystemTime::now();
        self.session.set_project_mode(&req.project_dir, mode)?;
        let record = GrantRecord {
            hash,
            project_dir: req.project_dir.clone(),
            command_name: command_name(&req.command).unwrap_or_default(),
            references: req.references.clone(),
            created_at: unix(now),
        };
        match mode {
            ApprovalMode::PerRun => Ok(()),
            ApprovalMode::TimeBoxed => {
                self.session.add_timeboxed(record, duration, now);
                Ok(())
            }
            ApprovalMode::Remember => self.session.remember(record),
        }
    }

    /// Metadata only. A failed save never changes the response.
    fn audit(&self, req: &Request, outcome: AuditOutcome) {
        let omitted = req.references.len().saturating_sub(MAX_AUDIT_ENTRIES);
        let mut dir = req.project_dir.clone();
        if dir.len() > MAX_AUDIT_DIR {
            let cut = (0..=MAX_AUDIT_DIR)
                .rev()
                .find(|i| dir.is_char_boundary(*i))
                .unwrap_or(0);
            dir.truncate(cut);
        }
        self.session.record_audit(AuditRecord {
            at: unix(SystemTime::now()),
            project_dir: dir,
            entries: req
                .references
                .iter()
                .take(MAX_AUDIT_ENTRIES)
                .map(|r| view::sanitize(r, MAX_AUDIT_ENTRY))
                .collect(),
            omitted,
            outcome,
        });
    }
}

fn fail(e: Error) -> Fail {
    let outcome = match &e {
        Error::VaultLocked => AuditOutcome::Locked,
        Error::EntryNotFound(_) => AuditOutcome::EntryNotFound,
        _ => AuditOutcome::Invalid,
    };
    (e, outcome)
}

/// Steps 1-2: version probe first, then the full request.
fn parse(raw: &[u8]) -> Result<Request, Error> {
    #[derive(Deserialize)]
    struct Probe {
        version: u32,
    }
    let malformed = || Error::Io("malformed request".into());
    let probe: Probe = serde_json::from_slice(raw).map_err(|_| malformed())?;
    if probe.version != PROTOCOL_VERSION {
        return Err(Error::VersionMismatch {
            expected: PROTOCOL_VERSION,
            got: probe.version,
        });
    }
    serde_json::from_slice(raw).map_err(|_| malformed())
}

fn validate(req: &mut Request) -> Result<(), Error> {
    if command_name(&req.command).is_none() {
        return Err(Error::InvalidInput("command is empty".into()));
    }
    for r in &req.references {
        Reference::parse(r)?;
    }
    let declared: BTreeSet<&String> = req.variables.values().collect();
    if declared != req.references.iter().collect() {
        return Err(Error::InvalidInput(
            "variables do not match references".into(),
        ));
    }
    req.project_dir = std::fs::canonicalize(&req.project_dir)
        .map_err(|_| Error::InvalidInput("project directory not found".into()))?
        .to_string_lossy()
        .into_owned();
    req.references.sort();
    req.references.dedup();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use onepass_core::vault::KdfParams;
    use std::{
        collections::VecDeque,
        path::PathBuf,
        sync::atomic::{AtomicBool, AtomicUsize, Ordering::SeqCst},
    };
    use zeroize::Zeroizing;

    const PW: &[u8] = b"password1";
    const SECRET: &str = "alpha-secret";
    const ALLOW: Decision = Decision::Allow {
        mode: ApprovalMode::TimeBoxed,
        duration: GrantDuration::Min15,
    };

    struct FakeHost {
        script: Mutex<VecDeque<Decision>>,
        default: Mutex<Decision>,
        prompts: AtomicUsize,
        unlocks: AtomicUsize,
        hang: AtomicBool,
        delay: Duration,
        views: Mutex<Vec<PromptView>>,
    }

    impl FakeHost {
        fn new(default: Decision) -> Arc<Self> {
            Arc::new(Self {
                script: Mutex::default(),
                default: Mutex::new(default),
                prompts: AtomicUsize::new(0),
                unlocks: AtomicUsize::new(0),
                hang: AtomicBool::new(false),
                delay: Duration::ZERO,
                views: Mutex::default(),
            })
        }
        fn prompts(&self) -> usize {
            self.prompts.load(SeqCst)
        }
    }

    impl Host for FakeHost {
        fn request_unlock(&self) {
            self.unlocks.fetch_add(1, SeqCst);
        }
        fn prompt(&self, view: PromptView, timeout: Duration, cancel: Cancel) -> Decision {
            self.prompts.fetch_add(1, SeqCst);
            self.views.lock().unwrap().push(view);
            let start = Instant::now();
            while self.hang.load(SeqCst) {
                if cancel() {
                    return Decision::Cancelled;
                }
                if start.elapsed() >= timeout {
                    return Decision::TimedOut;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            std::thread::sleep(self.delay);
            self.script
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or(*self.default.lock().unwrap())
        }
    }

    struct Env {
        session: Arc<Session>,
        host: Arc<FakeHost>,
        broker: Arc<Broker>,
        dir: PathBuf,
    }

    fn short() -> Limits {
        Limits {
            unlock_wait: Duration::from_millis(300),
            approval_wait: Duration::from_millis(500),
            request_deadline: Duration::from_secs(3),
        }
    }

    fn env(tag: &str, host: Arc<FakeHost>, limits: Limits) -> Env {
        let dir = std::env::temp_dir().join(format!("op-broker-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let dir = dir.canonicalize().unwrap();
        let kdf = KdfParams {
            m_cost: 8,
            t_cost: 1,
            p_cost: 1,
        };
        let session = Arc::new(Session::new(dir.join("vault.onepass"), kdf));
        session.create(PW).unwrap();
        session
            .mutate(|b| {
                b.save_entry(
                    "p",
                    "e",
                    "A",
                    None,
                    Some(Zeroizing::new(SECRET.into())),
                    String::new(),
                )?;
                b.save_entry(
                    "p",
                    "e",
                    "B",
                    None,
                    Some(Zeroizing::new("beta-secret".into())),
                    String::new(),
                )
            })
            .unwrap();
        let broker = Arc::new(Broker::new(session.clone(), host.clone(), limits));
        Env {
            session,
            host,
            broker,
            dir,
        }
    }

    fn body(dir: &std::path::Path, refs: &[&str], cmd: &[&str]) -> Vec<u8> {
        let variables: BTreeMap<String, String> = refs
            .iter()
            .enumerate()
            .map(|(i, r)| (format!("V{i}"), r.to_string()))
            .collect();
        serde_json::to_vec(&Request {
            version: 1,
            project_dir: dir.to_string_lossy().into_owned(),
            command: cmd.iter().map(|s| s.to_string()).collect(),
            references: refs.iter().map(|s| s.to_string()).collect(),
            variables,
        })
        .unwrap()
    }

    impl Env {
        fn call(&self, refs: &[&str], cmd: &[&str]) -> Response {
            self.broker.handle(
                &body(&self.dir, refs, cmd),
                Peer::default(),
                Arc::new(|| false),
            )
        }
        fn run(&self) -> Response {
            self.call(&["op://p/e/A"], &["node"])
        }
        fn audit(&self) -> Vec<AuditRecord> {
            self.session.read(|v| v.body.audit.clone()).unwrap()
        }
    }

    fn err(r: Response) -> Error {
        match r {
            Response::Err(e) => e,
            Response::Ok(_) => panic!("expected error"),
        }
    }

    fn ok(r: Response) -> BTreeMap<String, String> {
        match r {
            Response::Ok(m) => m,
            Response::Err(e) => panic!("expected ok, got {e}"),
        }
    }

    #[test]
    fn allow_returns_only_requested_values() {
        let e = env("allow", FakeHost::new(ALLOW), short());
        let m = ok(e.run());
        assert_eq!(m.len(), 1);
        assert_eq!(m["op://p/e/A"], SECRET);
    }

    #[test]
    fn deny_returns_denied_and_prompts_once() {
        let e = env("deny", FakeHost::new(Decision::Deny), short());
        assert_eq!(err(e.run()), Error::Denied);
        assert_eq!(e.host.prompts(), 1);
    }

    #[test]
    fn version_mismatch_before_anything() {
        let e = env("version", FakeHost::new(ALLOW), short());
        let r = e
            .broker
            .handle(br#"{"version":99}"#, Peer::default(), Arc::new(|| false));
        assert_eq!(
            err(r),
            Error::VersionMismatch {
                expected: 1,
                got: 99
            }
        );
        assert_eq!(e.host.prompts(), 0);
    }

    #[test]
    fn malformed_is_typed() {
        let e = env("malformed", FakeHost::new(ALLOW), short());
        assert!(matches!(
            err(e
                .broker
                .handle(b"nope", Peer::default(), Arc::new(|| false))),
            Error::Io(_)
        ));
    }

    #[test]
    fn variables_must_match_references() {
        let e = env("vars", FakeHost::new(ALLOW), short());
        let raw = format!(
            r#"{{"version":1,"project_dir":"{}","command":["node"],"references":["op://p/e/A"],"variables":{{}}}}"#,
            e.dir.display()
        );
        let r = e
            .broker
            .handle(raw.as_bytes(), Peer::default(), Arc::new(|| false));
        assert!(matches!(err(r), Error::InvalidInput(_)));
        assert_eq!(e.host.prompts(), 0);
    }

    #[test]
    fn invalid_requests_are_not_audited() {
        let e = env("noaudit", FakeHost::new(ALLOW), short());
        let raw = body(&e.dir.join("missing"), &["op://p/e/A"], &["node"]);
        let _ = e.broker.handle(&raw, Peer::default(), Arc::new(|| false));
        assert!(e.audit().is_empty());
    }

    #[test]
    fn nonexistent_project_dir_rejected() {
        let e = env("nodir", FakeHost::new(ALLOW), short());
        let raw = body(&e.dir.join("missing"), &["op://p/e/A"], &["node"]);
        let r = e.broker.handle(&raw, Peer::default(), Arc::new(|| false));
        assert!(matches!(err(r), Error::InvalidInput(_)));
        assert_eq!(e.host.prompts(), 0);
    }

    #[test]
    fn missing_entry_prompts_then_not_found_and_stores_no_grant() {
        let e = env("missing", FakeHost::new(ALLOW), short());
        for n in 1..=2 {
            let r = e.call(&["op://p/e/NOPE"], &["node"]);
            assert!(matches!(err(r), Error::EntryNotFound(_)));
            assert_eq!(e.host.prompts(), n);
        }
    }

    #[test]
    fn locked_raises_window_and_times_out() {
        let e = env("locked", FakeHost::new(ALLOW), short());
        e.session.lock();
        assert_eq!(err(e.run()), Error::VaultLocked);
        assert_eq!(e.host.unlocks.load(SeqCst), 1);
        assert_eq!(e.host.prompts(), 0);
    }

    #[test]
    fn locked_then_unlocked_proceeds() {
        let e = env("unlock", FakeHost::new(ALLOW), short());
        e.session.lock();
        let s = e.session.clone();
        let h = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            s.unlock(PW).unwrap();
        });
        assert_eq!(ok(e.run())["op://p/e/A"], SECRET);
        h.join().unwrap();
    }

    #[test]
    fn timeboxed_grant_skips_second_prompt() {
        let e = env("timebox", FakeHost::new(ALLOW), short());
        ok(e.run());
        ok(e.run());
        assert_eq!(e.host.prompts(), 1);
        assert_eq!(
            e.audit().last().unwrap().outcome,
            AuditOutcome::ApprovedByGrant
        );
    }

    #[test]
    fn changed_reference_set_prompts_again() {
        let e = env("refs", FakeHost::new(ALLOW), short());
        ok(e.run());
        ok(e.call(&["op://p/e/A", "op://p/e/B"], &["node"]));
        assert_eq!(e.host.prompts(), 2);
    }

    #[test]
    fn changed_exec_prompts_again() {
        let e = env("exec", FakeHost::new(ALLOW), short());
        ok(e.call(&["op://p/e/A"], &["node"]));
        ok(e.call(&["op://p/e/A"], &["python"]));
        assert_eq!(e.host.prompts(), 2);
    }

    #[test]
    fn per_run_always_prompts() {
        let allow = Decision::Allow {
            mode: ApprovalMode::PerRun,
            duration: GrantDuration::Min15,
        };
        let e = env("perrun", FakeHost::new(allow), short());
        ok(e.run());
        ok(e.run());
        assert_eq!(e.host.prompts(), 2);
    }

    #[test]
    fn remember_survives_lock() {
        let allow = Decision::Allow {
            mode: ApprovalMode::Remember,
            duration: GrantDuration::Min15,
        };
        let e = env("remember", FakeHost::new(allow), short());
        ok(e.run());
        e.session.lock();
        e.session.unlock(PW).unwrap();
        ok(e.run());
        assert_eq!(e.host.prompts(), 1);
    }

    #[test]
    fn three_denials_rate_limit() {
        let e = env("ratelimit", FakeHost::new(Decision::Deny), short());
        for _ in 0..3 {
            assert_eq!(err(e.run()), Error::Denied);
        }
        assert_eq!(err(e.run()), Error::RateLimited);
        assert_eq!(e.host.prompts(), 3);
        // Rate-limited/busy rejections are not persisted, so a flood cannot evict real history.
        assert_eq!(e.audit().len(), 3);
    }

    #[test]
    fn timeout_counts_as_denial() {
        let e = env("timeout", FakeHost::new(Decision::TimedOut), short());
        for _ in 0..3 {
            assert_eq!(err(e.run()), Error::Denied);
        }
        assert_eq!(err(e.run()), Error::RateLimited);
    }

    #[test]
    fn client_disconnect_cancels_without_grant_or_denial() {
        let host = FakeHost::new(Decision::Deny);
        host.hang.store(true, SeqCst);
        let e = env("gone", host, short());
        let gone = Arc::new(AtomicBool::new(false));
        let g = gone.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            g.store(true, SeqCst);
        });
        let g = gone.clone();
        let r = e.broker.handle(
            &body(&e.dir, &["op://p/e/A"], &["node"]),
            Peer::default(),
            Arc::new(move || g.load(SeqCst)),
        );
        assert_eq!(err(r), Error::Denied);
        assert_eq!(e.audit().last().unwrap().outcome, AuditOutcome::Cancelled);
        // Had the cancel counted, the third denial below would trip the limit on the fourth call.
        e.host.hang.store(false, SeqCst);
        for _ in 0..3 {
            assert_eq!(err(e.run()), Error::Denied);
        }
    }

    #[test]
    fn lock_during_prompt_returns_locked() {
        let host = FakeHost::new(Decision::Deny);
        host.hang.store(true, SeqCst);
        let e = env(
            "lockprompt",
            host,
            Limits {
                approval_wait: Duration::from_secs(5),
                ..short()
            },
        );
        let s = e.session.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            s.lock();
        });
        assert_eq!(err(e.run()), Error::VaultLocked);
    }

    #[test]
    fn pending_cap_returns_busy() {
        let host = FakeHost::new(Decision::Deny);
        host.hang.store(true, SeqCst);
        let long = Limits {
            unlock_wait: Duration::from_secs(1),
            approval_wait: Duration::from_secs(10),
            request_deadline: Duration::from_secs(20),
        };
        let e = Arc::new(env("busy", host, long));
        let hs: Vec<_> = (0..MAX_PENDING)
            .map(|_| {
                let e = e.clone();
                std::thread::spawn(move || e.run())
            })
            .collect();
        while e.broker.pending() < MAX_PENDING {
            std::thread::sleep(Duration::from_millis(5));
        }
        let t = Instant::now();
        assert_eq!(err(e.run()), Error::Busy);
        assert!(t.elapsed() < Duration::from_millis(500));
        e.host.hang.store(false, SeqCst);
        hs.into_iter().for_each(|h| drop(h.join().unwrap()));
    }

    #[test]
    fn parallel_same_key_prompts_once() {
        let mut host = Arc::try_unwrap(FakeHost::new(ALLOW)).ok().unwrap();
        host.delay = Duration::from_millis(50);
        let e = Arc::new(env("parallel", Arc::new(host), short()));
        let hs: Vec<_> = (0..4)
            .map(|_| {
                let e = e.clone();
                std::thread::spawn(move || e.run())
            })
            .collect();
        for h in hs {
            assert_eq!(ok(h.join().unwrap())["op://p/e/A"], SECRET);
        }
        assert_eq!(e.host.prompts(), 1);
    }

    #[test]
    fn queued_requests_respect_rate_limit() {
        let mut host = Arc::try_unwrap(FakeHost::new(Decision::Deny)).ok().unwrap();
        host.delay = Duration::from_millis(30);
        let e = Arc::new(env("queued", Arc::new(host), short()));
        let hs: Vec<_> = (0..5)
            .map(|_| {
                let e = e.clone();
                std::thread::spawn(move || e.run())
            })
            .collect();
        let errs: Vec<Error> = hs.into_iter().map(|h| err(h.join().unwrap())).collect();
        assert_eq!(e.host.prompts(), 3);
        assert_eq!(errs.iter().filter(|x| **x == Error::RateLimited).count(), 2);
        assert_eq!(errs.iter().filter(|x| **x == Error::Denied).count(), 3);
    }

    #[test]
    fn audit_has_no_values() {
        let e = env("audit", FakeHost::new(ALLOW), short());
        ok(e.run());
        let a = e.audit();
        assert!(!serde_json::to_string(&a).unwrap().contains(SECRET));
        assert_eq!(a.last().unwrap().outcome, AuditOutcome::Approved);
        assert_eq!(a.last().unwrap().entries, ["op://p/e/A"]);
    }

    #[test]
    fn prompt_view_has_no_values() {
        let e = env("view", FakeHost::new(ALLOW), short());
        ok(e.run());
        let v = serde_json::to_string(&*e.host.views.lock().unwrap()).unwrap();
        assert!(!v.contains(SECRET) && v.contains("op://p/e/A") && v.contains("\"V0\""));
    }
}
