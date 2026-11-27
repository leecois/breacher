//! The capture hotkey: which combination is bound, what to fall back to when it is taken, and how it reads.

use std::str::FromStr;
use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut};

/// Tried in this order when the player has not chosen a combination, or theirs is taken.
pub const DEFAULTS: [&str; 3] = ["CmdOrCtrl+Shift+B", "CmdOrCtrl+Alt+B", "Alt+Shift+B"];

#[derive(Clone, Copy, Default)]
pub struct Binding {
    /// The combination the system accepted.
    pub active: Option<Shortcut>,
    /// The combination that was wanted but belongs to another program.
    pub refused: Option<Shortcut>,
}

pub fn parse(accelerator: &str) -> Result<Shortcut, String> {
    Shortcut::from_str(accelerator).map_err(|_| "That key cannot be used as a hotkey.".to_string())
}

/// A combination the player may choose: it must not swallow a key that is typed on its own.
pub fn usable(shortcut: &Shortcut) -> Result<(), String> {
    let held = shortcut.mods.intersects(Modifiers::CONTROL | Modifiers::ALT | Modifiers::SUPER);
    let code = shortcut.key.to_string();
    let function = code
        .strip_prefix('F')
        .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
    if held || function {
        Ok(())
    } else {
        Err("Hold Ctrl or Alt with that key, or use a function key.".to_string())
    }
}

fn name(key: Code) -> String {
    let code = key.to_string();
    if let Some(letter) = code.strip_prefix("Key").or_else(|| code.strip_prefix("Digit")) {
        letter.to_string()
    } else if let Some(pad) = code.strip_prefix("Numpad") {
        format!("Num {pad}")
    } else {
        code
    }
}

/// The combination as the player would read it.
pub fn label(shortcut: &Shortcut) -> String {
    let mac = cfg!(target_os = "macos");
    let held = [
        (Modifiers::CONTROL, "Ctrl", "⌃"),
        (Modifiers::ALT, "Alt", "⌥"),
        (Modifiers::SHIFT, "Shift", "⇧"),
        (Modifiers::SUPER, "Win", "⌘"),
    ];
    let mut parts: Vec<String> = held
        .iter()
        .filter(|(m, ..)| shortcut.mods.contains(*m))
        .map(|(_, word, sign)| if mac { sign.to_string() } else { word.to_string() })
        .collect();
    parts.push(name(shortcut.key));
    parts.join(" ")
}

/// Binds the first combination the system accepts: the wanted one, then the defaults.
pub fn bind(wanted: Option<Shortcut>, mut register: impl FnMut(Shortcut) -> bool) -> Binding {
    let mut candidates: Vec<Shortcut> = Vec::new();
    for c in wanted.into_iter().chain(DEFAULTS.iter().filter_map(|a| Shortcut::from_str(a).ok())) {
        if !candidates.contains(&c) {
            candidates.push(c);
        }
    }
    let first = candidates.first().copied();
    let active = candidates.into_iter().find(|c| register(*c));
    Binding { active, refused: first.filter(|f| active != Some(*f)) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(accelerator: &str) -> Shortcut {
        parse(accelerator).unwrap()
    }

    #[test]
    fn the_wanted_combination_is_bound_when_free() {
        let b = bind(Some(key("Ctrl+Alt+F9")), |_| true);
        assert_eq!(b.active, Some(key("Ctrl+Alt+F9")));
        assert_eq!(b.refused, None);
    }

    #[test]
    fn a_taken_combination_falls_back_to_the_next_default() {
        let taken = key(DEFAULTS[0]);
        let mut tried = Vec::new();
        let b = bind(None, |s| {
            tried.push(s);
            s != taken
        });
        assert_eq!(b.active, Some(key(DEFAULTS[1])));
        assert_eq!(b.refused, Some(taken));
        assert_eq!(tried, vec![taken, key(DEFAULTS[1])]);
    }

    #[test]
    fn nothing_is_bound_when_every_combination_is_taken() {
        let mut tried = 0;
        let b = bind(Some(key(DEFAULTS[1])), |_| {
            tried += 1;
            false
        });
        assert!(b.active.is_none());
        assert_eq!(b.refused, Some(key(DEFAULTS[1])));
        assert_eq!(tried, DEFAULTS.len());
    }

    #[test]
    fn combinations_read_the_way_they_are_pressed() {
        let b = label(&key("Control+Shift+KeyB"));
        let n = label(&key("Alt+Numpad7"));
        if cfg!(target_os = "macos") {
            assert_eq!((b.as_str(), n.as_str()), ("⌃ ⇧ B", "⌥ Num 7"));
        } else {
            assert_eq!((b.as_str(), n.as_str()), ("Ctrl Shift B", "Alt Num 7"));
        }
    }

    #[test]
    fn a_bare_typing_key_is_not_a_hotkey() {
        assert!(usable(&key("Shift+KeyB")).is_err());
        assert!(usable(&key("KeyF")).is_err());
        assert!(usable(&key("F9")).is_ok());
        assert!(usable(&key("Alt+KeyB")).is_ok());
        assert!(parse("Ctrl+Nonsense").is_err());
    }
}
