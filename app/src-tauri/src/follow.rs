//! Following a breach after its route is drawn: decides, look by look, when the route has no screen left to sit on.

use breach_core::plan::{plan_from, PlanOptions, Priority, Route};
use breach_core::progress::{advance, Look as Seen};
use breach_core::reader::Reading;
use breach_core::solver::Matcher;
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

/// The breach being followed: what was read, the route that is drawn and the picks seen so far.
pub struct Followed {
    pub reading: Reading,
    pub route: Route,
    /// The route is a rule check; leaving it turns it into an ordinary route.
    pub check: bool,
    /// The player's choices for this screen, as the window last reported them.
    pub wanted: Option<Vec<bool>>,
    pub buffer: Option<usize>,
    /// Cells picked so far, in order.
    pub played: Vec<usize>,
}

/// What a newly seen pick did to the route.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// The pick was the next one of the route.
    Onward,
    /// The pick left the route, and the rest was planned again from where the breach stands.
    Replanned,
}

impl Followed {
    fn on_route(&self) -> bool {
        let n = self.reading.n;
        self.played.len() <= self.route.steps.len()
            && self.played.iter().zip(&self.route.steps).all(|(&c, s)| c == s.row * n + s.col)
    }

    /// Takes in what a look shows; says what changed when it shows a pick not seen before.
    pub fn see(
        &mut self,
        seen: &Seen,
        priority: Priority,
        matcher: Option<Matcher>,
    ) -> Option<Step> {
        let n = self.reading.n;
        let played = advance(n, &self.played, seen)?;
        if played.len() == self.played.len() {
            return None;
        }
        self.played = played;
        self.settle(priority, matcher)
    }

    /// Brings the route in line with the picks made: counts them off, or plans the rest again.
    pub fn settle(&mut self, priority: Priority, matcher: Option<Matcher>) -> Option<Step> {
        let n = self.reading.n;
        if self.on_route() {
            self.route.done = self.played.len();
            return Some(Step::Onward);
        }
        let opt = PlanOptions {
            buffer: self.buffer,
            wanted: self.wanted.clone(),
            priority,
            matcher,
            ..PlanOptions::default()
        };
        let picks: Vec<(usize, usize)> = self.played.iter().map(|&c| (c / n, c % n)).collect();
        let plan = plan_from(&self.reading, &opt, &picks).ok()?;
        self.route = plan.route;
        self.check = false;
        Some(Step::Replanned)
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
    use breach_core::plan::plan;
    use breach_core::progress::Mark;
    use breach_core::reader::{Geometry, Quality, Timings};
    use Look::*;

    fn followed() -> Followed {
        // A B down the first column; after a wrong first pick the same daemon is still there to be had
        let reading = Reading {
            n: 3,
            matrix: vec![0, 2, 2, 1, 0, 2, 2, 1, 2],
            sequences: vec![vec![0, 1]],
            buffer: 4,
            geometry: Geometry {
                x0: 100.0,
                y0: 200.0,
                px: 64.0,
                py: 64.0,
                n: 3,
                seq_x0: 800.0,
                seq_pitch_x: 42.0,
                seq_rows: vec![],
                buffer_y: 40.0,
            },
            quality: Quality::default(),
            timings: Timings::default(),
        };
        let route = plan(&reading, &PlanOptions::default()).route;
        Followed { reading, route, check: false, wanted: None, buffer: None, played: Vec::new() }
    }

    fn seen(marks: &[(usize, Mark)], committed: usize) -> Seen {
        let mut cells = vec![Mark::Code; 9];
        for &(cell, mark) in marks {
            cells[cell] = mark;
        }
        Seen { cells, committed }
    }

    #[test]
    fn picks_on_the_route_are_counted_off() {
        let mut f = followed();
        assert_eq!(f.route.steps.len(), 2);
        assert_eq!(f.see(&seen(&[(0, Mark::Hover)], 0), Priority::Count, None), None);
        assert_eq!(f.see(&seen(&[(0, Mark::Flash)], 1), Priority::Count, None), Some(Step::Onward));
        assert_eq!((f.route.done, f.played.clone()), (1, vec![0]));
        // the same look again is no news
        assert_eq!(f.see(&seen(&[(0, Mark::Used)], 1), Priority::Count, None), None);
        let last = seen(&[(0, Mark::Used), (3, Mark::Flash)], 2);
        assert_eq!(f.see(&last, Priority::Count, None), Some(Step::Onward));
        assert_eq!(f.route.done, 2);
    }

    #[test]
    fn a_pick_off_the_route_plans_the_rest_again() {
        let mut f = followed();
        f.check = true;
        let wrong = seen(&[(1, Mark::Flash)], 1);
        assert_eq!(f.see(&wrong, Priority::Count, None), Some(Step::Replanned));
        let cells: Vec<usize> = f.route.steps.iter().map(|s| s.row * 3 + s.col).collect();
        assert_eq!(cells, vec![1, 4, 3]);
        assert_eq!((f.route.done, f.route.uploaded.clone(), f.check), (1, vec![0], false));
        // and the new route is followed like any other
        let next = seen(&[(1, Mark::Used), (4, Mark::Flash)], 2);
        assert_eq!(f.see(&next, Priority::Count, None), Some(Step::Onward));
        assert_eq!(f.route.done, 2);
    }

    #[test]
    fn a_look_that_does_not_fit_changes_nothing() {
        let mut f = followed();
        assert_eq!(f.see(&seen(&[(8, Mark::Used)], 1), Priority::Count, None), None);
        assert!(f.played.is_empty() && f.route.done == 0);
    }

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
