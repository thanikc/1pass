//! `#[tauri::command]` layer. Thin: validation and logic live in core/session/broker.
use crate::{
    autolock::AutoLock,
    broker::{Decision, grants::GrantDuration, view::PromptView},
    clipboard::{Clipboard, ClipboardGuard},
    host::TauriHost,
    session::{Session, VaultStatus},
};
use onepass_core::{
    Error,
    protocol::Reference,
    vault::{ApprovalMode, AuditRecord, EntryMeta, Settings},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    sync::{Arc, Mutex, MutexGuard},
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_clipboard_manager::ClipboardExt;
use zeroize::Zeroizing;

pub struct AppState {
    pub app: AppHandle,
    pub session: Arc<Session>,
    pub host: Arc<TauriHost>,
    pub autolock: Mutex<AutoLock>,
    pub clip: Mutex<ClipboardGuard>,
    pub ipc_error: Mutex<Option<String>>,
    pub shutdown: Mutex<Option<Box<dyn FnOnce() + Send>>>,
}

fn guard<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

pub struct AppClipboard<'a>(pub &'a AppHandle);

impl Clipboard for AppClipboard<'_> {
    fn write_secret(&self, text: &str) -> Result<(), Error> {
        self.0
            .clipboard()
            .write_secret(text)
            .map_err(|e| Error::Io(e.to_string()))
    }
    fn write_plain(&self, text: &str) -> Result<(), Error> {
        self.0
            .clipboard()
            .write_text(text)
            .map_err(|e| Error::Io(e.to_string()))
    }
    fn read(&self) -> Option<Zeroizing<String>> {
        self.0.clipboard().read_text().ok().map(Zeroizing::new)
    }
    fn clear(&self) {
        let _ = self.0.clipboard().clear();
    }
}

impl AppState {
    fn changed(&self) {
        let _ = self.app.emit("vault-state-changed", ());
    }

    /// Lock and drop our clipboard value. Used by the Lock button and the idle timer.
    pub fn lock_now(&self) {
        self.session.lock();
        guard(&self.clip).clear_now(&AppClipboard(&self.app));
        self.changed();
    }

    fn touch(&self) {
        guard(&self.autolock).touch(SystemTime::now());
    }
}

type R<T> = Result<T, Error>;

fn unix(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

#[tauri::command(async)]
pub fn vault_status(s: State<'_, AppState>) -> R<VaultStatus> {
    Ok(s.session.status())
}

#[tauri::command(async)]
pub fn create_vault(s: State<'_, AppState>, password: Zeroizing<String>) -> R<()> {
    s.session.create(password.as_bytes())?;
    s.touch();
    s.changed();
    Ok(())
}

#[tauri::command(async)]
pub fn unlock(s: State<'_, AppState>, password: Zeroizing<String>) -> R<()> {
    s.session.unlock(password.as_bytes())?;
    s.touch();
    s.changed();
    Ok(())
}

#[tauri::command(async)]
pub fn lock(s: State<'_, AppState>) -> R<()> {
    s.lock_now();
    Ok(())
}

#[tauri::command(async)]
pub fn change_password(
    s: State<'_, AppState>,
    current: Zeroizing<String>,
    next: Zeroizing<String>,
) -> R<()> {
    s.session
        .change_password(current.as_bytes(), next.as_bytes())
}

#[tauri::command(async)]
pub fn list_entries(s: State<'_, AppState>) -> R<Vec<EntryMeta>> {
    s.session.read(|v| v.body.metadata())
}

/// Includes environments with no entries.
#[tauri::command(async)]
pub fn list_environments(s: State<'_, AppState>) -> R<Vec<(String, String)>> {
    s.session.read(|v| {
        v.body
            .projects
            .iter()
            .flat_map(|(p, envs)| envs.keys().map(move |e| (p.clone(), e.clone())))
            .collect()
    })
}

#[tauri::command(async)]
pub fn create_environment(s: State<'_, AppState>, project: String, environment: String) -> R<()> {
    s.session
        .mutate(|b| b.create_environment(&project, &environment))
}

#[tauri::command(async)]
pub fn save_entry(
    s: State<'_, AppState>,
    project: String,
    environment: String,
    name: String,
    rename_from: Option<String>,
    value: Option<Zeroizing<String>>,
    notes: String,
) -> R<()> {
    s.session.mutate(|b| {
        b.save_entry(
            &project,
            &environment,
            &name,
            rename_from.as_deref(),
            value,
            notes,
        )
    })
}

#[tauri::command(async)]
pub fn delete_entry(
    s: State<'_, AppState>,
    project: String,
    environment: String,
    name: String,
) -> R<()> {
    s.session
        .mutate(|b| b.delete_entry(&project, &environment, &name))
}

#[tauri::command(async)]
pub fn rename_environment(
    s: State<'_, AppState>,
    project: String,
    from: String,
    to: String,
) -> R<()> {
    s.session
        .mutate(|b| b.rename_environment(&project, &from, &to))
}

#[tauri::command(async)]
pub fn delete_environment(s: State<'_, AppState>, project: String, environment: String) -> R<()> {
    s.session
        .mutate(|b| b.delete_environment(&project, &environment))
}

fn value_of(s: &AppState, project: &str, environment: &str, name: &str) -> R<Zeroizing<String>> {
    s.session
        .read(|v| v.body.value(project, environment, name).cloned())?
}

/// The only command that returns a secret to the UI.
#[tauri::command(async)]
pub fn reveal_entry(
    s: State<'_, AppState>,
    project: String,
    environment: String,
    name: String,
) -> R<String> {
    value_of(&s, &project, &environment, &name).map(|v| (*v).clone())
}

#[tauri::command(async)]
pub fn copy_entry_value(
    s: State<'_, AppState>,
    project: String,
    environment: String,
    name: String,
) -> R<()> {
    let v = value_of(&s, &project, &environment, &name)?;
    guard(&s.clip).copy_secret(&AppClipboard(&s.app), &v, SystemTime::now())
}

#[tauri::command(async)]
pub fn copy_reference(
    s: State<'_, AppState>,
    project: String,
    environment: String,
    name: String,
) -> R<()> {
    let reference = format!("op://{project}/{environment}/{name}");
    Reference::parse(&reference)?;
    AppClipboard(&s.app).write_plain(&reference)
}

#[tauri::command(async)]
pub fn list_pending_approvals(s: State<'_, AppState>) -> R<Vec<PromptView>> {
    Ok(s.host.pending())
}

/// The UI can only allow or deny; timeouts and cancels are decided server-side.
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionInput {
    Allow {
        mode: ApprovalMode,
        duration: GrantDuration,
    },
    Deny,
}

#[tauri::command(async)]
pub fn respond_approval(s: State<'_, AppState>, id: u64, decision: DecisionInput) -> R<()> {
    s.host.respond(
        id,
        match decision {
            DecisionInput::Allow { mode, duration } => Decision::Allow { mode, duration },
            DecisionInput::Deny => Decision::Deny,
        },
    )
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GrantView {
    hash: String,
    project_dir: String,
    command_name: String,
    references: Vec<String>,
    expires_at: Option<u64>,
    remembered: bool,
}

#[tauri::command(async)]
pub fn list_grants(s: State<'_, AppState>) -> R<Vec<GrantView>> {
    let remembered: HashSet<String> = s
        .session
        .read(|v| v.body.remembered.iter().map(|r| r.hash.clone()).collect())?;
    Ok(s.session
        .list_grants(SystemTime::now())?
        .into_iter()
        .map(|g| GrantView {
            remembered: remembered.contains(&g.record.hash),
            expires_at: g.expires_at.map(unix),
            hash: g.record.hash,
            project_dir: g.record.project_dir,
            command_name: g.record.command_name,
            references: g.record.references,
        })
        .collect())
}

#[tauri::command(async)]
pub fn revoke_grant(s: State<'_, AppState>, hash: String) -> R<()> {
    s.session.revoke_grant(&hash)
}

#[tauri::command(async)]
pub fn list_project_modes(s: State<'_, AppState>) -> R<BTreeMap<String, ApprovalMode>> {
    s.session.read(|v| v.body.project_modes.clone())
}

#[tauri::command(async)]
pub fn set_project_mode(s: State<'_, AppState>, project_dir: String, mode: ApprovalMode) -> R<()> {
    s.session.set_project_mode(&project_dir, mode)
}

/// Newest first.
#[tauri::command(async)]
pub fn list_audit(s: State<'_, AppState>) -> R<Vec<AuditRecord>> {
    s.session
        .read(|v| v.body.audit.iter().rev().cloned().collect())
}

#[tauri::command(async)]
pub fn get_settings(s: State<'_, AppState>) -> R<Settings> {
    s.session.read(|v| v.body.settings.clone())
}

pub const AUTO_LOCK_CHOICES: [u32; 6] = [0, 1, 5, 15, 30, 60];

#[tauri::command(async)]
pub fn set_auto_lock(s: State<'_, AppState>, minutes: u32) -> R<()> {
    if !AUTO_LOCK_CHOICES.contains(&minutes) {
        return Err(Error::InvalidInput("unsupported auto-lock value".into()));
    }
    s.session.mutate(|b| {
        b.settings.auto_lock_minutes = minutes;
        Ok(())
    })
}

#[tauri::command(async)]
pub fn note_activity(s: State<'_, AppState>) -> R<()> {
    s.touch();
    Ok(())
}

#[tauri::command(async)]
pub fn get_ipc_error(s: State<'_, AppState>) -> R<Option<String>> {
    Ok(guard(&s.ipc_error).clone())
}
