//! Following a breach after its route is drawn: decides, look by look, when the route has no screen left to sit on.

use std::time::Duration;

/// Time between two looks at the game.
pub const INTERVAL: Duration = Duration::from_millis(700);
/// Looks in a row without a breach screen before the route comes down: one odd frame must not do it.
const GONE: u32 = 3;
/// Looks in a row that could not be taken before the game is given up on.
const FAILED: u32 = 6;
/// Looks after which the route comes down whatever is on screen, about five minutes.
const LOOKS: u32 = 430;

/// Looks kept for a report: the most recent ones, which is where a breach is played out.
const KEEP: usize = 24;

/// A look kept for a report.
#[derive(Clone)]
pub struct Kept {
    /// Tenths of a second since the capture.
    pub at: u32,
    /// Whether a breach screen was seen in it.
    pub breach: bool,
    pub jpeg: Vec<u8>,
}

/// Keeps a look for a report, letting go of the oldest once there are too many.
pub fn keep(kept: &mut Vec<Kept>, look: Kept) {
    kept.push(look);
    if kept.len() > KEEP {
        kept.remove(0);
    }
}

/// What one look at the game showed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    Breach,
    Other,
    Minimised,
    Failed,
}

#[derive(Default)]
struct Watch {
    other: u32,
    failed: u32,
}

impl Watch {
    fn see(&mut self, look: Look) -> Option<&'static str> {
        match look {
            Look::Breach => {
                *self = Watch::default();
                None
            }
            Look::Minimised => Some("the game was minimised"),
            Look::Other => {
                self.other += 1;
                (self.other >= GONE).then_some("the breach screen closed")
            }
            Look::Failed => {
                self.failed += 1;
                (self.failed >= FAILED).then_some("the game could no longer be captured")
            }
        }
    }
}

/// Looks until the route should come down and says why. `look` returns `None` once another capture owns the overlay.
pub fn until_gone(mut look: impl FnMut() -> Option<Look>) -> Option<&'static str> {
    let mut watch = Watch::default();
    for _ in 0..LOOKS {
        if let Some(why) = watch.see(look()?) {
            return Some(why);
        }
    }
    Some("it had been up for five minutes")
}

#[cfg(test)]
mod tests {
    use super::*;
    use Look::*;

    fn play(looks: &[Look]) -> (Option<&'static str>, usize) {
        let mut taken = 0;
        let why = until_gone(|| {
            taken += 1;
            Some(looks.get(taken - 1).copied().unwrap_or(Breach))
        });
        (why, taken)
    }

    #[test]
    fn the_route_stays_while_the_breach_is_on_screen() {
        let (why, taken) = play(&[Breach; 50]);
        assert_eq!(why, Some("it had been up for five minutes"));
        assert_eq!(taken, LOOKS as usize);
    }

    #[test]
    fn it_comes_down_once_the_screen_has_been_gone_for_three_looks() {
        let (why, taken) = play(&[Breach, Breach, Other, Other, Other]);
        assert_eq!((why, taken), (Some("the breach screen closed"), 5));
    }

    #[test]
    fn one_odd_frame_does_not_take_it_down() {
        let (why, taken) =
            play(&[Other, Other, Breach, Other, Failed, Breach, Other, Other, Other]);
        assert_eq!((why, taken), (Some("the breach screen closed"), 9));
    }

    #[test]
    fn a_minimised_game_takes_it_down_at_once() {
        let (why, taken) = play(&[Breach, Minimised]);
        assert_eq!((why, taken), (Some("the game was minimised"), 2));
    }

    #[test]
    fn a_game_that_cannot_be_captured_is_given_up_on() {
        let (why, taken) = play(&[Failed; 6]);
        assert_eq!((why, taken), (Some("the game could no longer be captured"), 6));
    }

    #[test]
    fn only_the_latest_looks_are_kept() {
        let mut kept = Vec::new();
        for at in 0..40u32 {
            keep(&mut kept, Kept { at, breach: at < 30, jpeg: vec![at as u8] });
        }
        assert_eq!(kept.len(), KEEP);
        assert_eq!((kept[0].at, kept[KEEP - 1].at), (16, 39));
        assert!(kept[0].breach && !kept[KEEP - 1].breach);
    }

    #[test]
    fn a_newer_capture_ends_the_watch_without_a_reason() {
        let mut taken = 0;
        let why = until_gone(|| {
            taken += 1;
            (taken < 3).then_some(Breach)
        });
        assert_eq!((why, taken), (None, 3));
    }
}
