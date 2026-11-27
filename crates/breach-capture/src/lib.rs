//! One-shot screen capture: finds the game window, checks permission without prompting, reports where the frame sits on screen.

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
    Backend(String),
}

impl std::fmt::Display for CaptureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CaptureError::Permission => {
                write!(f, "screen capture is not allowed for this application")
            }
            CaptureError::NoTarget(t) => write!(f, "nothing to capture: {t}"),
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
    pub capture_ms: f32,
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

/// Visible, non-minimised windows with a title.
pub fn windows() -> Vec<WindowInfo> {
    xcap::Window::all()
        .unwrap_or_default()
        .into_iter()
        .filter(|w| !w.is_minimized().unwrap_or(true))
        .filter_map(|w| {
            let title = w.title().unwrap_or_default();
            let app = w.app_name().unwrap_or_default();
            (!title.trim().is_empty()).then_some(WindowInfo { title, app })
        })
        .collect()
}

fn matches(w: &xcap::Window, needle: &str) -> bool {
    let needle = needle.to_lowercase();
    let title = w.title().unwrap_or_default().to_lowercase();
    let app = w.app_name().unwrap_or_default().to_lowercase();
    title.contains(&needle) || app.contains(&needle)
}

fn find_window(needles: &[&str]) -> Option<xcap::Window> {
    let all = xcap::Window::all().ok()?;
    let visible: Vec<xcap::Window> = all
        .into_iter()
        .filter(|w| {
            !w.is_minimized().unwrap_or(true)
                && w.width().unwrap_or(0) >= 320
                && w.height().unwrap_or(0) >= 200
        })
        .collect();
    for needle in needles {
        // the largest match: launchers and helper windows share the name
        if let Some(w) = visible
            .iter()
            .filter(|w| matches(w, needle))
            .max_by_key(|w| w.width().unwrap_or(0) as u64 * w.height().unwrap_or(0) as u64)
        {
            return Some(w.clone());
        }
    }
    None
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
        capture_ms: t.elapsed().as_secs_f32() * 1e3,
    })
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
            Some(w) => grab_window(&w),
            None => grab_display(0),
        },
        Target::Window(name) => match find_window(&[name.as_str()]) {
            Some(w) => grab_window(&w),
            None => Err(CaptureError::NoTarget(format!("no open window matches \"{name}\""))),
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
    fn target_round_trips_as_json() {
        for t in [Target::Auto, Target::Window("Cyberpunk".into()), Target::Display(1)] {
            let s = serde_json::to_string(&t).unwrap();
            assert_eq!(serde_json::from_str::<Target>(&s).unwrap(), t);
        }
    }
}
