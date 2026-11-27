//! Desktop shell: a global hotkey that captures and reads the game, an overlay window over it, and a watched screenshot folder.

use base64::Engine;
use breach_capture::{CaptureError, Permission, Target};
use breach_core::api::{self, Response};
use breach_core::glyph::Templates;
use breach_core::image::{Frame, PixelFormat};
use breach_core::plan::{PlanOptions, Priority, Route};
use breach_core::reader::{ReadOptions, Stage};
use breach_core::solver::Matcher;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, PhysicalPosition, PhysicalSize,
    State,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

const HOTKEY: &str = "CmdOrCtrl+Shift+B";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
struct Settings {
    priority: Priority,
    /// Draw the route over the game after a hotkey capture.
    overlay: bool,
    /// Seconds the overlay stays up.
    overlay_seconds: u64,
    target: Target,
    watch_dir: Option<PathBuf>,
    /// The wrong-pick rule, once confirmed in game (docs/RULE-CHECK.md). `None` = play safe.
    matcher: Option<Matcher>,
    /// Buffer size last read from the screen: assumed when a later frame's buffer panel is unreadable.
    last_buffer: Option<usize>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            priority: Priority::Count,
            overlay: true,
            overlay_seconds: 45,
            target: Target::Auto,
            watch_dir: None,
            matcher: None,
            last_buffer: None,
        }
    }
}

struct LastFrame {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

struct Shell {
    settings: Mutex<Settings>,
    config_path: PathBuf,
    /// Plain-text log next to the settings: what was captured and what came of it.
    log_path: PathBuf,
    templates: Templates,
    last: Mutex<Option<LastFrame>>,
    /// Bumped on every capture; a pending "clear the overlay" only fires for its own capture.
    generation: AtomicU64,
    watcher: Mutex<Option<RecommendedWatcher>>,
    /// Last file handled by the folder watcher (events arrive in bursts).
    last_file: Mutex<Option<(PathBuf, Instant)>>,
    hotkey_error: Mutex<Option<String>>,
}

#[derive(Clone, Serialize)]
struct FrameMeta {
    width: u32,
    height: u32,
    source: String,
}

#[derive(Clone, Serialize)]
struct Answer {
    response: Response,
    frame: FrameMeta,
    elapsed_ms: f32,
}

#[derive(Clone, Serialize)]
struct OverlayPayload {
    response: Response,
    frame: FrameMeta,
    route: Option<Route>,
}

#[derive(Clone, Serialize)]
struct ShellStatus {
    matcher: Option<Matcher>,
    /// The hotkey as the player would read it.
    hotkey: String,
    capture_ready: bool,
    capture_note: String,
    watch_dir: Option<String>,
    overlay: bool,
    log_path: String,
}

/// Appends one line to the log file, restarting it once it passes 256 KB.
fn log(shell: &Shell, line: &str) {
    use std::io::Write;
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    if std::env::var_os("BREACHER_LOG").is_some() {
        println!("{line}");
    }
    if let Some(dir) = shell.log_path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if std::fs::metadata(&shell.log_path).map(|m| m.len() > 256 * 1024).unwrap_or(false) {
        let _ = std::fs::remove_file(&shell.log_path);
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&shell.log_path) {
        let _ = writeln!(f, "[{secs}] {line}");
    }
}

fn hotkey_label() -> String {
    if cfg!(target_os = "macos") {
        "⌘ ⇧ B".into()
    } else {
        "Ctrl Shift B".into()
    }
}

fn status_of(shell: &Shell) -> ShellStatus {
    let settings = shell.settings.lock().unwrap().clone();
    let hotkey_error = shell.hotkey_error.lock().unwrap().clone();
    let (capture_ready, capture_note) = match (breach_capture::permission(), hotkey_error) {
        (Permission::Denied, _) => (
            false,
            "Screen capture is off for Breacher. Allow it to capture the game with the hotkey."
                .to_string(),
        ),
        (Permission::Granted, Some(e)) => (
            true,
            format!("The hotkey could not be registered ({e}). Use the Capture game button."),
        ),
        (Permission::Granted, None) => (true, String::new()),
    };
    ShellStatus {
        matcher: settings.matcher,
        hotkey: hotkey_label(),
        capture_ready,
        capture_note,
        watch_dir: settings.watch_dir.map(|p| p.display().to_string()),
        overlay: settings.overlay,
        log_path: shell.log_path.display().to_string(),
    }
}

fn save(shell: &Shell) {
    let settings = shell.settings.lock().unwrap().clone();
    if let Some(dir) = shell.config_path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_string_pretty(&settings) {
        let _ = std::fs::write(&shell.config_path, json);
    }
}

fn rejected(message: String, hint: &str) -> Response {
    Response::Rejected { stage: Stage::Frame, message, hint: hint.to_string() }
}

/// Read pixels, tell the main window, remember the frame.
fn answer(
    app: &AppHandle,
    rgba: Vec<u8>,
    width: u32,
    height: u32,
    source: String,
    started: Instant,
) -> (Response, FrameMeta) {
    let shell = app.state::<Shell>();
    let (priority, matcher, last_buffer) = {
        let s = shell.settings.lock().unwrap();
        (s.priority, s.matcher, s.last_buffer)
    };
    let frame = Frame::new(width as usize, height as usize, PixelFormat::Rgba8, &rgba);
    // 4 is the smallest buffer in the game: assuming too few slots can cost a daemon, never a breach
    let read_opt =
        ReadOptions { buffer_fallback: Some(last_buffer.unwrap_or(4)), ..ReadOptions::default() };
    let response = api::read_and_plan(
        &frame,
        &shell.templates,
        &read_opt,
        &PlanOptions { priority, matcher, ..PlanOptions::default() },
    );
    if let Response::Ok { reading, .. } = &response {
        if !reading.quality.buffer_assumed && last_buffer != Some(reading.buffer) {
            shell.settings.lock().unwrap().last_buffer = Some(reading.buffer);
            save(&shell);
        }
    }
    let meta = FrameMeta { width, height, source };
    let elapsed_ms = started.elapsed().as_secs_f32() * 1e3;
    let what = match &response {
        Response::Ok { reading, plan } => format!(
            "ok {}x{} buffer {}{} picks {} uploads {:?}{}",
            reading.n,
            reading.n,
            reading.buffer,
            if reading.quality.buffer_assumed { " (assumed)" } else { "" },
            plan.route.steps.len(),
            plan.route.uploaded,
            if reading.quality.layout_fallback { " [unusual layout]" } else { "" }
        ),
        Response::Rejected { stage, message, .. } => format!("rejected at {stage:?}: {message}"),
    };
    log(
        &shell,
        &format!("answer: {} ({}x{}) {what} in {elapsed_ms:.1} ms", meta.source, width, height),
    );
    let _ = app.emit_to(
        "main",
        "answer",
        Answer { response: response.clone(), frame: meta.clone(), elapsed_ms },
    );
    *shell.last.lock().unwrap() = Some(LastFrame { width, height, rgba });
    (response, meta)
}

fn clear_overlay(app: &AppHandle) {
    let _ = app.emit_to("overlay", "clear", ());
}

/// Capture the game, read it, show the route in the main window and over the game.
fn run_capture(app: &AppHandle) {
    let shell = app.state::<Shell>();
    let settings = shell.settings.lock().unwrap().clone();
    let generation = shell.generation.fetch_add(1, Ordering::SeqCst) + 1;
    // our own drawing must not be in the frame (it would be when a whole display is captured)
    clear_overlay(app);
    if !matches!(settings.target, Target::Window(_)) {
        std::thread::sleep(Duration::from_millis(60));
    }
    let started = Instant::now();
    let cap = match breach_capture::capture(&settings.target) {
        Ok(c) => c,
        Err(e) => {
            log(&shell, &format!("capture failed (target {:?}): {e}", settings.target));
            let hint = match e {
                CaptureError::Permission => "Screen capture is off for Breacher. Allow it in the system's privacy settings, then start Breacher again.",
                CaptureError::NoTarget(_) => "The window to capture is not open.",
                CaptureError::Backend(_) => "The screen could not be captured. Drop a screenshot here instead.",
            };
            let _ = app.emit_to(
                "main",
                "answer",
                Answer {
                    response: rejected(e.to_string(), hint),
                    frame: FrameMeta { width: 0, height: 0, source: "Capture".into() },
                    elapsed_ms: 0.0,
                },
            );
            return;
        }
    };
    let (x, y, sw, sh, logical) =
        (cap.x, cap.y, cap.screen_width, cap.screen_height, cap.logical_units);
    log(
        &shell,
        &format!(
            "captured {} in {:.1} ms: {}x{} px, on screen at ({x}, {y}) size {sw}x{sh} ({})",
            cap.source,
            cap.capture_ms,
            cap.width,
            cap.height,
            if logical { "points" } else { "pixels" }
        ),
    );
    let (response, meta) = answer(app, cap.rgba, cap.width, cap.height, cap.source, started);
    let Response::Ok { plan, .. } = &response else { return };
    if !settings.overlay {
        return;
    }
    if let Some(overlay) = app.get_webview_window("overlay") {
        if logical {
            let _ = overlay.set_position(LogicalPosition::new(x as f64, y as f64));
            let _ = overlay.set_size(LogicalSize::new(sw as f64, sh as f64));
        } else {
            let _ = overlay.set_position(PhysicalPosition::new(x, y));
            let _ = overlay.set_size(PhysicalSize::new(sw, sh));
        }
        let _ = overlay.set_ignore_cursor_events(true);
        if !overlay.is_visible().unwrap_or(false) {
            let _ = overlay.show();
        }
        let route = Some(plan.route.clone());
        let _ = app.emit_to(
            "overlay",
            "route",
            OverlayPayload { response: response.clone(), frame: meta, route },
        );
        let app = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(settings.overlay_seconds.max(3)));
            if app.state::<Shell>().generation.load(Ordering::SeqCst) == generation {
                clear_overlay(&app);
            }
        });
    }
}

fn is_image(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()).map(str::to_lowercase).as_deref(),
        Some("png" | "jpg" | "jpeg" | "bmp" | "webp")
    )
}

/// A new screenshot appeared in the watched folder.
fn on_new_file(app: &AppHandle, path: PathBuf) {
    if !is_image(&path) {
        return;
    }
    let shell = app.state::<Shell>();
    {
        let mut last = shell.last_file.lock().unwrap();
        if let Some((p, t)) = last.as_ref() {
            if *p == path && t.elapsed() < Duration::from_secs(2) {
                return;
            }
        }
        *last = Some((path.clone(), Instant::now()));
    }
    let app = app.clone();
    std::thread::spawn(move || {
        // the writer may still be flushing: retry briefly until the file decodes
        for attempt in 0..8 {
            std::thread::sleep(Duration::from_millis(if attempt == 0 { 120 } else { 250 }));
            let started = Instant::now();
            if let Ok(img) = image::open(&path) {
                let rgba = img.to_rgba8();
                let (w, h) = (rgba.width(), rgba.height());
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "Screenshot".into());
                answer(&app, rgba.into_raw(), w, h, name, started);
                return;
            }
        }
    });
}

fn start_watching(app: &AppHandle, dir: Option<PathBuf>) -> Result<(), String> {
    let shell = app.state::<Shell>();
    *shell.watcher.lock().unwrap() = None;
    let Some(dir) = dir else { return Ok(()) };
    if !dir.is_dir() {
        return Err(format!("{} is not a folder", dir.display()));
    }
    let handle = app.clone();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(event) = res {
            if matches!(
                event.kind,
                notify::EventKind::Create(_)
                    | notify::EventKind::Modify(notify::event::ModifyKind::Name(_))
                    | notify::EventKind::Modify(notify::event::ModifyKind::Data(_))
            ) {
                for p in event.paths {
                    if p.is_file() {
                        on_new_file(&handle, p);
                    }
                }
            }
        }
    })
    .map_err(|e| e.to_string())?;
    watcher.watch(&dir, RecursiveMode::NonRecursive).map_err(|e| e.to_string())?;
    *shell.watcher.lock().unwrap() = Some(watcher);
    Ok(())
}

#[tauri::command]
fn status(shell: State<Shell>) -> ShellStatus {
    status_of(&shell)
}

#[tauri::command]
fn capture(app: AppHandle) {
    std::thread::spawn(move || run_capture(&app));
}

/// Ask the system for screen-capture permission. Only ever called from the player's own click.
#[tauri::command]
fn allow_capture(shell: State<Shell>) -> ShellStatus {
    breach_capture::request_permission();
    status_of(&shell)
}

#[tauri::command]
fn set_priority(shell: State<Shell>, priority: Priority) {
    shell.settings.lock().unwrap().priority = priority;
    save(&shell);
}

#[tauri::command]
fn set_matcher(shell: State<Shell>, matcher: Option<Matcher>) {
    shell.settings.lock().unwrap().matcher = matcher;
    save(&shell);
}

#[tauri::command]
fn set_overlay(app: AppHandle, shell: State<Shell>, enabled: bool) -> ShellStatus {
    shell.settings.lock().unwrap().overlay = enabled;
    save(&shell);
    if !enabled {
        clear_overlay(&app);
    }
    status_of(&shell)
}

#[tauri::command]
fn watch_folder(
    app: AppHandle,
    shell: State<Shell>,
    dir: Option<String>,
) -> Result<ShellStatus, String> {
    let dir = dir.map(PathBuf::from);
    start_watching(&app, dir.clone())?;
    shell.settings.lock().unwrap().watch_dir = dir;
    save(&shell);
    Ok(status_of(&shell))
}

/// The last frame as a JPEG data URL, scaled down for display.
#[tauri::command]
fn last_frame_url(shell: State<Shell>) -> Option<String> {
    let guard = shell.last.lock().unwrap();
    let last = guard.as_ref()?;
    let img = image::RgbaImage::from_raw(last.width, last.height, last.rgba.clone())?;
    let rgb = image::DynamicImage::ImageRgba8(img).to_rgb8();
    let rgb = if rgb.width() > 1920 {
        let h = (rgb.height() as u64 * 1920 / rgb.width() as u64) as u32;
        image::imageops::resize(&rgb, 1920, h.max(1), image::imageops::FilterType::Triangle)
    } else {
        rgb
    };
    let mut buf = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, 85).encode_image(&rgb).ok()?;
    Some(format!(
        "data:image/jpeg;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(buf)
    ))
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        let app = app.clone();
                        std::thread::spawn(move || run_capture(&app));
                    }
                })
                .build(),
        )
        .setup(|app| {
            // BREACHER_CONFIG=<file> keeps tests away from the real settings
            let config_path = std::env::var_os("BREACHER_CONFIG")
                .map(PathBuf::from)
                .unwrap_or_else(|| app.path().app_config_dir().map(|d| d.join("settings.json")).unwrap_or_else(|_| PathBuf::from("breacher-settings.json")));
            let settings: Settings = std::fs::read_to_string(&config_path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
            let watch_dir = settings.watch_dir.clone();
            let log_path = config_path.with_file_name("breacher.log");
            app.manage(Shell {
                settings: Mutex::new(settings),
                config_path,
                log_path,
                templates: Templates::builtin(),
                last: Mutex::new(None),
                generation: AtomicU64::new(0),
                watcher: Mutex::new(None),
                last_file: Mutex::new(None),
                hotkey_error: Mutex::new(None),
            });
            let hotkey = app.global_shortcut().register(HOTKEY);
            {
                let shell = app.state::<Shell>();
                log(
                    &shell,
                    &format!(
                        "started Breacher {} on {} {}; hotkey {HOTKEY}: {}; capture permission {:?}",
                        env!("CARGO_PKG_VERSION"),
                        std::env::consts::OS,
                        std::env::consts::ARCH,
                        hotkey.as_ref().map(|_| "registered".to_string()).unwrap_or_else(|e| format!("FAILED ({e})")),
                        breach_capture::permission()
                    ),
                );
                if let Err(e) = hotkey {
                    *shell.hotkey_error.lock().unwrap() = Some(e.to_string());
                }
            }
            if let Some(overlay) = app.get_webview_window("overlay") {
                let _ = overlay.set_ignore_cursor_events(true);
            }
            if let Some(dir) = watch_dir {
                let _ = start_watching(app.handle(), Some(dir));
            }
            // BREACHER_SELFTEST=<image>: run the native pipeline once on a file, print the outcome, quit.
            if let Some(path) = std::env::var_os("BREACHER_SELFTEST") {
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(400));
                    let started = Instant::now();
                    let line = match image::open(&path) {
                        Err(e) => format!("selftest: cannot open image: {e}"),
                        Ok(img) => {
                            let rgba = img.to_rgba8();
                            let (w, h) = (rgba.width(), rgba.height());
                            let (response, meta) = answer(&handle, rgba.into_raw(), w, h, "selftest".into(), started);
                            let shell = handle.state::<Shell>();
                            let summary = match &response {
                                Response::Ok { reading, plan } => format!("ok n={} buffer={} picks={} uploaded={:?}", reading.n, reading.buffer, plan.route.steps.len(), plan.route.uploaded),
                                Response::Rejected { stage, .. } => format!("rejected at {stage:?}"),
                            };
                            format!(
                                "selftest: {summary} | frame {}x{} | {:.1} ms | hotkey error: {:?} | windows: main={} overlay={} | capture permission: {:?} | frame url: {}",
                                meta.width,
                                meta.height,
                                started.elapsed().as_secs_f32() * 1e3,
                                shell.hotkey_error.lock().unwrap(),
                                handle.get_webview_window("main").is_some(),
                                handle.get_webview_window("overlay").is_some(),
                                breach_capture::permission(),
                                last_frame_url(shell.clone()).map(|u| u.len()).unwrap_or(0)
                            )
                        }
                    };
                    println!("{line}");
                    handle.exit(0);
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![status, capture, allow_capture, set_priority, set_matcher, set_overlay, watch_folder, last_frame_url])
        .run(tauri::generate_context!())
        .expect("error while running Breacher");
}
