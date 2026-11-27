//! Progress of a breach under way, read from a later look at the screen its reading came from.

use crate::image::Frame;
use crate::reader::Geometry;
use serde::{Deserialize, Serialize};

/// What a matrix cell shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    /// Its code, untouched.
    Code,
    /// The pointer's box: what is under it cannot be told.
    Hover,
    /// The flash of a pick just made.
    Flash,
    /// The brackets left where a code was picked.
    Used,
}

/// What has become of a daemon.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fate {
    /// Its sequence is still shown: it can still be uploaded.
    Pending,
    /// The game has put its green bar over it.
    Installed,
    /// The game has put its red bar over it.
    Failed,
}

/// What one look shows of the picks made so far.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Look {
    /// One mark per matrix cell, row by row.
    pub cells: Vec<Mark>,
    /// Buffer slots filled for good; the preview of a code under the pointer does not count.
    pub committed: usize,
    /// One fate per daemon, in the order of the sequence list.
    pub fates: Vec<Fate>,
}

impl Look {
    /// Whether the matrix is still there: a breach leaves at most one pick per buffer slot, and the pointer covers one cell more.
    pub fn matrix_shown(&self, buffer: usize) -> bool {
        self.cells.iter().filter(|m| **m != Mark::Code).count() <= buffer + 2
    }

    /// Whether the game has settled every daemon, which ends the breach.
    pub fn over(&self) -> bool {
        !self.fates.is_empty() && self.fates.iter().all(|f| *f != Fate::Pending)
    }
}

/// Half the brightness of the brightest code: what counts as lit on this screen.
fn lit(frame: &Frame, g: &Geometry) -> u32 {
    let mut brightest = 0u32;
    for i in 0..g.n * g.n {
        let (cx, cy) = g.cell_center(i / g.n, i % g.n);
        let half = 0.3 * g.px.min(g.py);
        let (x0, x1) = ((cx - half).max(0.0) as usize, ((cx + half) as usize).min(frame.width - 1));
        let (y0, y1) =
            ((cy - half).max(0.0) as usize, ((cy + half) as usize).min(frame.height - 1));
        for y in (y0..=y1).step_by(2) {
            for x in (x0..=x1).step_by(2) {
                let [r, g, b] = frame.rgb(x, y);
                brightest = brightest.max(r.max(g).max(b) as u32);
            }
        }
    }
    (brightest * 45 / 100).max(60)
}

/// Share of the pixels in a box that are lit in the yellow-green of codes, and in the cyan of the pointer's marks.
fn tints(frame: &Frame, cx: f32, cy: f32, half: f32, lit: u32) -> (f32, f32) {
    // the outer fifth holds the pointer's box and the neighbours' edges, not the content
    let inner = half * 0.8;
    let (x0, x1) = ((cx - inner).max(0.0) as usize, ((cx + inner) as usize).min(frame.width - 1));
    let (y0, y1) = ((cy - inner).max(0.0) as usize, ((cy + inner) as usize).min(frame.height - 1));
    let (mut all, mut yellow, mut cyan) = (0u32, 0u32, 0u32);
    for y in y0..=y1 {
        for x in x0..=x1 {
            let [r, g, b] = frame.rgb(x, y).map(u32::from);
            all += 1;
            if r.max(g).max(b) > lit {
                yellow += (4 * r > 5 * b) as u32;
                cyan += (4 * b > 5 * r) as u32;
            }
        }
    }
    (yellow as f32 / all.max(1) as f32, cyan as f32 / all.max(1) as f32)
}

/// The bar the game lays over a daemon's row once it is settled, told by its colour.
fn fate(frame: &Frame, g: &Geometry, row: f32, lit: u32) -> Fate {
    let (x0, x1) = (
        (g.seq_x0 - 0.5 * g.seq_pitch_x).max(0.0) as usize,
        ((g.seq_x0 + 5.5 * g.seq_pitch_x) as usize).min(frame.width - 1),
    );
    let (y0, y1) = (
        (row - 0.25 * g.py).max(0.0) as usize,
        ((row + 0.25 * g.py) as usize).min(frame.height - 1),
    );
    let (mut all, mut green, mut red) = (0u32, 0u32, 0u32);
    for y in y0..=y1 {
        for x in x0..=x1 {
            let [r, g, b] = frame.rgb(x, y).map(u32::from);
            all += 1;
            if r.max(g).max(b) > lit {
                green += (2 * g > 3 * r && 5 * g > 6 * b) as u32;
                red += (2 * r > 3 * g && 2 * r > 3 * b) as u32;
            }
        }
    }
    if 5 * green >= 3 * all {
        Fate::Installed
    } else if 5 * red >= 3 * all {
        Fate::Failed
    } else {
        Fate::Pending
    }
}

/// Reads the marks of every cell, the fill of the buffer and the fate of every daemon from a frame of the same screen.
pub fn look(frame: &Frame, g: &Geometry, buffer: usize) -> Look {
    let lit = lit(frame, g);
    let cells = (0..g.n * g.n)
        .map(|i| {
            let (cx, cy) = g.cell_center(i / g.n, i % g.n);
            let (yellow, cyan) = tints(frame, cx, cy, 0.49 * g.px.min(g.py), lit);
            if cyan >= 0.6 {
                Mark::Flash
            } else if yellow >= 0.04 {
                Mark::Code
            } else if cyan >= 0.04 {
                Mark::Hover
            } else {
                Mark::Used
            }
        })
        .collect();
    let committed = (0..buffer)
        .take_while(|&slot| {
            let cx = g.seq_x0 + slot as f32 * g.seq_pitch_x;
            let (yellow, cyan) = tints(frame, cx, g.buffer_y, 0.49 * g.seq_pitch_x, lit);
            // a filled slot is drawn in the codes' colour, or flashes; the pointer's preview is an outline
            cyan >= 0.6 || (yellow >= 0.15 && cyan < 0.1)
        })
        .count();
    let fates = g.seq_rows.iter().map(|&row| fate(frame, g, row, lit)).collect();
    Look { cells, committed, fates }
}

fn follows(n: usize, picks: &[usize], next: usize) -> bool {
    match picks.last() {
        None => next / n == 0,
        Some(&last) if picks.len() % 2 == 1 => next % n == last % n,
        Some(&last) => next / n == last / n,
    }
}

fn orderings(n: usize, picks: &mut Vec<usize>, pool: &mut Vec<usize>, found: &mut Vec<Vec<usize>>) {
    if pool.is_empty() {
        found.push(picks.clone());
        return;
    }
    for i in 0..pool.len() {
        let cell = pool[i];
        if follows(n, picks, cell) {
            pool.remove(i);
            picks.push(cell);
            orderings(n, picks, pool, found);
            picks.pop();
            pool.insert(i, cell);
        }
    }
}

/// The picks made so far, as cell indices in order, given those known before and what a look shows. `None` when the look does not fit them or leaves the order open.
pub fn advance(n: usize, played: &[usize], look: &Look) -> Option<Vec<usize>> {
    if look.cells.len() != n * n || look.committed < played.len() {
        return None;
    }
    // a pick that shows its code again was never one
    if played.iter().any(|&c| look.cells[c] == Mark::Code) {
        return None;
    }
    let new = |mark: Mark| -> Vec<usize> {
        (0..n * n).filter(|c| !played.contains(c) && look.cells[*c] == mark).collect()
    };
    let mut pool = [new(Mark::Used), new(Mark::Flash)].concat();
    let missing = look.committed - played.len();
    if pool.len() + 1 == missing {
        // the pointer may still rest on the cell it picked, hiding what became of it
        let under = new(Mark::Hover);
        if under.len() != 1 {
            return None;
        }
        pool.push(under[0]);
    }
    if pool.len() != missing {
        return None;
    }
    let mut found = Vec::new();
    orderings(n, &mut played.to_vec(), &mut pool, &mut found);
    (found.len() == 1).then(|| found.remove(0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use Mark::*;

    fn seen(n: usize, marks: &[(usize, usize, Mark)], committed: usize) -> Look {
        let mut cells = vec![Code; n * n];
        for &(row, col, mark) in marks {
            cells[row * n + col] = mark;
        }
        Look { cells, committed, fates: Vec::new() }
    }

    #[test]
    fn picks_are_followed_one_look_at_a_time() {
        let n = 6;
        // before the first pick the pointer rests on a cell, and its code is previewed in the buffer
        let hovering = seen(n, &[(0, 5, Hover)], 0);
        assert_eq!(advance(n, &[], &hovering), Some(vec![]));
        // the pick flashes, then leaves brackets
        let first = seen(n, &[(0, 5, Flash)], 1);
        assert_eq!(advance(n, &[], &first), Some(vec![5]));
        let second = seen(n, &[(0, 5, Used), (1, 5, Used), (1, 0, Hover)], 2);
        assert_eq!(advance(n, &[5], &second), Some(vec![5, 11]));
    }

    #[test]
    fn several_picks_between_two_looks_are_put_in_the_only_legal_order() {
        let n = 6;
        let later =
            seen(n, &[(0, 5, Used), (1, 5, Used), (1, 0, Used), (0, 0, Used), (0, 4, Hover)], 4);
        assert_eq!(advance(n, &[5], &later), Some(vec![5, 11, 6, 0]));
        // from nothing the same four cells could have been walked from either end of the top row
        assert_eq!(advance(n, &[], &later), None);
    }

    #[test]
    fn a_pick_still_under_the_pointer_is_taken_from_the_buffer_count() {
        let n = 6;
        let resting = seen(n, &[(0, 5, Used), (1, 5, Hover)], 2);
        assert_eq!(advance(n, &[5], &resting), Some(vec![5, 11]));
        // with the buffer saying one pick, the same pointer is only hovering
        let hovering = seen(n, &[(0, 5, Used), (1, 5, Hover)], 1);
        assert_eq!(advance(n, &[5], &hovering), Some(vec![5]));
    }

    #[test]
    fn a_matrix_that_is_gone_and_a_breach_that_is_over_are_told() {
        let n = 6;
        let mut playing = seen(n, &[(0, 5, Used), (1, 5, Used), (1, 0, Hover)], 2);
        assert!(playing.matrix_shown(8) && !playing.over());
        playing.fates = vec![Fate::Installed, Fate::Pending];
        assert!(!playing.over());
        playing.fates = vec![Fate::Installed, Fate::Failed];
        assert!(playing.over());
        let gone = Look { cells: vec![Used; n * n], committed: 0, fates: Vec::new() };
        assert!(!gone.matrix_shown(8));
    }

    #[test]
    fn a_look_that_does_not_fit_is_refused() {
        let n = 6;
        // brackets where the rules allow no pick
        assert_eq!(advance(n, &[], &seen(n, &[(3, 3, Used)], 1)), None);
        // more brackets than the buffer accounts for
        assert_eq!(
            advance(n, &[5], &seen(n, &[(0, 5, Used), (1, 5, Used), (1, 0, Used)], 2)),
            None
        );
        // a known pick showing its code again
        assert_eq!(advance(n, &[5], &seen(n, &[], 1)), None);
        // two cells that could each have been the second pick
        let open = seen(n, &[(0, 5, Used), (1, 5, Hover), (2, 5, Hover)], 2);
        assert_eq!(advance(n, &[5], &open), None);
        // the buffer behind what is already known
        assert_eq!(advance(n, &[5, 11], &seen(n, &[(0, 5, Used), (1, 5, Used)], 1)), None);
    }
}
