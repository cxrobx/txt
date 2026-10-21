//! Updates from GitHub Releases (`latest.json`, signed with the key in
//! `~/.tauri/txt.key`). Checked a minute after launch and then daily, since
//! txt stays running with no windows; also from txt → Check for Updates….
//! An update downloads in the background and is installed in place; txt asks
//! before restarting, and closes every window first so each one saves.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use tauri_plugin_updater::UpdaterExt;

static BUSY: AtomicBool = AtomicBool::new(false);
/// The version already installed and waiting for a restart.
static READY: Mutex<Option<String>> = Mutex::new(None);

pub fn start(app: AppHandle) {
    // Dev builds and test runs never update themselves.
    if cfg!(debug_assertions) || std::env::var_os("TXT_NO_UPDATE").is_some() {
        return;
    }
    tauri::async_runtime::spawn(async move {
        tokio_sleep(Duration::from_secs(60)).await;
        loop {
            check(app.clone(), false).await;
            tokio_sleep(Duration::from_secs(24 * 3600)).await;
        }
    });
}

async fn tokio_sleep(d: Duration) {
    let _ = tauri::async_runtime::spawn_blocking(move || std::thread::sleep(d)).await;
}

/// `manual`: the menu item, which reports "up to date" and errors too.
pub async fn check(app: AppHandle, manual: bool) {
    if let Some(version) = READY.lock().unwrap().clone() {
        return ask_restart(&app, &version);
    }
    if BUSY.swap(true, Ordering::SeqCst) {
        return;
    }
    let result = install(&app).await;
    BUSY.store(false, Ordering::SeqCst);
    match result {
        Ok(Some(version)) => {
            *READY.lock().unwrap() = Some(version.clone());
            ask_restart(&app, &version);
        }
        Ok(None) if manual => tell(&app, "You're up to date", &format!("txt {} is the latest version.", app.package_info().version)),
        Err(e) if manual => tell(&app, "Couldn't check for updates", &e),
        _ => {}
    }
}

async fn install(app: &AppHandle) -> Result<Option<String>, String> {
    let updater = app.updater().map_err(|e| e.to_string())?;
    let Some(update) = updater.check().await.map_err(|e| e.to_string())? else {
        return Ok(None);
    };
    let version = update.version.clone();
    update.download_and_install(|_, _| {}, || {}).await.map_err(|e| e.to_string())?;
    Ok(Some(version))
}

fn tell(app: &AppHandle, title: &str, text: &str) {
    app.dialog().message(text).title(title).kind(MessageDialogKind::Info).show(|_| {});
}

fn ask_restart(app: &AppHandle, version: &str) {
    let handle = app.clone();
    app.dialog()
        .message(format!("txt {version} is installed. Restart now? Your notes are saved first."))
        .title("Update ready")
        .buttons(MessageDialogButtons::OkCancelCustom("Restart".into(), "Later".into()))
        .show(move |restart| {
            if restart {
                std::thread::spawn(move || restart_after_saving(handle));
            }
        });
}

/// Closing a window runs its save-on-close handler; wait (up to 5 s) for all
/// of them, then relaunch.
fn restart_after_saving(app: AppHandle) {
    for w in app.webview_windows().values() {
        let _ = w.close();
    }
    for _ in 0..50 {
        if app.webview_windows().is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    app.restart();
}
