//! A report the player can send: the last frame, what was made of it, the log and the versions.

use crate::{hotkey, Shell};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};
use tauri_plugin_opener::OpenerExt;

/// Reports go under the player's documents; a test run keeps them beside its own settings.
fn home(app: &AppHandle, shell: &Shell) -> Result<PathBuf, String> {
    if std::env::var_os("BREACHER_CONFIG").is_some() {
        if let Some(dir) = shell.config_path.parent() {
            return Ok(dir.join("reports"));
        }
    }
    let docs = app.path().document_dir().or_else(|_| app.path().app_data_dir());
    Ok(docs.map_err(|e| e.to_string())?.join("Breacher reports"))
}

fn about(shell: &Shell) -> String {
    let mut settings = shell.settings.lock().unwrap().clone();
    // a folder path can name the player, and a report is meant to be passed on
    settings.watch_dir = None;
    let active = shell.hotkey.lock().unwrap().active;
    format!(
        "Breacher {}\nSystem: {} {}\nGame: {}\nHotkey: {}\nSettings: {}\n",
        shell.version,
        std::env::consts::OS,
        std::env::consts::ARCH,
        shell.game.lock().unwrap().as_deref().unwrap_or("not seen by a capture yet"),
        active.as_ref().map(hotkey::label).unwrap_or_else(|| "none".to_string()),
        serde_json::to_string(&settings).unwrap_or_default()
    )
}

/// Writes the report into a new folder named after `stamp` and returns the folder and the files in it.
pub fn write(app: &AppHandle, stamp: &str) -> Result<(PathBuf, Vec<&'static str>), String> {
    let shell = app.state::<Shell>();
    let name: String =
        stamp.chars().filter(|c| c.is_ascii_digit() || matches!(c, '-' | ' ')).take(24).collect();
    let dir = home(app, &shell)?.join(if name.trim().is_empty() { "report" } else { name.trim() });
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut files = Vec::new();
    let put = |file: &'static str, bytes: &[u8]| -> Result<&'static str, String> {
        std::fs::write(dir.join(file), bytes).map(|_| file).map_err(|e| e.to_string())
    };
    let frame = shell.last.lock().unwrap().as_ref().map(|f| (f.width, f.height, f.rgba.clone()));
    if let Some(image) = frame.and_then(|(w, h, rgba)| image::RgbaImage::from_raw(w, h, rgba)) {
        let rgb = image::DynamicImage::ImageRgba8(image).to_rgb8();
        rgb.save(dir.join("frame.png")).map_err(|e| e.to_string())?;
        files.push("frame.png");
    }
    if let Some(answer) = shell.last_answer.lock().unwrap().as_ref() {
        let json = serde_json::to_vec_pretty(answer).map_err(|e| e.to_string())?;
        files.push(put("answer.json", &json)?);
    }
    let looks = shell.looks.lock().unwrap().clone();
    for (i, (seconds, frame)) in looks.iter().enumerate() {
        let file = dir.join(format!("look-{:02}-at-{seconds}s.jpg", i + 1));
        std::fs::write(file, frame).map_err(|e| e.to_string())?;
    }
    if !looks.is_empty() {
        files.push("looks");
    }
    if let Ok(log) = std::fs::read(&shell.log_path) {
        files.push(put("breacher.log", &log)?);
    }
    files.push(put("about.txt", about(&shell).as_bytes())?);
    Ok((dir, files))
}

fn open(app: &AppHandle, dir: &Path) {
    let _ = app.opener().open_path(dir.display().to_string(), None::<&str>);
}

/// Saves a report, shows its folder and returns where it is.
#[tauri::command(async)]
pub fn save_report(app: AppHandle, stamp: String) -> Result<String, String> {
    let (dir, files) = write(&app, &stamp)?;
    crate::log(&app.state::<Shell>(), &format!("report saved: {} file(s)", files.len()));
    open(&app, &dir);
    Ok(dir.display().to_string())
}
