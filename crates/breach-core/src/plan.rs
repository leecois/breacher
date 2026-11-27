//! Turns a reading into the route to show, pick by pick, with screen positions.

use crate::clock::Stopwatch;
use crate::reader::Reading;
use crate::solver::{self, Daemon, Matcher, Puzzle, Solution};
use serde::{Deserialize, Serialize};

/// How daemons are ranked against each other.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    /// Upload as many daemons as possible; later (harder) daemons win ties.
    Count,
    /// A later daemon outranks every combination of earlier ones (datamines: v3 before v2 before v1).
    Tier,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlanOptions {
    /// Use this buffer size instead of the one read from the screen.
    pub buffer: Option<usize>,
    /// Daemons to go for, by index. `None` = all.
    pub wanted: Option<Vec<bool>>,
    pub priority: Priority,
    /// The in-game matching rule, once confirmed. `None` = prefer routes that work under every rule.
    pub matcher: Option<Matcher>,
    pub node_budget: u64,
}

impl Default for PlanOptions {
    fn default() -> Self {
        PlanOptions {
            buffer: None,
            wanted: None,
            priority: Priority::Count,
            matcher: None,
            node_budget: 5_000_000,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Step {
    pub row: usize,
    pub col: usize,
    pub code: u8,
    /// Centre of the cell in frame pixels.
    pub x: f32,
    pub y: f32,
    /// Daemons (original indices) uploaded by this pick.
    pub uploads: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Route {
    pub steps: Vec<Step>,
    /// Daemons uploaded, by original index.
    pub uploaded: Vec<usize>,
    /// Wanted daemons the route does not reach.
    pub missed: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Plan {
    pub buffer: usize,
    pub route: Route,
    /// Set when a route relying on plain substring matching would upload more; offered, not recommended.
    pub optimistic: Option<Route>,
    /// The search finished; `false` means the node budget ran out and the route is best-so-far.
    pub proven: bool,
    pub solve_us: u32,
}

fn weights(count: usize, priority: Priority) -> Vec<u32> {
    (0..count)
        .map(|i| match priority {
            Priority::Count => solver::default_weight(i),
            Priority::Tier => 1u32 << (2 * i.min(14)),
        })
        .collect()
}

fn route(
    reading: &Reading,
    puzzle: &Puzzle,
    map: &[usize],
    sol: &Solution,
    matcher: Matcher,
) -> Route {
    let replay = solver::verify(puzzle, &sol.path, matcher).ok();
    let steps = sol
        .path
        .iter()
        .enumerate()
        .map(|(i, &cell)| {
            let (row, col) = (cell as usize / puzzle.cols, cell as usize % puzzle.cols);
            let (x, y) = reading.geometry.cell_center(row, col);
            let mask = replay.as_ref().and_then(|r| r.uploaded_at.get(i)).copied().unwrap_or(0);
            Step {
                row,
                col,
                code: puzzle.cells[cell as usize],
                x,
                y,
                uploads: (0..map.len()).filter(|&d| mask >> d & 1 == 1).map(|d| map[d]).collect(),
            }
        })
        .collect();
    let uploaded: Vec<usize> =
        (0..map.len()).filter(|&d| sol.uploaded >> d & 1 == 1).map(|d| map[d]).collect();
    let missed = map.iter().copied().filter(|d| !uploaded.contains(d)).collect();
    Route { steps, uploaded, missed }
}

pub fn plan(reading: &Reading, opt: &PlanOptions) -> Plan {
    let t = Stopwatch::start();
    let buffer = opt.buffer.unwrap_or(reading.buffer).min(solver::MAX_BUFFER);
    let total = reading.sequences.len();
    let w = weights(total, opt.priority);
    let map: Vec<usize> = (0..total)
        .filter(|&i| opt.wanted.as_ref().is_none_or(|v| v.get(i).copied().unwrap_or(true)))
        .take(solver::MAX_DAEMONS)
        .collect();
    let puzzle = Puzzle {
        rows: reading.n,
        cols: reading.n,
        cells: reading.matrix.clone(),
        daemons: map
            .iter()
            .map(|&i| Daemon { seq: reading.sequences[i].clone(), weight: w[i] })
            .collect(),
        buffer,
    };
    let so =
        |matcher| solver::SolveOptions { matcher, node_budget: opt.node_budget, dp_bound: true };
    let (main, main_matcher, optimistic, proven) = match opt.matcher {
        Some(m) => {
            let s = solver::solve_with(&puzzle, &so(m));
            let proven = s.proven;
            (s, m, None, proven)
        }
        None => {
            let safe = solver::solve_with(&puzzle, &so(Matcher::Unanimous));
            let best = solver::solve_with(&puzzle, &so(Matcher::Substring));
            let proven = safe.proven && best.proven;
            let alt = (best.reward > safe.reward).then_some(best);
            (safe, Matcher::Unanimous, alt, proven)
        }
    };
    Plan {
        buffer,
        route: route(reading, &puzzle, &map, &main, main_matcher),
        optimistic: optimistic.map(|s| route(reading, &puzzle, &map, &s, Matcher::Substring)),
        proven,
        solve_us: t.us(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader::{Geometry, Quality, Timings};

    fn reading(n: usize, matrix: Vec<u8>, sequences: Vec<Vec<u8>>, buffer: usize) -> Reading {
        Reading {
            n,
            matrix,
            sequences,
            buffer,
            geometry: Geometry {
                x0: 100.0,
                y0: 200.0,
                px: 64.0,
                py: 64.0,
                n,
                seq_x0: 800.0,
                seq_pitch_x: 42.0,
                seq_rows: vec![],
                buffer_y: 40.0,
            },
            quality: Quality::default(),
            timings: Timings::default(),
        }
    }

    #[test]
    fn steps_carry_screen_positions_and_upload_events() {
        // 3x3: route 0 -> (0,0)=A, then column 0 -> (1,0)=B uploads [A,B]
        let r = reading(3, vec![0, 2, 2, 1, 2, 2, 2, 2, 2], vec![vec![0, 1]], 4);
        let p = plan(&r, &PlanOptions::default());
        assert_eq!(p.route.uploaded, vec![0]);
        assert_eq!(p.route.steps.len(), 2);
        assert_eq!((p.route.steps[0].row, p.route.steps[0].col), (0, 0));
        assert_eq!((p.route.steps[1].row, p.route.steps[1].col), (1, 0));
        assert_eq!((p.route.steps[1].x, p.route.steps[1].y), (100.0, 264.0));
        assert_eq!(p.route.steps[1].uploads, vec![0]);
        assert!(p.proven && p.optimistic.is_none());
    }

    #[test]
    fn unwanted_daemons_are_left_out_and_indices_survive() {
        let r = reading(
            3,
            vec![0, 1, 2, 1, 2, 0, 2, 0, 1],
            vec![vec![0, 1], vec![2, 2], vec![1, 2]],
            2,
        );
        let all = plan(&r, &PlanOptions::default());
        let only_last = plan(
            &r,
            &PlanOptions { wanted: Some(vec![false, false, true]), ..PlanOptions::default() },
        );
        assert_eq!(only_last.route.uploaded, vec![2]);
        assert!(only_last.route.missed.is_empty());
        assert!(!all.route.uploaded.is_empty());
    }

    #[test]
    fn buffer_override_and_tier_priority() {
        // two daemons that cannot both fit in 2 picks: [A,B] (index 0) and [C,D] (index 1)
        let r = reading(3, vec![0, 2, 9 % 4, 1, 3, 0, 0, 0, 0], vec![vec![0, 1], vec![2, 3]], 2);
        let tier = plan(&r, &PlanOptions { priority: Priority::Tier, ..PlanOptions::default() });
        assert_eq!(tier.route.uploaded, vec![1], "tier priority goes for the later daemon");
        let small = plan(&r, &PlanOptions { buffer: Some(1), ..PlanOptions::default() });
        assert!(small.route.uploaded.is_empty());
        assert_eq!(small.buffer, 1);
    }

    #[test]
    fn rule_sensitive_puzzles_offer_the_optimistic_route_separately() {
        // A A B reachable only as A A A B: (0,0)=A, (1,0)=A, (1,1)=A, (2,1)=B
        #[rustfmt::skip]
        let m = vec![
            0, 2, 2, 2,
            0, 0, 2, 2,
            2, 1, 2, 2,
            2, 2, 2, 2,
        ];
        let r = reading(4, m, vec![vec![0, 0, 1]], 4);
        let p = plan(&r, &PlanOptions::default());
        if let Some(opt) = &p.optimistic {
            assert_eq!(opt.uploaded, vec![0]);
            assert!(p.route.uploaded.is_empty());
        } else {
            // a clean A A B route exists as well; then it must be the recommended one
            assert_eq!(p.route.uploaded, vec![0]);
        }
        let forced =
            plan(&r, &PlanOptions { matcher: Some(Matcher::Substring), ..PlanOptions::default() });
        assert_eq!(forced.route.uploaded, vec![0]);
        assert!(forced.optimistic.is_none());
    }
}
