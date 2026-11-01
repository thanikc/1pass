//! Bridge between the broker and the window: pending prompts, events, raising the window.
use crate::{
    broker::{Decision, Host, view::PromptView},
    session::Cancel,
};
use onepass_core::Error;
use std::{
    collections::BTreeMap,
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering::SeqCst},
        mpsc::{Receiver, Sender, channel},
    },
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, Manager};

const POLL: Duration = Duration::from_millis(50);

struct Pending {
    view: PromptView,
    tx: Sender<Decision>,
}

/// Prompts awaiting a decision. Separate from `AppHandle` so it can be tested.
#[derive(Default)]
pub struct PromptQueue {
    next_id: AtomicU64,
    pending: Mutex<BTreeMap<u64, Pending>>,
}

impl PromptQueue {
    fn map(&self) -> std::sync::MutexGuard<'_, BTreeMap<u64, Pending>> {
        self.pending.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn open(&self, mut view: PromptView) -> (u64, Receiver<Decision>) {
        let id = self.next_id.fetch_add(1, SeqCst) + 1;
        view.id = id;
        let (tx, rx) = channel();
        self.map().insert(id, Pending { view, tx });
        (id, rx)
    }

    pub fn pending(&self) -> Vec<PromptView> {
        self.map().values().map(|p| p.view.clone()).collect()
    }

    pub fn respond(&self, id: u64, decision: Decision) -> Result<(), Error> {
        let p = self
            .map()
            .remove(&id)
            .ok_or_else(|| Error::InvalidInput("no such approval".into()))?;
        p.tx.send(decision)
            .map_err(|_| Error::InvalidInput("approval already closed".into()))
    }

    /// Blocks until answered, timed out or cancelled. `changed` runs when the pending set changes.
    pub fn wait(
        &self,
        view: PromptView,
        timeout: Duration,
        cancel: &Cancel,
        changed: &dyn Fn(),
    ) -> Decision {
        let (id, rx) = self.open(view);
        changed();
        let deadline = Instant::now() + timeout;
        let decision = loop {
            if let Ok(d) = rx.recv_timeout(POLL) {
                break d;
            }
            let end = if cancel() {
                Decision::Cancelled
            } else if Instant::now() >= deadline {
                Decision::TimedOut
            } else {
                continue;
            };
            // A respond may have raced us: its answer wins.
            break if self.map().remove(&id).is_some() {
                end
            } else {
                rx.try_recv().unwrap_or(end)
            };
        };
        changed();
        decision
    }
}

pub struct TauriHost {
    app: AppHandle,
    queue: PromptQueue,
}

impl TauriHost {
    pub fn new(app: AppHandle) -> Self {
        Self {
            app,
            queue: PromptQueue::default(),
        }
    }

    pub fn pending(&self) -> Vec<PromptView> {
        self.queue.pending()
    }

    pub fn respond(&self, id: u64, decision: Decision) -> Result<(), Error> {
        self.queue.respond(id, decision)
    }
}

pub fn raise(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

impl Host for TauriHost {
    fn request_unlock(&self) {
        raise(&self.app);
        let _ = self.app.emit("vault-state-changed", ());
    }

    fn prompt(&self, view: PromptView, timeout: Duration, cancel: Cancel) -> Decision {
        raise(&self.app);
        self.queue.wait(view, timeout, &cancel, &|| {
            let _ = self.app.emit("approvals-changed", ());
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::broker::Peer;
    use onepass_core::{protocol::Request, vault::ApprovalMode};
    use std::sync::Arc;

    fn view() -> PromptView {
        let req = Request {
            version: 1,
            project_dir: "/p".into(),
            command: vec!["node".into()],
            references: vec![],
            variables: Default::default(),
        };
        PromptView::new(&req, &Peer::default(), ApprovalMode::TimeBoxed)
    }

    fn never() -> Cancel {
        Arc::new(|| false)
    }

    fn spawn_wait(
        q: &Arc<PromptQueue>,
        timeout: Duration,
        cancel: Cancel,
    ) -> std::thread::JoinHandle<Decision> {
        let q = q.clone();
        std::thread::spawn(move || q.wait(view(), timeout, &cancel, &|| {}))
    }

    fn first_id(q: &PromptQueue) -> u64 {
        loop {
            if let Some(v) = q.pending().first() {
                return v.id;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn respond_delivers_decision() {
        let q = Arc::new(PromptQueue::default());
        let h = spawn_wait(&q, Duration::from_secs(5), never());
        q.respond(first_id(&q), Decision::Deny).unwrap();
        assert_eq!(h.join().unwrap(), Decision::Deny);
        assert!(q.pending().is_empty());
    }

    #[test]
    fn respond_unknown_id_is_error() {
        let q = Arc::new(PromptQueue::default());
        let h = spawn_wait(&q, Duration::from_secs(5), never());
        let id = first_id(&q);
        q.respond(id, Decision::Deny).unwrap();
        assert!(q.respond(id, Decision::Deny).is_err());
        h.join().unwrap();
    }

    #[test]
    fn prompt_times_out_and_is_removed() {
        let q = Arc::new(PromptQueue::default());
        let d = spawn_wait(&q, Duration::from_millis(100), never())
            .join()
            .unwrap();
        assert_eq!(d, Decision::TimedOut);
        assert!(q.pending().is_empty());
    }

    #[test]
    fn prompt_cancel_removes_view() {
        let q = Arc::new(PromptQueue::default());
        let d = spawn_wait(&q, Duration::from_secs(5), Arc::new(|| true))
            .join()
            .unwrap();
        assert_eq!(d, Decision::Cancelled);
        assert!(q.pending().is_empty());
    }

    #[test]
    fn ids_are_not_reused() {
        let q = Arc::new(PromptQueue::default());
        let mut ids = vec![];
        for _ in 0..3 {
            let h = spawn_wait(&q, Duration::from_secs(5), never());
            let id = first_id(&q);
            ids.push(id);
            q.respond(id, Decision::Deny).unwrap();
            h.join().unwrap();
        }
        assert!(ids.windows(2).all(|w| w[0] < w[1]));
    }
}
