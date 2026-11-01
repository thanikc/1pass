pub mod autolock;
pub mod broker;
pub mod clipboard;
mod commands;
mod host;
pub mod session;
pub mod transport;

use broker::{Broker, Limits};
use commands::{AppClipboard, AppState};
use host::TauriHost;
use onepass_core::{Error, protocol::endpoint, vault::KdfParams};
use session::Session;
use std::{
    sync::{Arc, Mutex},
    time::{Duration, SystemTime},
};
use tauri::{Emitter, Manager, RunEvent};

type Shutdown = Box<dyn FnOnce() + Send>;

#[cfg(unix)]
fn start_ipc(broker: Arc<Broker>) -> Result<Shutdown, Error> {
    let handle = transport::serve(broker, endpoint()?)?;
    Ok(Box::new(move || handle.shutdown()))
}

#[cfg(not(unix))]
fn start_ipc(_: Arc<Broker>) -> Result<Shutdown, Error> {
    Err(Error::Io("this platform is not supported yet".into()))
}

fn guard<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Once a second: auto-lock when idle (wall clock, so sleep counts) and expire our clipboard value.
fn maintain(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_secs(1));
            let s = app.state::<AppState>();
            let now = SystemTime::now();
            if let Ok(minutes) = s.session.read(|v| v.body.settings.auto_lock_minutes)
                && guard(&s.autolock).is_due(now, minutes)
            {
                s.lock_now();
            }
            guard(&s.clip).tick(&AppClipboard(&app), now);
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            host::raise(app)
        }))
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            let session = Arc::new(Session::new(
                dir.join("vault.onepass"),
                KdfParams::default(),
            ));
            let host = Arc::new(TauriHost::new(app.handle().clone()));
            let broker = Arc::new(Broker::new(
                session.clone(),
                host.clone(),
                Limits::default(),
            ));
            let (shutdown, ipc_error) = match start_ipc(broker) {
                Ok(s) => (Some(s), None),
                Err(e) => (None, Some(e.to_string())),
            };
            app.manage(AppState {
                app: app.handle().clone(),
                session,
                host,
                autolock: Mutex::new(autolock::AutoLock::new(SystemTime::now())),
                clip: Mutex::default(),
                ipc_error: Mutex::new(ipc_error),
                shutdown: Mutex::new(shutdown),
            });
            maintain(app.handle().clone());
            let _ = app.emit("vault-state-changed", ());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::vault_status,
            commands::create_vault,
            commands::unlock,
            commands::lock,
            commands::change_password,
            commands::list_entries,
            commands::list_environments,
            commands::create_environment,
            commands::save_entry,
            commands::delete_entry,
            commands::delete_environment,
            commands::rename_environment,
            commands::reveal_entry,
            commands::copy_entry_value,
            commands::copy_reference,
            commands::list_pending_approvals,
            commands::respond_approval,
            commands::list_grants,
            commands::revoke_grant,
            commands::list_project_modes,
            commands::set_project_mode,
            commands::list_audit,
            commands::get_settings,
            commands::set_auto_lock,
            commands::note_activity,
            commands::get_ipc_error,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if let RunEvent::Exit = event
                && let Some(s) = app.try_state::<AppState>()
            {
                guard(&s.clip).clear_now(&AppClipboard(app));
                if let Some(stop) = guard(&s.shutdown).take() {
                    stop();
                }
            }
        });
}
