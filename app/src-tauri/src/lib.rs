//! Desktop shell: a global hotkey that captures and reads the game, an overlay window over it, and a watched screenshot folder.

mod follow;
mod hotkey;
mod update;

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
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::menu::MenuBuilder;
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, PhysicalPosition, PhysicalSize,
    State, WindowEvent,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
struct Settings {
    priority: Priority,
    /// Draw the route over the game after a hotkey capture.
    overlay: bool,
    /// Seconds the overlay stays up when the breach is not followed.
    overlay_seconds: u64,
    /// Take the route down when the breach screen closes. `None` = wherever looking again goes unnoticed.
    overlay_follow: Option<bool>,
    target: Target,
    watch_dir: Option<PathBuf>,
    /// The wrong-pick rule, once confirmed in game (docs/RULE-CHECK.md). `None` = play safe.
    matcher: Option<Matcher>,
    /// Buffer size last read from the screen: assumed when a later frame's buffer panel is unreadable.
    last_buffer: Option<usize>,
    /// The capture hotkey the player chose. `None` = the first default that is free.
    hotkey: Option<String>,
    /// Ask the release page for a newer version at start.
    updates: bool,
    /// Keep running in the tray, hotkey and all, when the main window is closed.
    tray: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            priority: Priority::Count,
            overlay: true,
            overlay_seconds: 45,
            overlay_follow: None,
            target: Target::Auto,
            watch_dir: None,
            matcher: None,
            last_buffer: None,
            hotkey: None,
            updates: true,
            tray: true,
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
    hotkey: Mutex<hotkey::Binding>,
    /// This program's own version, as the updater compares it.
    version: String,
    /// Whether the system gave a tray icon: without one a closed window could not be brought back.
    tray: AtomicBool,
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
    version: String,
    matcher: Option<Matcher>,
    /// The bound hotkey as the player would read it; empty when none could be bound.
    hotkey: String,
    capture_ready: bool,
    capture_note: String,
    watch_dir: Option<String>,
    overlay: bool,
    updates: bool,
    /// Whether closing the window keeps the program in the tray; `None` where there is no tray.
    tray: Option<bool>,
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

fn status_of(shell: &Shell) -> ShellStatus {
    let settings = shell.settings.lock().unwrap().clone();
    let binding = *shell.hotkey.lock().unwrap();
    let capture_ready = breach_capture::permission() == Permission::Granted;
    let capture_note = match (capture_ready, binding.active, binding.refused) {
        (false, ..) => "Screen capture is off for Breacher. Allow it to capture the game.".into(),
        (true, Some(active), Some(refused)) => format!(
            "{} belongs to another program, so the hotkey is {}.",
            hotkey::label(&refused),
            hotkey::label(&active)
        ),
        (true, None, _) => {
            "No hotkey is free. Choose one with Change, or use the Capture game button.".into()
        }
        (true, Some(_), None) => String::new(),
    };
    ShellStatus {
        version: shell.version.clone(),
        matcher: settings.matcher,
        hotkey: binding.active.as_ref().map(hotkey::label).unwrap_or_default(),
        capture_ready,
        capture_note,
        watch_dir: settings.watch_dir.map(|p| p.display().to_string()),
        overlay: settings.overlay,
        updates: settings.updates,
        tray: shell.tray.load(Ordering::SeqCst).then_some(settings.tray),
        log_path: shell.log_path.display().to_string(),
    }
}

/// What the player does next to capture from inside the game.
fn next_step(shell: &Shell) -> String {
    match shell.hotkey.lock().unwrap().active {
        Some(active) => format!("then press {}", hotkey::label(&active)),
        None => "then capture again".to_string(),
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

/// Read pixels, tell the main window, remember the frame. `lost` replaces the hint when the frame holds no breach screen.
fn answer(
    app: &AppHandle,
    rgba: Vec<u8>,
    width: u32,
    height: u32,
    source: String,
    started: Instant,
    lost: Option<String>,
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
    let response = match (response, lost) {
        (
            Response::Rejected {
                stage: stage @ (Stage::Segment | Stage::Lattice), message, ..
            },
            Some(hint),
        ) => Response::Rejected { stage, message, hint },
        (response, _) => response,
    };
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
    // stored first: the page asks for this frame as soon as it hears the answer
    *shell.last.lock().unwrap() = Some(LastFrame { width, height, rgba });
    let _ = app.emit_to(
        "main",
        "answer",
        Answer { response: response.clone(), frame: meta.clone(), elapsed_ms },
    );
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
                CaptureError::Permission => "Screen capture is off for Breacher. Allow it in the system's privacy settings, then start Breacher again.".to_string(),
                CaptureError::NoTarget(_) => "The window to capture is not open.".to_string(),
                CaptureError::Minimised => format!("The game is minimised. Switch to it and open a breach, {}.", next_step(&shell)),
                CaptureError::Backend(_) => "The screen could not be captured. Drop a screenshot here instead.".to_string(),
            };
            let _ = app.emit_to(
                "main",
                "answer",
                Answer {
                    response: rejected(e.to_string(), &hint),
                    frame: FrameMeta { width: 0, height: 0, source: "Capture".into() },
                    elapsed_ms: 0.0,
                },
            );
            return;
        }
    };
    let (x, y, sw, sh, logical) =
        (cap.x, cap.y, cap.screen_width, cap.screen_height, cap.logical_units);
    if let Some(program) = cap.program {
        log(&shell, &format!("game version {}", program.label()));
        let _ = app.emit_to("main", "game-version", program.label());
    }
    let follow = settings.overlay_follow.unwrap_or_else(breach_capture::recapture_is_quiet);
    let again = cap.again.filter(|_| follow);
    log(
        &shell,
        &format!(
            "captured {} in {:.1} ms: {}x{} px, on screen at ({x}, {y}) size {sw}x{sh} ({}){}",
            cap.source,
            cap.capture_ms,
            cap.width,
            cap.height,
            if logical { "points" } else { "pixels" },
            if cap.fallback { "; no game window found" } else { "" }
        ),
    );
    let lost = cap.fallback.then(|| {
        format!(
            "The game window was not found, so the whole screen was captured. Open a breach in the game, {}.",
            next_step(&shell)
        )
    });
    let (response, meta) = answer(app, cap.rgba, cap.width, cap.height, cap.source, started, lost);
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
        log(
            &shell,
            match &again {
                Some(_) => "route drawn over the game, up until the breach screen closes",
                None => "route drawn over the game, up for a fixed time",
            },
        );
        let app = app.clone();
        std::thread::spawn(move || match again {
            Some(game) => follow_breach(&app, &game, generation),
            None => {
                std::thread::sleep(Duration::from_secs(settings.overlay_seconds.max(3)));
                if app.state::<Shell>().generation.load(Ordering::SeqCst) == generation {
                    clear_overlay(&app);
                }
            }
        });
    }
}

/// Keeps the route up for as long as the breach screen is, and takes it down when the screen goes.
fn follow_breach(app: &AppHandle, game: &breach_capture::Source, generation: u64) {
    let shell = app.state::<Shell>();
    let started = Instant::now();
    let ours = || shell.generation.load(Ordering::SeqCst) == generation;
    let why = follow::until_gone(|| {
        std::thread::sleep(follow::INTERVAL);
        if !ours() {
            return None;
        }
        Some(match game.grab() {
            Ok(cap) => {
                let frame = Frame::new(
                    cap.width as usize,
                    cap.height as usize,
                    PixelFormat::Rgba8,
                    &cap.rgba,
                );
                if api::breach_on_screen(&frame) {
                    follow::Look::Breach
                } else {
                    follow::Look::Other
                }
            }
            Err(CaptureError::Minimised) => follow::Look::Minimised,
            Err(_) => follow::Look::Failed,
        })
    });
    if let Some(why) = why.filter(|_| ours()) {
        clear_overlay(app);
        log(
            &shell,
            &format!("route taken down after {:.0} s: {why}", started.elapsed().as_secs_f32()),
        );
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
                answer(&app, rgba.into_raw(), w, h, name, started, None);
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
        dismiss_overlay(app, shell.clone());
    }
    status_of(&shell)
}

/// Wipes the route from the game and ends any watch that would have done it later.
#[tauri::command]
fn dismiss_overlay(app: AppHandle, shell: State<Shell>) {
    shell.generation.fetch_add(1, Ordering::SeqCst);
    clear_overlay(&app);
}

#[tauri::command]
fn set_tray(shell: State<Shell>, enabled: bool) -> ShellStatus {
    shell.settings.lock().unwrap().tray = enabled;
    save(&shell);
    status_of(&shell)
}

#[tauri::command]
fn set_updates(shell: State<Shell>, enabled: bool) -> ShellStatus {
    shell.settings.lock().unwrap().updates = enabled;
    save(&shell);
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

/// Lets go of the hotkey while the player chooses a new one, so the page can see the keys.
#[tauri::command(async)]
fn pause_hotkey(app: AppHandle, shell: State<Shell>) {
    // read first: unregistering waits for the main thread, which may itself want this lock
    let active = shell.hotkey.lock().unwrap().active;
    if let Some(active) = active {
        let _ = app.global_shortcut().unregister(active);
    }
}

/// Binds the combination the player chose. `None` takes the current one back after a cancelled change.
#[tauri::command(async)]
fn set_hotkey(
    app: AppHandle,
    shell: State<Shell>,
    accelerator: Option<String>,
) -> Result<ShellStatus, String> {
    let shortcuts = app.global_shortcut();
    let current = shell.hotkey.lock().unwrap().active;
    let restore = || {
        if let Some(c) = current.filter(|c| !shortcuts.is_registered(*c)) {
            if shortcuts.register(c).is_err() {
                shell.hotkey.lock().unwrap().active = None;
            }
        }
    };
    let Some(accelerator) = accelerator else {
        restore();
        return Ok(status_of(&shell));
    };
    let chosen = match hotkey::parse(&accelerator).and_then(|s| hotkey::usable(&s).map(|_| s)) {
        Ok(chosen) => chosen,
        Err(e) => {
            restore();
            return Err(e);
        }
    };
    if !shortcuts.is_registered(chosen) {
        if let Some(c) = current.filter(|c| shortcuts.is_registered(*c)) {
            let _ = shortcuts.unregister(c);
        }
        if let Err(e) = shortcuts.register(chosen) {
            log(&shell, &format!("hotkey {} refused: {e}", hotkey::label(&chosen)));
            restore();
            return Err(format!(
                "{} belongs to another program. Try another combination.",
                hotkey::label(&chosen)
            ));
        }
    }
    *shell.hotkey.lock().unwrap() = hotkey::Binding { active: Some(chosen), refused: None };
    shell.settings.lock().unwrap().hotkey = Some(chosen.to_string());
    save(&shell);
    log(&shell, &format!("hotkey set to {}", hotkey::label(&chosen)));
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

/// Self-test of a hotkey change: an unusable choice is refused, a usable one is bound, the first one comes back.
fn selftest_hotkey(app: &AppHandle) -> String {
    let shell = app.state::<Shell>();
    let Some(before) = shell.hotkey.lock().unwrap().active else {
        return "none bound".to_string();
    };
    pause_hotkey(app.clone(), shell.clone());
    let refused = set_hotkey(app.clone(), shell.clone(), Some("Shift+KeyB".into())).is_err();
    let kept = app.global_shortcut().is_registered(before);
    let changed = set_hotkey(app.clone(), shell.clone(), Some("Control+Alt+F9".into()))
        .map(|s| s.hotkey)
        .unwrap_or_else(|e| format!("FAILED ({e})"));
    let released = !app.global_shortcut().is_registered(before);
    let back = set_hotkey(app.clone(), shell.clone(), Some(before.to_string()))
        .map(|s| s.hotkey)
        .unwrap_or_else(|e| format!("FAILED ({e})"));
    format!("{back} | change: bare key refused={refused} kept={kept}, bound {changed} released={released}, back to {back}")
}

fn show_main(app: &AppHandle) {
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.unminimize();
        let _ = main.show();
        let _ = main.set_focus();
    }
}

/// Puts the program in the tray: a click brings the window back, the menu can end the program.
fn add_tray(app: &AppHandle) -> tauri::Result<()> {
    let menu = MenuBuilder::new(app)
        .text("open", "Open Breacher")
        .separator()
        .text("quit", "Quit Breacher")
        .build()?;
    let mut tray = TrayIconBuilder::with_id("breacher")
        .tooltip("Breacher")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_main(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            let released = MouseButtonState::Up;
            if matches!(event, TrayIconEvent::Click { button: MouseButton::Left, button_state, .. } if button_state == released)
            {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app).map(|_| ())
}

pub fn run() {
    let mut builder = tauri::Builder::default();
    // a self-test must run even while the player has Breacher open
    if std::env::var_os("BREACHER_SELFTEST").is_none() {
        builder =
            builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_main(app)));
    }
    builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(update::Pending::default())
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
        .on_window_event(|window, event| {
            if window.label() != "main" {
                return;
            }
            match event {
                WindowEvent::CloseRequested { api, .. } => {
                    let shell = window.state::<Shell>();
                    let stay = shell.settings.lock().unwrap().tray;
                    // with a tray icon to come back from, closing only puts the window away
                    if stay && shell.tray.load(Ordering::SeqCst) {
                        api.prevent_close();
                        let _ = window.hide();
                    }
                }
                // the hidden overlay window would otherwise keep the process, and its hotkey, alive
                WindowEvent::Destroyed => window.app_handle().exit(0),
                _ => {}
            }
        })
        .setup(|app| {
            // BREACHER_CONFIG=<file> keeps tests away from the real settings
            let config_path = std::env::var_os("BREACHER_CONFIG")
                .map(PathBuf::from)
                .unwrap_or_else(|| app.path().app_config_dir().map(|d| d.join("settings.json")).unwrap_or_else(|_| PathBuf::from("breacher-settings.json")));
            let settings: Settings = std::fs::read_to_string(&config_path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
            let watch_dir = settings.watch_dir.clone();
            let wanted = settings.hotkey.as_deref().and_then(|a| hotkey::parse(a).ok());
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
                hotkey: Mutex::new(hotkey::Binding::default()),
                version: app.package_info().version.to_string(),
                tray: AtomicBool::new(false),
            });
            let tray = add_tray(app.handle());
            app.state::<Shell>().tray.store(tray.is_ok(), Ordering::SeqCst);
            if let Err(e) = tray {
                log(&app.state::<Shell>(), &format!("no tray icon, so closing the window ends the program: {e}"));
            }
            let mut refusals = Vec::new();
            let binding = hotkey::bind(wanted, |s| match app.global_shortcut().register(s) {
                Ok(()) => true,
                Err(e) => {
                    refusals.push(format!("{} ({e})", hotkey::label(&s)));
                    false
                }
            });
            {
                let shell = app.state::<Shell>();
                *shell.hotkey.lock().unwrap() = binding;
                log(
                    &shell,
                    &format!(
                        "started Breacher {} on {} {}; hotkey {}; refused {:?}; capture permission {:?}",
                        app.package_info().version,
                        std::env::consts::OS,
                        std::env::consts::ARCH,
                        binding.active.as_ref().map(hotkey::label).unwrap_or_else(|| "none".into()),
                        refusals,
                        breach_capture::permission()
                    ),
                );
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
                            let (response, meta) = answer(&handle, rgba.into_raw(), w, h, "selftest".into(), started, None);
                            let shell = handle.state::<Shell>();
                            let summary = match &response {
                                Response::Ok { reading, plan } => format!("ok n={} buffer={} picks={} uploaded={:?}", reading.n, reading.buffer, plan.route.steps.len(), plan.route.uploaded),
                                Response::Rejected { stage, .. } => format!("rejected at {stage:?}"),
                            };
                            let update = match tauri::async_runtime::block_on(update::find(&handle)) {
                                Ok(Some(found)) => format!("{} available", found.version),
                                Ok(None) => "up to date".to_string(),
                                Err(e) => format!("nothing found ({e})"),
                            };
                            let own = breach_capture::version::of_process(std::process::id())
                                .map_or("none".to_string(), |v| v.label());
                            format!(
                                "selftest: {summary} | frame {}x{} | {:.1} ms | hotkey: {} | windows: main={} overlay={} | capture permission: {:?} | frame url: {} | update: {update} | version the system reports for this program: {own}",
                                meta.width,
                                meta.height,
                                started.elapsed().as_secs_f32() * 1e3,
                                selftest_hotkey(&handle),
                                handle.get_webview_window("main").is_some(),
                                handle.get_webview_window("overlay").is_some(),
                                breach_capture::permission(),
                                last_frame_url(shell.clone()).map(|u| u.len()).unwrap_or(0)
                            )
                        }
                    };
                    println!("{line}");
                    // BREACHER_SELFTEST_INSTALL: a development build goes on to install the update its test manifest offers
                    #[cfg(debug_assertions)]
                    if std::env::var_os("BREACHER_SELFTEST_INSTALL").is_some()
                        && tauri::async_runtime::block_on(update::check_update(handle.clone())).is_some()
                    {
                        let outcome = tauri::async_runtime::block_on(update::install_update(handle.clone()));
                        println!("selftest: the update did not install: {outcome:?}");
                    }
                    // closing the window must end the process, or with the tray only put the window away
                    let Some(main) = handle.get_webview_window("main") else {
                        return handle.exit(0);
                    };
                    let tray = status_of(&handle.state::<Shell>()).tray;
                    println!("selftest: closing the window; kept in the tray: {tray:?}");
                    let _ = main.close();
                    if tray == Some(true) {
                        std::thread::sleep(Duration::from_millis(600));
                        println!("selftest: still running, window visible: {:?}", main.is_visible());
                        handle.exit(0);
                    }
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![status, capture, allow_capture, set_priority, set_matcher, set_overlay, dismiss_overlay, set_updates, set_tray, watch_folder, pause_hotkey, set_hotkey, last_frame_url,
            update::check_update,
            update::install_update,
            update::open_downloads])
        .run(tauri::generate_context!())
        .expect("error while running Breacher");
}
