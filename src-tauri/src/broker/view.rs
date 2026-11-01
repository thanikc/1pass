//! What the approval dialog shows. Client-claimed strings are sanitized and bounded here so they
//! cannot spoof the dialog; values never appear.
use super::Peer;
use onepass_core::{protocol::Request, vault::ApprovalMode};
use serde::Serialize;

const MAX_FIELD: usize = 1024;
const MAX_ARGS: usize = 64;

fn is_bidi(c: char) -> bool {
    matches!(c, '\u{061C}' | '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
}

/// Replaces control and bidi characters with U+FFFD and truncates to `max_chars` (ending in `…`).
pub fn sanitize(s: &str, max_chars: usize) -> String {
    let mut out: String = s
        .chars()
        .take(max_chars + 1)
        .map(|c| {
            if c.is_control() || is_bidi(c) {
                '\u{FFFD}'
            } else {
                c
            }
        })
        .collect();
    if out.chars().count() > max_chars {
        out = out.chars().take(max_chars.saturating_sub(1)).collect();
        out.push('…');
    }
    out
}

fn bound_command(command: &[String]) -> Vec<String> {
    let mut out: Vec<String> = command
        .iter()
        .take(MAX_ARGS)
        .map(|a| sanitize(a, MAX_FIELD))
        .collect();
    if command.len() > MAX_ARGS {
        out.push(format!("… ({} more)", command.len() - MAX_ARGS));
    }
    out
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PromptView {
    /// Assigned by `Host`; the broker passes 0.
    pub id: u64,
    pub project_dir: String,
    pub command: Vec<String>,
    pub command_name: String,
    pub peer_pid: Option<u32>,
    pub peer_exe: Option<String>,
    pub variables: Vec<(String, String)>,
    pub references: Vec<String>,
    pub mode: ApprovalMode,
}

impl PromptView {
    pub fn new(req: &Request, peer: &Peer, mode: ApprovalMode) -> Self {
        Self {
            id: 0,
            project_dir: sanitize(&req.project_dir, MAX_FIELD),
            command: bound_command(&req.command),
            command_name: super::grants::command_name(&req.command)
                .map(|n| sanitize(&n, MAX_FIELD))
                .unwrap_or_default(),
            peer_pid: peer.pid,
            peer_exe: peer.exe.as_deref().map(|e| sanitize(e, MAX_FIELD)),
            variables: req
                .variables
                .iter()
                .map(|(k, v)| (sanitize(k, MAX_FIELD), sanitize(v, MAX_FIELD)))
                .collect(),
            references: req
                .references
                .iter()
                .map(|r| sanitize(r, MAX_FIELD))
                .collect(),
            mode,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_replaces_control_and_bidi() {
        assert_eq!(
            sanitize("a\nb\u{202E}c\u{2066}", 100),
            "a\u{FFFD}b\u{FFFD}c\u{FFFD}"
        );
    }

    #[test]
    fn sanitize_truncates_with_ellipsis() {
        let s = sanitize(&"x".repeat(5000), 1024);
        assert_eq!(s.chars().count(), 1024);
        assert!(s.ends_with('…'));
    }

    #[test]
    fn huge_argv_is_bounded() {
        let cmd: Vec<String> = (0..100).map(|i| i.to_string()).collect();
        let v = bound_command(&cmd);
        assert_eq!(v.len(), 65);
        assert_eq!(v[64], "… (36 more)");
    }
}
