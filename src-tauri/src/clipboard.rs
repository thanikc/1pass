//! Clear a copied secret after 30 s, but only if the clipboard still holds it.
use onepass_core::Error;
use sha2::{Digest, Sha256};
use std::{
    fmt,
    time::{Duration, SystemTime},
};
use zeroize::Zeroizing;

const CLEAR_AFTER: Duration = Duration::from_secs(30);

pub trait Clipboard {
    fn write_secret(&self, text: &str) -> Result<(), Error>;
    fn write_plain(&self, text: &str) -> Result<(), Error>;
    fn read(&self) -> Option<Zeroizing<String>>;
    fn clear(&self);
}

fn digest(s: &str) -> [u8; 32] {
    Sha256::digest(s.as_bytes()).into()
}

/// Remembers only a digest of what it copied, never the secret.
#[derive(Default)]
pub struct ClipboardGuard {
    ours: Option<([u8; 32], SystemTime)>,
}

impl fmt::Debug for ClipboardGuard {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "ClipboardGuard(armed: {})", self.ours.is_some())
    }
}

impl ClipboardGuard {
    fn holds_ours(&self, cb: &dyn Clipboard) -> bool {
        self.ours
            .is_some_and(|(d, _)| cb.read().is_some_and(|c| digest(&c) == d))
    }

    pub fn copy_secret(
        &mut self,
        cb: &dyn Clipboard,
        value: &str,
        now: SystemTime,
    ) -> Result<(), Error> {
        cb.write_secret(value)?;
        self.ours = Some((digest(value), now));
        Ok(())
    }

    pub fn tick(&mut self, cb: &dyn Clipboard, now: SystemTime) {
        let Some((_, at)) = self.ours else { return };
        if now.duration_since(at).is_ok_and(|age| age >= CLEAR_AFTER) {
            self.clear_now(cb);
        }
    }

    pub fn clear_now(&mut self, cb: &dyn Clipboard) {
        if self.holds_ours(cb) {
            cb.clear();
        }
        self.ours = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[derive(Default)]
    struct Fake(RefCell<Option<String>>);

    impl Clipboard for Fake {
        fn write_secret(&self, t: &str) -> Result<(), Error> {
            *self.0.borrow_mut() = Some(t.into());
            Ok(())
        }
        fn write_plain(&self, t: &str) -> Result<(), Error> {
            self.write_secret(t)
        }
        fn read(&self) -> Option<Zeroizing<String>> {
            self.0.borrow().clone().map(Zeroizing::new)
        }
        fn clear(&self) {
            *self.0.borrow_mut() = None;
        }
    }

    const S: Duration = Duration::from_secs(1);
    fn t0() -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000)
    }

    #[test]
    fn cleared_after_30s() {
        let (cb, mut g) = (Fake::default(), ClipboardGuard::default());
        g.copy_secret(&cb, "hunter2", t0()).unwrap();
        g.tick(&cb, t0() + 29 * S);
        assert!(cb.read().is_some());
        g.tick(&cb, t0() + 30 * S);
        assert!(cb.read().is_none());
    }

    #[test]
    fn user_overwrite_is_not_cleared() {
        let (cb, mut g) = (Fake::default(), ClipboardGuard::default());
        g.copy_secret(&cb, "hunter2", t0()).unwrap();
        cb.write_plain("other").unwrap();
        g.tick(&cb, t0() + 30 * S);
        assert_eq!(cb.read().as_deref().map(String::as_str), Some("other"));
    }

    #[test]
    fn second_copy_restarts_timer() {
        let (cb, mut g) = (Fake::default(), ClipboardGuard::default());
        g.copy_secret(&cb, "one", t0()).unwrap();
        g.copy_secret(&cb, "two", t0() + 20 * S).unwrap();
        g.tick(&cb, t0() + 40 * S);
        assert!(cb.read().is_some());
        g.tick(&cb, t0() + 50 * S);
        assert!(cb.read().is_none());
    }

    #[test]
    fn clear_now_only_clears_ours() {
        let (cb, mut g) = (Fake::default(), ClipboardGuard::default());
        g.copy_secret(&cb, "hunter2", t0()).unwrap();
        g.clear_now(&cb);
        assert!(cb.read().is_none());
        cb.write_plain("mine").unwrap();
        g.clear_now(&cb);
        assert!(cb.read().is_some());
    }

    #[test]
    fn guard_stores_digest_only() {
        let (cb, mut g) = (Fake::default(), ClipboardGuard::default());
        g.copy_secret(&cb, "hunter2", t0()).unwrap();
        assert!(!format!("{g:?}").contains("hunter2"));
        assert_eq!(g.ours.unwrap().0, digest("hunter2"));
    }
}
