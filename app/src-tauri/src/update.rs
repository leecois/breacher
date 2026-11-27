//! Updates: asks the release page for a newer version and installs it over an installed copy.

use crate::{log, Shell};
use serde::Serialize;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_opener::OpenerExt;
use tauri_plugin_updater::{Update, UpdaterExt};

/// Where a copy that cannot replace itself sends the player.
const DOWNLOADS: &str = "https://github.com/leecois/breacher/releases/latest";

/// The update found by the last check, kept for the player's go-ahead.
#[derive(Default)]
pub struct Pending(Mutex<Option<Update>>);

#[derive(Clone, Serialize)]
pub struct UpdateInfo {
    version: String,
    /// False for a copy that was never installed: there is no installation for the installer to replace.
    installable: bool,
}

fn installed() -> bool {
    if cfg!(windows) {
        std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|dir| dir.join("uninstall.exe").exists()))
            .unwrap_or(false)
    } else {
        true
    }
}

/// The newer release, if the release page has one. An unreachable or empty page is an error, not an update.
pub async fn find(app: &AppHandle) -> Result<Option<Update>, String> {
    let builder = app.updater_builder().timeout(Duration::from_secs(20));
    // BREACHER_UPDATE_URL=<manifest> points a development build at a test manifest
    #[cfg(debug_assertions)]
    let builder = match std::env::var("BREACHER_UPDATE_URL").ok().and_then(|u| u.parse().ok()) {
        Some(url) => builder.endpoints(vec![url]).map_err(|e| e.to_string())?,
        None => builder,
    };
    builder.build().map_err(|e| e.to_string())?.check().await.map_err(|e| e.to_string())
}

/// Looks for a newer release. Finding none, for whatever reason, is quiet: it only goes to the log.
#[tauri::command]
pub async fn check_update(app: AppHandle) -> Option<UpdateInfo> {
    if !app.state::<Shell>().settings.lock().unwrap().updates {
        return None;
    }
    let found = find(&app).await;
    let shell = app.state::<Shell>();
    match found {
        Ok(Some(update)) => {
            log(&shell, &format!("update check: {} is available", update.version));
            let info = UpdateInfo { version: update.version.clone(), installable: installed() };
            *app.state::<Pending>().0.lock().unwrap() = Some(update);
            Some(info)
        }
        Ok(None) => {
            log(&shell, "update check: up to date");
            None
        }
        Err(e) => {
            log(&shell, &format!("update check: nothing found ({e})"));
            None
        }
    }
}

/// Downloads the waiting update, checks its signature, installs it and starts the new version.
#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    let waiting = app.state::<Pending>().0.lock().unwrap().clone();
    let Some(update) = waiting else {
        return Err("No update is waiting.".to_string());
    };
    if !installed() {
        return Err("This copy was not installed, so it cannot replace itself.".to_string());
    }
    log(&app.state::<Shell>(), &format!("update: installing {}", update.version));
    let page = app.clone();
    let (mut got, mut shown) = (0u64, 0u64);
    let result = update
        .download_and_install(
            move |chunk, total| {
                got += chunk as u64;
                let percent = total.filter(|t| *t > 0).map_or(0, |t| (got * 100 / t).min(100));
                if percent != shown {
                    shown = percent;
                    let _ = page.emit_to("main", "update-progress", percent);
                }
            },
            || {},
        )
        .await;
    if let Err(e) = result {
        log(&app.state::<Shell>(), &format!("update: failed ({e})"));
        return Err(e.to_string());
    }
    app.restart()
}

#[tauri::command]
pub fn open_downloads(app: AppHandle) -> Result<(), String> {
    app.opener().open_url(DOWNLOADS, None::<&str>).map_err(|e| e.to_string())
}
