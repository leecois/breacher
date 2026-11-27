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

/// Seconds between two looks kept for a report.
const KEEP_EVERY: u32 = 2;
/// Looks kept for a report: the most recent ones, which is where a breach is played out.
const KEEP: usize = 8;

/// A look kept for a report: the seconds since the capture and the frame, encoded.
pub type Kept = (u32, Vec<u8>);

/// Keeps a look for a report if the last one kept is old enough; `encode` runs only then.
pub fn keep(kept: &mut Vec<Kept>, seconds: u32, encode: impl FnOnce() -> Option<Vec<u8>>) {
    if kept.last().is_some_and(|(at, _)| seconds < at + KEEP_EVERY) {
        return;
    }
    if let Some(frame) = encode() {
        kept.push((seconds, frame));
        if kept.len() > KEEP {
            kept.remove(0);
        }
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
    fn looks_are_kept_two_seconds_apart_and_only_the_latest_eight() {
        let mut kept = Vec::new();
        let mut encoded = 0;
        for tenth in 0..400u32 {
            keep(&mut kept, tenth / 10, || {
                encoded += 1;
                Some(vec![tenth as u8])
            });
        }
        let seconds: Vec<u32> = kept.iter().map(|(at, _)| *at).collect();
        assert_eq!(seconds, vec![24, 26, 28, 30, 32, 34, 36, 38]);
        assert_eq!(encoded, 20, "a frame is encoded only when it is kept");
        keep(&mut kept, 40, || None);
        assert_eq!(kept.len(), 8, "a frame that does not encode is not kept");
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
