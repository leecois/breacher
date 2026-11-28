//! Desktop shell: a global hotkey that captures and reads the game, an overlay window over it, and a watched screenshot folder.

mod follow;
mod hotkey;
mod report;
mod update;

use base64::Engine;
use breach_capture::{CaptureError, Permission, Target};
use breach_core::api::{self, Response};
use breach_core::glyph::Templates;
use breach_core::image::{Frame, PixelFormat};
use breach_core::plan::{rule_check, PlanOptions, Priority, Route, RuleCheck};
use breach_core::progress;
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
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};
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
    /// The version that ran last: the first start of a new one shows its window even when asked to start hidden.
    last_version: Option<String>,
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
            last_version: None,
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
    /// What was made of the last frame, for a report.
    last_answer: Mutex<Option<Answer>>,
    /// The game's version as last seen by a capture.
    game: Mutex<Option<String>>,
    /// A few looks at the last breach that was followed, for a report.
    looks: Mutex<Vec<follow::Kept>>,
    /// The breach whose picks are being followed, with the answer its route came from.
    followed: Mutex<Option<(follow::Followed, Response, FrameMeta)>>,
    /// Whether the program starts when the player signs in; `None` when the system will not say.
    autostart: Mutex<Option<bool>>,
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
    /// The player asked for the next breach to be spent on checking the wrong-pick rule.
    checking: AtomicBool,
    /// A route is up and its breach is being followed: the watch for new breaches rests meanwhile.
    following: AtomicBool,
    /// Held while a frame is taken and answered, so that a hotkey capture and the watch do not cross.
    capturing: Mutex<()>,
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
    /// The rule check to play on this screen in place of the route, when one was asked for.
    check: Option<RuleCheck>,
}

#[derive(Clone, Serialize)]
struct OverlayPayload {
    response: Response,
    frame: FrameMeta,
    route: Option<Route>,
    /// The route is a rule check, to be played exactly as drawn.
    check: bool,
}

/// News for the main window of a pick seen in the game.
#[derive(Clone, Serialize)]
struct Progress {
    /// The route as it now stands, with the picks made counted off.
    route: Route,
    /// The pick left the route that was drawn, and this one replaces it.
    replanned: bool,
    check: bool,
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
    /// What a capture looks for: the game on its own, a window the player named, or a display.
    target: Target,
    /// Whether closing the window keeps the program in the tray; `None` where there is no tray.
    tray: Option<bool>,
    autostart: Option<bool>,
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
        target: settings.target,
        tray: shell.tray.load(Ordering::SeqCst).then_some(settings.tray),
        autostart: *shell.autostart.lock().unwrap(),
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
) -> Answer {
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
    let check = match &response {
        Response::Ok { reading, .. } if shell.checking.load(Ordering::SeqCst) => {
            rule_check(reading, &PlanOptions { priority, matcher, ..PlanOptions::default() })
        }
        _ => None,
    };
    let told = Answer { response, frame: meta, elapsed_ms, check };
    *shell.last_answer.lock().unwrap() = Some(told.clone());
    let _ = app.emit_to("main", "answer", told.clone());
    told
}

/// How long a notice about an unread capture stays over the game.
const NOTICE: Duration = Duration::from_secs(6);

fn clear_overlay(app: &AppHandle) {
    let _ = app.emit_to("overlay", "clear", ());
}

/// Puts the overlay back above the game: a game that keeps itself on top covers it each time it comes to the front.
fn raise_overlay(app: &AppHandle) {
    #[cfg(windows)]
    if let Some(handle) = app.get_webview_window("overlay").and_then(|o| o.hwnd().ok()) {
        breach_capture::stack::raise(handle.0 as isize);
    }
    #[cfg(not(windows))]
    let _ = app;
}

/// Whether the overlay could be raised and whether the system counts it among the windows that stay on top, for the self-test.
fn overlay_stack(app: &AppHandle) -> String {
    #[cfg(windows)]
    if let Some(handle) = app.get_webview_window("overlay").and_then(|o| o.hwnd().ok()) {
        let window = handle.0 as isize;
        let raised = breach_capture::stack::raise(window);
        return format!(
            "raised {raised}, stays on top {:?}",
            breach_capture::stack::topmost(window)
        );
    }
    let _ = app;
    "not tried".to_string()
}

/// Starts a new answer: whatever route was up is no longer the one that is followed.
fn begin(app: &AppHandle) -> u64 {
    let shell = app.state::<Shell>();
    let generation = shell.generation.fetch_add(1, Ordering::SeqCst) + 1;
    *shell.followed.lock().unwrap() = None;
    shell.following.store(false, Ordering::SeqCst);
    clear_overlay(app);
    generation
}

/// Capture the game, read it, show the route in the main window and over the game.
fn run_capture(app: &AppHandle) {
    let shell = app.state::<Shell>();
    let _one = shell.capturing.lock().unwrap_or_else(|e| e.into_inner());
    let settings = shell.settings.lock().unwrap().clone();
    let generation = begin(app);
    // our own drawing must not be in the frame (it would be when a whole display is captured)
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
                CaptureError::NoTarget(_) => "What was chosen under Capture is not there. Choose another window or display in Breacher.".to_string(),
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
                    check: None,
                },
            );
            return;
        }
    };
    present(app, cap, started, generation, &settings, true);
}

/// Shows what a captured frame came to: the answer in the window, and the route over the game. A frame the player asked for also says over the game why it was not read.
fn present(
    app: &AppHandle,
    cap: breach_capture::Captured,
    started: Instant,
    generation: u64,
    settings: &Settings,
    asked: bool,
) {
    let shell = app.state::<Shell>();
    let (x, y, sw, sh, logical) =
        (cap.x, cap.y, cap.screen_width, cap.screen_height, cap.logical_units);
    if let Some(program) = cap.program {
        log(&shell, &format!("game version {}", program.label()));
        *shell.game.lock().unwrap() = Some(program.label());
        let _ = app.emit_to("main", "game-version", program.label());
    }
    let follow = settings.overlay_follow.unwrap_or_else(breach_capture::recapture_is_quiet);
    let again = cap.again.filter(|_| follow);
    log(
        &shell,
        &format!(
            "captured {} in {:.1} ms: {}x{} px, on screen at ({x}, {y}) size {sw}x{sh} ({}){}{}",
            cap.source,
            cap.capture_ms,
            cap.width,
            cap.height,
            if logical { "points" } else { "pixels" },
            if cap.fallback { "; no game window found" } else { "" },
            if cap.topmost == Some(true) { "; the window keeps itself on top" } else { "" }
        ),
    );
    let lost = cap.fallback.then(|| {
        format!(
            "The game window was not found, so the whole screen was captured. Open a breach in the game, {}. If the game is open, choose its window under Capture in Breacher.",
            next_step(&shell)
        )
    });
    let told = answer(app, cap.rgba, cap.width, cap.height, cap.source, started, lost);
    let (response, meta) = (told.response, told.frame);
    if !settings.overlay {
        return;
    }
    let Some(overlay) = app.get_webview_window("overlay") else { return };
    if matches!(response, Response::Rejected { .. }) && !asked {
        return;
    }
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
    raise_overlay(app);
    let ours = {
        let app = app.clone();
        move || app.state::<Shell>().generation.load(Ordering::SeqCst) == generation
    };
    let (reading, plan) = match &response {
        Response::Ok { reading, plan } => (reading, plan),
        Response::Rejected { hint, .. } => {
            // nothing else tells a player inside the game that the capture came to nothing
            let _ = app.emit_to("overlay", "notice", hint.clone());
            let app = app.clone();
            std::thread::spawn(move || {
                std::thread::sleep(NOTICE);
                if ours() {
                    clear_overlay(&app);
                }
            });
            return;
        }
    };
    // a rule check takes the place of the route: its picks are the ones to play
    let check = told.check.is_some();
    let route = told.check.map_or_else(|| plan.route.clone(), |c| c.route);
    if again.is_some() {
        let followed = follow::Followed::new(reading.clone(), route.clone(), check);
        *shell.followed.lock().unwrap() = Some((followed, response.clone(), meta.clone()));
    }
    let route = Some(route);
    let _ = app.emit_to(
        "overlay",
        "route",
        OverlayPayload { response: response.clone(), frame: meta, route, check },
    );
    if check {
        log(&shell, "the route drawn is a rule check");
    }
    log(
        &shell,
        match &again {
            Some(_) => "route drawn over the game, up until the breach screen closes",
            None => "route drawn over the game, up for a fixed time",
        },
    );
    let app = app.clone();
    let seconds = settings.overlay_seconds.max(3);
    std::thread::spawn(move || match again {
        Some(game) => {
            let shell = app.state::<Shell>();
            if ours() {
                shell.following.store(true, Ordering::SeqCst);
            }
            follow_breach(&app, &game, generation);
            if ours() {
                shell.following.store(false, Ordering::SeqCst);
            }
        }
        None => {
            let until = Instant::now() + Duration::from_secs(seconds);
            while Instant::now() < until && ours() {
                raise_overlay(&app);
                std::thread::sleep(Duration::from_millis(500));
            }
            if ours() {
                clear_overlay(&app);
            }
        }
    });
}

/// Tells the overlay and the main window how the followed route now stands.
fn tell_progress(app: &AppHandle, shell: &Shell, replanned: bool) {
    let guard = shell.followed.lock().unwrap();
    let Some((followed, response, meta)) = guard.as_ref() else { return };
    let (route, check) = (followed.route.clone(), followed.check);
    let drawn = OverlayPayload {
        response: response.clone(),
        frame: meta.clone(),
        route: Some(route.clone()),
        check,
    };
    drop(guard);
    let _ = app.emit_to("overlay", "route", drawn);
    let _ = app.emit_to("main", "progress", Progress { route, replanned, check });
}

/// Reads one look at the followed breach: takes in daemons the game has settled and picks it shows, redraws the route to match, and says whether anything was new.
fn follow_look(app: &AppHandle, frame: &Frame) -> (follow::Look, bool) {
    let shell = app.state::<Shell>();
    let (priority, matcher) = {
        let s = shell.settings.lock().unwrap();
        (s.priority, s.matcher)
    };
    let mut guard = shell.followed.lock().unwrap();
    let Some((followed, ..)) = guard.as_mut() else {
        let breach = api::breach_on_screen(frame);
        return (if breach { follow::Look::Breach } else { follow::Look::Other }, false);
    };
    let looked = followed.look(frame, priority, matcher);
    let picks = followed.played.len();
    drop(guard);
    for (daemon, fate) in &looked.settled {
        let what = if *fate == progress::Fate::Installed { "installed" } else { "failed" };
        log(&shell, &format!("daemon {daemon} {what}"));
    }
    if let Some(step) = looked.step {
        let replanned = step == follow::Step::Replanned;
        let what = if replanned { "off the route, the rest planned again" } else { "on the route" };
        log(&shell, &format!("pick {picks} seen: {what}"));
        tell_progress(app, &shell, replanned);
    }
    (looked.sight.into(), looked.step.is_some() || !looked.settled.is_empty())
}

/// Keeps the route up for as long as the breach screen is, and takes it down when the screen goes.
fn follow_breach(app: &AppHandle, game: &breach_capture::Source, generation: u64) {
    let shell = app.state::<Shell>();
    let started = Instant::now();
    let ours = || shell.generation.load(Ordering::SeqCst) == generation;
    shell.looks.lock().unwrap().clear();
    // one letter per look, for the log: breach, other, over, minimised, failed
    let mut trace = String::new();
    let mut kept_at = None;
    let why = follow::until_gone(|| {
        std::thread::sleep(follow::INTERVAL);
        if !ours() {
            return None;
        }
        raise_overlay(app);
        let look = match game.grab() {
            Ok(cap) => {
                let frame = Frame::new(
                    cap.width as usize,
                    cap.height as usize,
                    PixelFormat::Rgba8,
                    &cap.rgba,
                );
                let (look, news) = follow_look(app, &frame);
                // a report wants the looks where something happened, and one now and then in between
                let at = (started.elapsed().as_secs_f32() * 10.0) as u32;
                if news || kept_at.is_none_or(|last| at >= last + 6) {
                    if let Some(jpeg) = jpeg(cap.width, cap.height, &cap.rgba, 88) {
                        kept_at = Some(at);
                        let breach = look == follow::Look::Breach;
                        follow::keep(
                            &mut shell.looks.lock().unwrap(),
                            follow::Kept { at, breach, jpeg },
                        );
                    }
                }
                look
            }
            Err(CaptureError::Minimised) => follow::Look::Minimised,
            Err(_) => follow::Look::Failed,
        };
        trace.push(match look {
            follow::Look::Breach => 'B',
            follow::Look::Other => 'o',
            follow::Look::Over => 'v',
            follow::Look::Minimised => 'm',
            follow::Look::Failed => 'x',
        });
        Some(look)
    });
    let outcome = shell.followed.lock().unwrap().as_ref().and_then(|(followed, ..)| {
        let promised: Vec<usize> = followed.route.uploaded.iter().map(|d| d + 1).collect();
        followed.outcome().map(|(installed, failed)| {
            format!("result: installed {installed:?}, failed {failed:?}; the route promised {promised:?}")
        })
    });
    if let Some(outcome) = outcome.filter(|_| ours()) {
        log(&shell, &outcome);
    }
    if let Some(why) = why.filter(|_| ours()) {
        *shell.followed.lock().unwrap() = None;
        clear_overlay(app);
        log(
            &shell,
            &format!(
                "route taken down after {:.0} s: {why}; looks {trace}",
                started.elapsed().as_secs_f32()
            ),
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

/// Spends the next breach, and every one after it until told otherwise, on checking the wrong-pick rule.
#[tauri::command]
fn set_checking(shell: State<Shell>, on: bool) {
    shell.checking.store(on, Ordering::SeqCst);
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
        dismiss_overlay(app);
    }
    status_of(&shell)
}

/// Wipes the route from the game and ends any watch that would have done it later.
#[tauri::command]
fn dismiss_overlay(app: AppHandle) {
    begin(&app);
}

/// The window says which route it shows and under which choices, so that picks are followed against that one.
#[tauri::command]
fn shown_route(
    app: AppHandle,
    shell: State<Shell>,
    route: Route,
    wanted: Option<Vec<bool>>,
    buffer: Option<usize>,
    check: bool,
) {
    let (priority, matcher) = {
        let s = shell.settings.lock().unwrap();
        (s.priority, s.matcher)
    };
    let settled = {
        let mut guard = shell.followed.lock().unwrap();
        let Some((followed, ..)) = guard.as_mut() else { return };
        followed.adopt(route, wanted, buffer, check, priority, matcher)
    };
    if let Some(step) = settled {
        tell_progress(&app, &shell, step == follow::Step::Replanned);
    }
}

/// What a capture can be pointed at right now.
#[derive(Serialize)]
struct Targets {
    windows: Vec<breach_capture::WindowInfo>,
    displays: Vec<breach_capture::DisplayInfo>,
}

#[tauri::command(async)]
fn list_targets() -> Targets {
    Targets { windows: breach_capture::windows(), displays: breach_capture::displays() }
}

#[tauri::command]
fn set_target(shell: State<Shell>, target: Target) -> ShellStatus {
    log(&shell, &format!("capture target set to {target:?}"));
    shell.settings.lock().unwrap().target = target;
    save(&shell);
    status_of(&shell)
}

#[tauri::command]
fn set_tray(shell: State<Shell>, enabled: bool) -> ShellStatus {
    shell.settings.lock().unwrap().tray = enabled;
    save(&shell);
    status_of(&shell)
}

#[tauri::command]
fn set_autostart(
    app: AppHandle,
    shell: State<Shell>,
    enabled: bool,
) -> Result<ShellStatus, String> {
    let launch = app.autolaunch();
    let changed = if enabled { launch.enable() } else { launch.disable() };
    *shell.autostart.lock().unwrap() = launch.is_enabled().ok();
    changed.map_err(|e| e.to_string())?;
    Ok(status_of(&shell))
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

fn jpeg(width: u32, height: u32, rgba: &[u8], quality: u8) -> Option<Vec<u8>> {
    let rgb: Vec<u8> = rgba.chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]]).collect();
    let image = image::RgbImage::from_raw(width, height, rgb)?;
    let mut bytes = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, quality)
        .encode_image(&image)
        .ok()?;
    Some(bytes)
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

/// The argument a start at sign-in carries: go straight to the tray.
const HIDDEN: &str = "--hidden";

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
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, Some(vec![HIDDEN])))
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
                last_answer: Mutex::new(None),
                game: Mutex::new(None),
                looks: Mutex::new(Vec::new()),
                followed: Mutex::new(None),
                autostart: Mutex::new(None),
                generation: AtomicU64::new(0),
                watcher: Mutex::new(None),
                last_file: Mutex::new(None),
                hotkey: Mutex::new(hotkey::Binding::default()),
                version: app.package_info().version.to_string(),
                tray: AtomicBool::new(false),
                checking: AtomicBool::new(false),
                following: AtomicBool::new(false),
                capturing: Mutex::new(()),
            });
            let tray = add_tray(app.handle());
            app.state::<Shell>().tray.store(tray.is_ok(), Ordering::SeqCst);
            if let Err(e) = &tray {
                log(&app.state::<Shell>(), &format!("no tray icon, so closing the window ends the program: {e}"));
            }
            {
                let shell = app.state::<Shell>();
                *shell.autostart.lock().unwrap() = app.autolaunch().is_enabled().ok();
                let version = shell.version.clone();
                let (stay, known) = {
                    let s = shell.settings.lock().unwrap();
                    (s.tray, s.last_version.as_deref() == Some(version.as_str()))
                };
                if !known {
                    shell.settings.lock().unwrap().last_version = Some(version);
                    save(&shell);
                }
                // a start at sign-in goes straight to the tray, except the first start of a new version
                let asked = std::env::args().any(|a| a == HIDDEN);
                let test = std::env::var_os("BREACHER_SELFTEST").is_some();
                if !(asked && stay && known && tray.is_ok() && !test) {
                    show_main(app.handle());
                }
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
                            let told = answer(&handle, rgba.into_raw(), w, h, "selftest".into(), started, None);
                            let (response, meta) = (told.response, told.frame);
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
                            let check = match &response {
                                Response::Ok { reading, .. } => rule_check(reading, &PlanOptions::default())
                                    .map_or("none".to_string(), |c| format!("{} picks, {} outcomes", c.route.steps.len(), c.outcomes.len())),
                                Response::Rejected { .. } => "no reading".to_string(),
                            };
                            let report = match report::write(&handle, "selftest") {
                                Ok((_, files)) => files.join(" "),
                                Err(e) => format!("FAILED ({e})"),
                            };
                            // BREACHER_SELFTEST_AUTOSTART: also switch starting at sign-in on and off again
                            let autostart = if std::env::var_os("BREACHER_SELFTEST_AUTOSTART").is_some() {
                                let on = set_autostart(handle.clone(), handle.state(), true).map(|s| s.autostart);
                                let off = set_autostart(handle.clone(), handle.state(), false).map(|s| s.autostart);
                                format!("on {on:?}, off again {off:?}")
                            } else {
                                "not tried".to_string()
                            };
                            // what the list of capture targets finds here, and whether a display chosen from it can be captured
                            let targets = {
                                let found = list_targets();
                                let chosen = set_target(handle.state(), Target::Display(0)).target;
                                let grabbed = match breach_capture::capture(&chosen) {
                                    Ok(c) => format!("captured at {}x{}", c.width, c.height),
                                    Err(e) => format!("not captured ({e})"),
                                };
                                let back = set_target(handle.state(), Target::Auto).target;
                                format!("{} windows, {} displays, display 1 {grabbed}, back to {back:?}", found.windows.len(), found.displays.len())
                            };
                            let stack = overlay_stack(&handle);
                            format!(
                                "selftest: {summary} | frame {}x{} | {:.1} ms | hotkey: {} | windows: main={} overlay={} | capture permission: {:?} | frame url: {} | update: {update} | version the system reports for this program: {own} | report: {report} | start at sign-in: {autostart} | rule check: {check} | capture targets: {targets} | overlay: {stack}",
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
        .invoke_handler(tauri::generate_handler![status, capture, allow_capture, set_priority, set_matcher, set_checking, set_overlay, dismiss_overlay, shown_route, list_targets, set_target, set_updates, set_tray, set_autostart, report::save_report, watch_folder, pause_hotkey, set_hotkey, last_frame_url,
            update::check_update,
            update::install_update,
            update::open_downloads])
        .run(tauri::generate_context!())
        .expect("error while running Breacher");
}
