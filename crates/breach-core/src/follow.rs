//! Following a breach as it is played: the picks seen so far, the daemons the game has settled, and the route that fits them.

use crate::glyph::{Gate, Templates};
use crate::image::Frame;
use crate::plan::{plan_from, PlanOptions, Priority, Route};
use crate::progress::{self, advance, Fate, Look as Seen};
use crate::reader::{buffer_codes, Reading};
use crate::solver::{uploaded_by, Matcher};
use serde::{Deserialize, Serialize};

/// The breach being followed: what was read, the route that is drawn and the picks seen so far.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Followed {
    pub reading: Reading,
    pub route: Route,
    /// The route is a rule check; leaving it turns it into an ordinary route.
    #[serde(default)]
    pub check: bool,
    /// The player's choices for this screen, as the window last reported them.
    #[serde(default)]
    pub wanted: Option<Vec<bool>>,
    #[serde(default)]
    pub buffer: Option<usize>,
    /// Cells picked so far, in order.
    #[serde(default)]
    pub played: Vec<usize>,
    /// What the game has made of each daemon so far.
    #[serde(default)]
    pub fates: Vec<Fate>,
}

/// What a newly seen pick did to the route.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Step {
    /// The pick was the next one of the route.
    Onward,
    /// The pick left the route, and the rest was planned again from where the breach stands.
    Replanned,
}

/// What a look shows of the breach as a whole.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sight {
    /// The matrix is there and daemons are still open.
    Breach,
    /// The matrix is not there.
    Other,
    /// The game has settled every daemon: nothing is left to play.
    Over,
}

/// What a finished breach showed about the wrong-pick rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "rule", rename_all = "snake_case")]
pub enum Learnt {
    /// Every rule still in play predicts what the game did.
    Nothing,
    /// Only the rules this stands for predict it: one is ruled out, or the rule is settled.
    Narrowed(Matcher),
    /// No rule still in play predicts it: something was misread, so nothing is concluded.
    Contradicted,
}

/// What one look added to what was known.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Looked {
    pub sight: Sight,
    /// Daemons the game has settled since the look before, counted from one.
    pub settled: Vec<(usize, Fate)>,
    /// What a newly seen pick did to the route.
    pub step: Option<Step>,
}

impl Followed {
    pub fn new(reading: Reading, route: Route, check: bool) -> Self {
        Followed {
            reading,
            route,
            check,
            wanted: None,
            buffer: None,
            played: Vec::new(),
            fates: Vec::new(),
        }
    }

    fn on_route(&self) -> bool {
        let n = self.reading.n;
        self.played.len() <= self.route.steps.len()
            && self.played.iter().zip(&self.route.steps).all(|(&c, s)| c == s.row * n + s.col)
    }

    /// Takes in the bars the game has laid over daemons; returns those that are new, counted from one.
    pub fn settled(&mut self, seen: &[Fate]) -> Vec<(usize, Fate)> {
        self.fates.resize(seen.len().max(self.fates.len()), Fate::Pending);
        let mut news = Vec::new();
        for (i, &fate) in seen.iter().enumerate() {
            if fate != Fate::Pending && self.fates[i] != fate {
                self.fates[i] = fate;
                news.push((i + 1, fate));
            }
        }
        news
    }

    /// Whether the game has settled every daemon, which ends the breach.
    pub fn over(&self) -> bool {
        self.fates.len() == self.reading.sequences.len() && !self.fates.contains(&Fate::Pending)
    }

    /// The daemons the game installed and those it failed, counted from one, once any is settled.
    pub fn outcome(&self) -> Option<(Vec<usize>, Vec<usize>)> {
        let of =
            |fate| (1..=self.fates.len()).filter(|i| self.fates[i - 1] == fate).collect::<Vec<_>>();
        let (installed, failed) = (of(Fate::Installed), of(Fate::Failed));
        (!installed.is_empty() || !failed.is_empty()).then_some((installed, failed))
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

    /// Takes in a route drawn afresh, which knows nothing of the picks already made, and the choices it was planned under.
    pub fn adopt(
        &mut self,
        route: Route,
        wanted: Option<Vec<bool>>,
        buffer: Option<usize>,
        check: bool,
        priority: Priority,
        matcher: Option<Matcher>,
    ) -> Option<Step> {
        (self.route, self.wanted, self.buffer, self.check) = (route, wanted, buffer, check);
        if self.played.is_empty() {
            None
        } else {
            self.settle(priority, matcher)
        }
    }

    /// The codes left in the buffer, read from a frame of the screen that follows the breach. `None` unless that frame shows the same bars over the daemons as when the breach ended, and a buffer that reads with confidence.
    pub fn final_codes(
        &self,
        frame: &Frame,
        templates: &Templates,
        gate: &Gate,
    ) -> Option<Vec<u8>> {
        let seen = progress::look(frame, &self.reading.geometry, self.reading.buffer);
        if !self.over() || seen.fates != self.fates {
            return None;
        }
        buffer_codes(frame, &self.reading, templates, gate).filter(|codes| !codes.is_empty())
    }

    /// Whether these codes can have been played: the picks that were seen, then cells that follow the rules of the matrix and hold the codes that come after.
    fn playable(&self, codes: &[u8]) -> bool {
        let (n, matrix) = (self.reading.n, &self.reading.matrix);
        if self.played.len() > codes.len() || self.played.iter().any(|&c| c >= matrix.len()) {
            return false;
        }
        if self.played.iter().zip(codes).any(|(&c, &code)| matrix[c] != code) {
            return false;
        }
        fn go(n: usize, matrix: &[u8], codes: &[u8], used: &mut Vec<usize>) -> bool {
            let k = used.len();
            if k == codes.len() {
                return true;
            }
            // the first pick comes from the top row, then columns and rows take turns
            let line: Vec<usize> = match used.last() {
                None => (0..n).collect(),
                Some(&last) if k % 2 == 1 => (0..n).map(|r| r * n + last % n).collect(),
                Some(&last) => (0..n).map(|c| last / n * n + c).collect(),
            };
            for cell in line {
                if matrix[cell] == codes[k] && !used.contains(&cell) {
                    used.push(cell);
                    if go(n, matrix, codes, used) {
                        return true;
                    }
                    used.pop();
                }
            }
            false
        }
        go(n, matrix, codes, &mut self.played.clone())
    }

    /// Holds what the game installed against what each rule still in play predicts for the codes that ended up in the buffer. `None` while a daemon is unsettled, or when the codes cannot have been played after the picks that were seen.
    pub fn learn(&self, codes: &[u8], known: Option<Matcher>) -> Option<Learnt> {
        let sequences = &self.reading.sequences;
        let fits_buffer = !codes.is_empty() && codes.len() <= self.reading.buffer.max(1);
        if !fits_buffer || !self.over() || !self.playable(codes) {
            return None;
        }
        let installed = self
            .fates
            .iter()
            .enumerate()
            .fold(0u32, |mask, (i, fate)| mask | ((*fate == Fate::Installed) as u32) << i);
        let in_play = known.map_or([true; 3], Matcher::rules);
        let single = [Matcher::Substring, Matcher::Retest, Matcher::Strict];
        let mut fits = [false; 3];
        for k in 0..3 {
            fits[k] = in_play[k] && uploaded_by(sequences, codes, single[k]) == installed;
        }
        Some(if fits == in_play {
            Learnt::Nothing
        } else {
            Matcher::of(fits).map_or(Learnt::Contradicted, Learnt::Narrowed)
        })
    }

    /// Reads one frame of the same screen: takes in the daemons the game has settled and the picks it shows.
    pub fn look(&mut self, frame: &Frame, priority: Priority, matcher: Option<Matcher>) -> Looked {
        let buffer = self.reading.buffer;
        let seen = progress::look(frame, &self.reading.geometry, buffer);
        let settled = self.settled(&seen.fates);
        let playing = seen.matrix_shown(buffer) && !seen.over();
        let step = if playing { self.see(&seen, priority, matcher) } else { None };
        let sight = if seen.over() {
            Sight::Over
        } else if playing {
            Sight::Breach
        } else {
            Sight::Other
        };
        Looked { sight, settled, step }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::plan;
    use crate::progress::Mark;
    use crate::reader::{Geometry, Quality, Timings};

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
        Followed::new(reading, route, false)
    }

    fn seen(marks: &[(usize, Mark)], committed: usize) -> Seen {
        let mut cells = vec![Mark::Code; 9];
        for &(cell, mark) in marks {
            cells[cell] = mark;
        }
        Seen { cells, committed, fates: Vec::new() }
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

    #[test]
    fn a_route_drawn_afresh_is_brought_in_line_with_the_picks_made() {
        let mut f = followed();
        let drawn = f.route.clone();
        // before any pick a new route is simply taken
        assert_eq!(f.adopt(drawn.clone(), None, None, false, Priority::Count, None), None);
        assert_eq!(f.see(&seen(&[(0, Mark::Flash)], 1), Priority::Count, None), Some(Step::Onward));
        // the same route again, knowing nothing of the pick, has it counted off
        let step = f.adopt(drawn.clone(), None, Some(4), false, Priority::Count, None);
        assert_eq!((step, f.route.done), (Some(Step::Onward), 1));
        // a route that does not start with the pick made gives way to one that does
        let mut other = drawn;
        other.steps.reverse();
        let step = f.adopt(other, None, None, true, Priority::Count, None);
        assert_eq!((step, f.route.done, f.check), (Some(Step::Replanned), 1, false));
    }

    #[test]
    fn the_bars_the_game_lays_over_daemons_are_noted_once() {
        let mut f = followed();
        assert_eq!(f.outcome(), None);
        assert_eq!(f.settled(&[Fate::Pending]), vec![]);
        assert_eq!(f.settled(&[Fate::Installed]), vec![(1, Fate::Installed)]);
        assert_eq!(f.settled(&[Fate::Installed]), vec![]);
        // a bar hidden for a moment does not unsettle the daemon
        assert_eq!(f.settled(&[Fate::Pending]), vec![]);
        assert_eq!(f.outcome(), Some((vec![1], vec![])));
    }

    #[test]
    fn a_finished_breach_tells_the_rules_apart() {
        // A A B against the codes A A A B, picked down column 1, along row 2 and down column 2: only a rule that keeps overlapping codes uploads it
        let mut f = followed();
        f.reading.matrix = vec![0, 2, 2, 0, 0, 2, 2, 1, 2];
        f.reading.sequences = vec![vec![0, 0, 1]];
        let codes = [0, 0, 0, 1];
        assert_eq!(
            f.learn(&codes, None),
            None,
            "nothing is learnt before the game has settled the daemon"
        );
        f.fates = vec![Fate::Installed];
        assert_eq!(f.learn(&codes, None), Some(Learnt::Narrowed(Matcher::Substring)));
        f.fates = vec![Fate::Failed];
        assert_eq!(f.learn(&codes, None), Some(Learnt::Narrowed(Matcher::NotSubstring)));
        // what is already known is not news, and a pair that is down to two can be settled
        assert_eq!(f.learn(&codes, Some(Matcher::NotSubstring)), Some(Learnt::Nothing));
        assert_eq!(
            f.learn(&codes, Some(Matcher::NotRetest)),
            Some(Learnt::Narrowed(Matcher::Strict))
        );
        assert_eq!(f.learn(&codes, Some(Matcher::Substring)), Some(Learnt::Contradicted));
        // the picks that were seen have to be the start of what was played
        f.played = vec![0, 3];
        assert_eq!(f.learn(&codes, None), Some(Learnt::Narrowed(Matcher::NotSubstring)));
        f.played = vec![1];
        assert_eq!(f.learn(&codes, None), None);
        // codes that cannot be picked in this matrix are a misreading: row 2 holds no B
        f.played = vec![];
        assert_eq!(f.learn(&[0, 0, 1], None), None);
        // nor can more codes be played than the buffer holds
        assert_eq!(f.learn(&[0, 0, 0, 1, 2], None), None);
        assert_eq!(f.learn(&[], None), None);
        // codes every rule reads alike tell nothing
        f.reading.sequences = vec![vec![0, 0]];
        f.fates = vec![Fate::Installed];
        assert_eq!(f.learn(&[0, 0], None), Some(Learnt::Nothing));
    }

    #[test]
    fn what_was_learnt_reads_as_plain_json() {
        let narrowed = serde_json::to_string(&Learnt::Narrowed(Matcher::NotStrict)).unwrap();
        assert_eq!(narrowed, r#"{"kind":"narrowed","rule":"not_strict"}"#);
        assert_eq!(serde_json::to_string(&Learnt::Nothing).unwrap(), r#"{"kind":"nothing"}"#);
    }

    #[test]
    fn a_followed_breach_survives_a_trip_through_json() {
        let mut f = followed();
        f.fates = vec![Fate::Installed];
        f.played = vec![0];
        let json = serde_json::to_string(&f).unwrap();
        assert!(json.contains("\"fates\":[\"installed\"]"), "{json}");
        let back: Followed = serde_json::from_str(&json).unwrap();
        assert_eq!((&back.played, &back.fates, &back.route), (&f.played, &f.fates, &f.route));
        // only the reading and the route have to be given
        let bare = serde_json::json!({ "reading": f.reading, "route": f.route });
        let bare: Followed = serde_json::from_value(bare).unwrap();
        assert!(bare.played.is_empty() && bare.wanted.is_none() && !bare.check);
    }
}
