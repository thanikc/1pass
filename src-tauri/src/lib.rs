pub mod autolock;
pub mod broker;
pub mod cli_install;
pub mod clipboard;
mod commands;
pub mod env_import;
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
use tauri::{
    Emitter, Manager, RunEvent, WindowEvent,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent},
};

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

/// libappindicator-sys panics (an abort in release) instead of erroring when no appindicator
/// library is installed, so check it loads before building the tray. Same names it tries.
#[cfg(target_os = "linux")]
fn tray_library_available() -> bool {
    [
        c"libayatana-appindicator3.so.1",
        c"libappindicator3.so.1",
        c"libayatana-appindicator3.so",
        c"libappindicator3.so",
    ]
    .iter()
    .any(|name| {
        // SAFETY: valid NUL-terminated name; the handle is closed right away.
        let h = unsafe { libc::dlopen(name.as_ptr(), libc::RTLD_LAZY | libc::RTLD_LOCAL) };
        !h.is_null() && unsafe { libc::dlclose(h) } == 0
    })
}

#[cfg(not(target_os = "linux"))]
fn tray_library_available() -> bool {
    true
}

/// Tray icon with Open / Lock / Quit. Quit goes through `app.exit`, which runs the `RunEvent::Exit` cleanup.
fn build_tray(app: &tauri::App) -> tauri::Result<TrayIcon> {
    if !tray_library_available() {
        return Err(tauri::Error::Io(std::io::Error::other(
            "no appindicator library",
        )));
    }
    let menu = Menu::with_items(
        app,
        &[
            &MenuItem::with_id(app, "open", "Open", true, None::<&str>)?,
            &MenuItem::with_id(app, "lock", "Lock", true, None::<&str>)?,
            &MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?,
        ],
    )?;
    let mut tray = TrayIconBuilder::new()
        .tooltip("one-pass")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => host::raise(app),
            "lock" => app.state::<AppState>().lock_now(),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                host::raise(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            host::raise(app)
        }))
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
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
                env_file: Mutex::new(None),
            });
            // Managed so the icon lives as long as the app; its presence lets a close hide the window.
            if let Ok(tray) = build_tray(app) {
                app.manage(tray);
            }
            maintain(app.handle().clone());
            let _ = app.emit("vault-state-changed", ());
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event
                && window.label() == "main"
                && window.try_state::<TrayIcon>().is_some()
            {
                api.prevent_close();
                let _ = window.hide();
            }
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
            commands::pick_env_file,
            commands::import_env,
            commands::copy_env_references,
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
            commands::cli_install_target,
            commands::install_cli,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            match event {
                RunEvent::Exit => {
                    if let Some(s) = app.try_state::<AppState>() {
                        guard(&s.clip).clear_now(&AppClipboard(app));
                        if let Some(stop) = guard(&s.shutdown).take() {
                            stop();
                        }
                    }
                }
                // Dock icon clicked while the window is hidden in the tray.
                #[cfg(target_os = "macos")]
                RunEvent::Reopen { .. } => host::raise(app),
                _ => {}
            }
        });
}
