//! One-shot screen capture: finds the game window, checks permission without prompting, reports where the frame sits on screen.

pub mod version;

use serde::{Deserialize, Serialize};
use std::time::Instant;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum Target {
    /// The game window if one is found, otherwise the primary display.
    Auto,
    /// A window whose title or application name contains this text (case-insensitive).
    Window(String),
    /// A display by index (0 = primary first).
    Display(usize),
}

/// Text looked for in window titles and application names when the target is [`Target::Auto`].
pub const GAME_HINTS: [&str; 2] = ["cyberpunk 2077", "cyberpunk"];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    Granted,
    /// The operating system refuses screen capture for this application until the user allows it.
    Denied,
}

#[derive(Debug)]
pub enum CaptureError {
    Permission,
    /// The requested window or display does not exist.
    NoTarget(String),
    /// The game is running but its window is minimised, so there is nothing to see.
    Minimised,
    Backend(String),
}

impl std::fmt::Display for CaptureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CaptureError::Permission => {
                write!(f, "screen capture is not allowed for this application")
            }
            CaptureError::NoTarget(t) => write!(f, "nothing to capture: {t}"),
            CaptureError::Minimised => write!(f, "the game window is minimised"),
            CaptureError::Backend(e) => write!(f, "capture failed: {e}"),
        }
    }
}

impl std::error::Error for CaptureError {}

/// A captured frame and where it sits on the desktop.
pub struct Captured {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    /// What was captured, for display.
    pub source: String,
    /// Top-left corner and size on the desktop, in the units `logical_units` names.
    pub x: i32,
    pub y: i32,
    pub screen_width: u32,
    pub screen_height: u32,
    /// True when the bounds are in logical points, false for physical pixels.
    pub logical_units: bool,
    /// True when no game window was found and the primary display was captured instead.
    pub fallback: bool,
    /// The window this frame came from, for looking at it again; `None` for a display.
    pub again: Option<Source>,
    /// The version of the program that was looked for, when the window found is its own.
    pub program: Option<version::Version>,
    pub capture_ms: f32,
}

/// A window that was captured once and can be looked at again without searching for it.
#[derive(Clone)]
pub struct Source(xcap::Window);

impl Source {
    pub fn grab(&self) -> Result<Captured, CaptureError> {
        if self.0.is_minimized().unwrap_or(true) {
            return Err(CaptureError::Minimised);
        }
        grab_window(&self.0)
    }
}

/// Whether capturing again and again goes unnoticed: some systems flash a border round the window on every capture.
pub fn recapture_is_quiet() -> bool {
    #[cfg(windows)]
    {
        windows_version::OsVersion::current() >= windows_version::OsVersion::new(10, 0, 0, 20348)
    }
    #[cfg(not(windows))]
    {
        true
    }
}

#[cfg(target_os = "macos")]
mod tcc {
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        pub fn CGPreflightScreenCaptureAccess() -> bool;
        pub fn CGRequestScreenCaptureAccess() -> bool;
    }
}

/// Whether capture is currently allowed. Never shows a prompt.
pub fn permission() -> Permission {
    #[cfg(target_os = "macos")]
    {
        if unsafe { tcc::CGPreflightScreenCaptureAccess() } {
            Permission::Granted
        } else {
            Permission::Denied
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        Permission::Granted
    }
}

/// Asks the operating system for capture permission, which may show a system prompt; call it only from an explicit user action.
pub fn request_permission() -> Permission {
    #[cfg(target_os = "macos")]
    {
        if unsafe { tcc::CGRequestScreenCaptureAccess() } {
            Permission::Granted
        } else {
            Permission::Denied
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        Permission::Granted
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowInfo {
    pub title: String,
    pub app: String,
}

/// The windows a player could mean: shown, titled, large enough to hold a game, and not this program's own.
pub fn windows() -> Vec<WindowInfo> {
    let own = std::process::id();
    let mut found: Vec<WindowInfo> = xcap::Window::all()
        .unwrap_or_default()
        .into_iter()
        .filter(|w| !w.is_minimized().unwrap_or(true) && w.pid().ok() != Some(own))
        .filter(|w| w.width().unwrap_or(0) >= 320 && w.height().unwrap_or(0) >= 200)
        .filter_map(|w| {
            let title = w.title().unwrap_or_default().trim().to_string();
            let app = w.app_name().unwrap_or_default().trim().to_string();
            (!title.is_empty()).then_some(WindowInfo { title, app })
        })
        .collect();
    found.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
    found.dedup();
    found
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisplayInfo {
    pub width: u32,
    pub height: u32,
    pub primary: bool,
}

/// The displays in the order [`Target::Display`] counts them: the primary one first.
pub fn displays() -> Vec<DisplayInfo> {
    let mut all = xcap::Monitor::all().unwrap_or_default();
    all.sort_by_key(|m| !m.is_primary().unwrap_or(false));
    all.iter()
        .map(|m| DisplayInfo {
            width: m.width().unwrap_or(0),
            height: m.height().unwrap_or(0),
            primary: m.is_primary().unwrap_or(false),
        })
        .collect()
}

/// How well a window fits a name, lower is better: the application itself, then a title that starts with the name, then one that mentions it.
fn rank(title: &str, app: &str, needles: &[&str]) -> Option<u8> {
    let (title, app) = (title.to_lowercase(), app.to_lowercase());
    let needles: Vec<String> = needles.iter().map(|n| n.to_lowercase()).collect();
    if needles.iter().any(|n| app.contains(n)) {
        Some(0)
    } else if needles.iter().any(|n| title.starts_with(n)) {
        Some(1)
    } else if needles.iter().any(|n| title.contains(n)) {
        Some(2)
    } else {
        None
    }
}

enum Search {
    /// The window, and whether it belongs to the program itself and does not merely mention it.
    Found(xcap::Window, bool),
    /// The application is running, but its only windows are minimised.
    Minimised,
    Missing,
}

fn find_window(needles: &[&str]) -> Search {
    let Ok(all) = xcap::Window::all() else { return Search::Missing };
    let mut shown = Vec::new();
    let mut minimised = false;
    for w in all {
        let (title, app) = (w.title().unwrap_or_default(), w.app_name().unwrap_or_default());
        let Some(rank) = rank(&title, &app, needles) else { continue };
        if w.is_minimized().unwrap_or(true) {
            minimised |= rank == 0;
            continue;
        }
        let (width, height) = (w.width().unwrap_or(0), w.height().unwrap_or(0));
        if width >= 320 && height >= 200 {
            shown.push((rank, std::cmp::Reverse(width as u64 * height as u64), w));
        }
    }
    // among equals the largest wins: launchers and helper windows share the name
    match shown.into_iter().min_by_key(|(rank, area, _)| (*rank, *area)) {
        Some((rank, _, w)) => Search::Found(w, rank == 0),
        None if minimised => Search::Minimised,
        None => Search::Missing,
    }
}

fn backend(e: xcap::XCapError) -> CaptureError {
    CaptureError::Backend(e.to_string())
}

fn grab_window(w: &xcap::Window) -> Result<Captured, CaptureError> {
    let t = Instant::now();
    let img = w.capture_image().map_err(backend)?;
    let title = w.title().unwrap_or_default();
    Ok(Captured {
        width: img.width(),
        height: img.height(),
        rgba: img.into_raw(),
        source: if title.is_empty() { "Window".into() } else { format!("{title} window") },
        x: w.x().map_err(backend)?,
        y: w.y().map_err(backend)?,
        screen_width: w.width().map_err(backend)?,
        screen_height: w.height().map_err(backend)?,
        logical_units: cfg!(target_os = "macos"),
        fallback: false,
        again: Some(Source(w.clone())),
        program: None,
        capture_ms: t.elapsed().as_secs_f32() * 1e3,
    })
}

fn grab_found(w: &xcap::Window, own: bool) -> Result<Captured, CaptureError> {
    let mut captured = grab_window(w)?;
    if own {
        captured.program = w.pid().ok().and_then(version::of_process);
    }
    Ok(captured)
}

fn grab_display(index: usize) -> Result<Captured, CaptureError> {
    let mut all = xcap::Monitor::all().map_err(backend)?;
    all.sort_by_key(|m| !m.is_primary().unwrap_or(false));
    let m = all
        .get(index)
        .ok_or_else(|| CaptureError::NoTarget(format!("display {} of {}", index + 1, all.len())))?;
    let t = Instant::now();
    let img = m.capture_image().map_err(backend)?;
    Ok(Captured {
        width: img.width(),
        height: img.height(),
        rgba: img.into_raw(),
        source: format!("Display {}", index + 1),
        x: m.x().map_err(backend)?,
        y: m.y().map_err(backend)?,
        screen_width: m.width().map_err(backend)?,
        screen_height: m.height().map_err(backend)?,
        logical_units: cfg!(target_os = "macos"),
        fallback: false,
        again: None,
        program: None,
        capture_ms: t.elapsed().as_secs_f32() * 1e3,
    })
}

/// Capture one frame.
pub fn capture(target: &Target) -> Result<Captured, CaptureError> {
    if permission() == Permission::Denied {
        return Err(CaptureError::Permission);
    }
    match target {
        Target::Auto => match find_window(&GAME_HINTS) {
            Search::Found(w, own) => grab_found(&w, own),
            Search::Minimised => Err(CaptureError::Minimised),
            Search::Missing => grab_display(0).map(|c| Captured { fallback: true, ..c }),
        },
        Target::Window(name) => match find_window(&[name.as_str()]) {
            Search::Found(w, own) => grab_found(&w, own),
            Search::Minimised => Err(CaptureError::Minimised),
            Search::Missing => {
                Err(CaptureError::NoTarget(format!("no open window matches \"{name}\"")))
            }
        },
        Target::Display(i) => grab_display(*i),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_check_never_panics_and_capture_respects_it() {
        // without the capture grant this must be a clean, typed refusal
        match permission() {
            Permission::Denied => {
                assert!(matches!(capture(&Target::Auto), Err(CaptureError::Permission)))
            }
            Permission::Granted => {}
        }
    }

    #[test]
    fn missing_targets_are_reported() {
        if permission() == Permission::Granted {
            assert!(matches!(
                capture(&Target::Window("no-such-window-7f3a".into())),
                Err(CaptureError::NoTarget(_))
            ));
            assert!(matches!(capture(&Target::Display(99)), Err(CaptureError::NoTarget(_))));
        }
    }

    #[test]
    fn the_application_outranks_windows_that_only_mention_it() {
        let game = rank("Cyberpunk 2077 (C) 2020", "Cyberpunk 2077", &GAME_HINTS);
        let unnamed = rank("Cyberpunk 2077 (C) 2020", "", &GAME_HINTS);
        let browser = rank("Guide to Cyberpunk 2077 - Browser", "Browser", &GAME_HINTS);
        assert_eq!((game, unnamed, browser), (Some(0), Some(1), Some(2)));
        assert_eq!(rank("Breacher", "Breacher", &GAME_HINTS), None);
        assert_eq!(rank("x", "CYBERPUNK2077.EXE", &GAME_HINTS), Some(0));
    }

    #[test]
    fn displays_are_listed_primary_first_and_windows_never_panic() {
        let all = displays();
        assert!(all.iter().skip(1).all(|d| !d.primary), "{all:?}");
        let _ = windows();
    }

    #[test]
    fn target_round_trips_as_json() {
        for t in [Target::Auto, Target::Window("Cyberpunk".into()), Target::Display(1)] {
            let s = serde_json::to_string(&t).unwrap();
            assert_eq!(serde_json::from_str::<Target>(&s).unwrap(), t);
        }
    }
}
