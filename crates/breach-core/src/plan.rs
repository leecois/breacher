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

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Route {
    pub steps: Vec<Step>,
    /// Daemons uploaded, by original index.
    pub uploaded: Vec<usize>,
    /// Wanted daemons the route does not reach.
    pub missed: Vec<usize>,
    /// How many of the first steps were already played when the route was planned.
    #[serde(default)]
    pub done: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
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
    Route { steps, uploaded, missed, done: 0 }
}

pub fn plan(reading: &Reading, opt: &PlanOptions) -> Plan {
    plan_from(reading, opt, &[]).unwrap_or_default()
}

/// Plans the rest of a breach that is under way: `played` are the picks made so far as (row, column), in order.
pub fn plan_from(
    reading: &Reading,
    opt: &PlanOptions,
    played: &[(usize, usize)],
) -> Result<Plan, String> {
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
    let n = reading.n;
    let start: Vec<u16> = played
        .iter()
        .map(|&(row, col)| (row < n && col < n).then_some((row * n + col) as u16))
        .collect::<Option<_>>()
        .ok_or("a pick lies outside the matrix")?;
    let solve = |matcher| {
        let so = solver::SolveOptions { matcher, node_budget: opt.node_budget, dp_bound: true };
        match solver::solve_from(&puzzle, &so, &start) {
            // a puzzle the solver does not take has no route, which is not an error for a fresh screen
            Err(_) if start.is_empty() => Ok(Solution { proven: true, ..Solution::default() }),
            other => other,
        }
    };
    let known = opt.matcher.unwrap_or(Matcher::Unanimous);
    let in_play = known.rules();
    let (main, main_matcher, optimistic, proven) = if in_play.iter().filter(|r| **r).count() == 1 {
        let s = solve(known)?;
        let proven = s.proven;
        (s, known, None, proven)
    } else {
        let safe = solve(known)?;
        // plain substring matching is the most generous rule: its route is worth offering while it is in play
        let best = if in_play[0] { Some(solve(Matcher::Substring)?) } else { None };
        let proven = safe.proven && best.as_ref().is_none_or(|b| b.proven);
        let alt = best.filter(|b| b.reward > safe.reward);
        (safe, known, alt, proven)
    };
    let resumed = |mut r: Route| {
        r.done = start.len();
        r
    };
    Ok(Plan {
        buffer,
        route: resumed(route(reading, &puzzle, &map, &main, main_matcher)),
        optimistic: optimistic
            .map(|s| resumed(route(reading, &puzzle, &map, &s, Matcher::Substring))),
        proven,
        solve_us: t.us(),
    })
}

/// One way a rule check can turn out.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Outcome {
    /// Daemons, by original index, that the game uploads in this case.
    pub uploaded: Vec<usize>,
    /// What is known about the rule once this has been seen.
    pub then: Matcher,
}

/// A route to play once, chosen so that what uploads tells the wrong-pick rules apart.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RuleCheck {
    pub route: Route,
    /// Every outcome the rules still in play predict, the one that uploads most first.
    pub outcomes: Vec<Outcome>,
}

/// The rule check this screen offers, given what `opt.matcher` already says about the rule. `None` when the rule is settled or this screen cannot tell the rest apart.
pub fn rule_check(reading: &Reading, opt: &PlanOptions) -> Option<RuleCheck> {
    let in_play = opt.matcher.unwrap_or(Matcher::Unanimous).rules();
    if in_play.iter().filter(|r| **r).count() < 2 {
        return None;
    }
    let puzzle = Puzzle::square(
        reading.n,
        reading.matrix.clone(),
        reading.sequences.iter().take(solver::MAX_DAEMONS).cloned().collect(),
        opt.buffer.unwrap_or(reading.buffer).min(solver::MAX_BUFFER),
    );
    let probe = solver::rule_probe_among(&puzzle, opt.node_budget, in_play)?;
    let masks = [probe.substring, probe.retest, probe.strict];
    let mut outcomes: Vec<Outcome> = Vec::new();
    for i in (0..3).filter(|&i| in_play[i]) {
        if (0..i).any(|j| in_play[j] && masks[j] == masks[i]) {
            continue;
        }
        let agree = [0, 1, 2].map(|j| in_play[j] && masks[j] == masks[i]);
        outcomes.push(Outcome {
            uploaded: (0..puzzle.daemons.len()).filter(|d| masks[i] >> d & 1 == 1).collect(),
            then: Matcher::of(agree)?,
        });
    }
    outcomes.sort_by_key(|o| std::cmp::Reverse(o.uploaded.len()));
    let steps = probe
        .path
        .iter()
        .map(|&cell| {
            let (row, col) = (cell as usize / puzzle.cols, cell as usize % puzzle.cols);
            let (x, y) = reading.geometry.cell_center(row, col);
            Step { row, col, code: puzzle.cells[cell as usize], x, y, uploads: Vec::new() }
        })
        .collect();
    Some(RuleCheck { route: Route { steps, ..Route::default() }, outcomes })
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
    fn a_breach_under_way_is_planned_from_where_it_stands() {
        // 3x3 with A B in the first column and C A B to be had after a wrong first pick
        let r = reading(3, vec![0, 2, 2, 1, 0, 2, 2, 1, 2], vec![vec![0, 1]], 4);
        let fresh = plan(&r, &PlanOptions::default());
        assert_eq!((fresh.route.steps.len(), fresh.route.done), (2, 0));
        // the player took the top middle cell instead: the daemon is still reachable down that column
        let rescued = plan_from(&r, &PlanOptions::default(), &[(0, 1)]).unwrap();
        let cells: Vec<(usize, usize)> =
            rescued.route.steps.iter().map(|s| (s.row, s.col)).collect();
        assert_eq!(cells, vec![(0, 1), (1, 1), (1, 0)]);
        assert_eq!((rescued.route.done, rescued.route.uploaded.clone()), (1, vec![0]));
        // on the planned route nothing changes but the count of steps behind
        let on_route = plan_from(&r, &PlanOptions::default(), &[(0, 0)]).unwrap();
        assert_eq!(on_route.route.steps, fresh.route.steps);
        assert_eq!(on_route.route.done, 1);
        assert!(
            plan_from(&r, &PlanOptions::default(), &[(1, 1)]).is_err(),
            "not a legal first pick"
        );
        assert!(plan_from(&r, &PlanOptions::default(), &[(0, 7)]).is_err(), "outside the matrix");
    }

    #[test]
    fn a_rule_check_narrows_the_rule_step_by_step() {
        // the only way to A B here is A A B: it uploads unless a wrong pick resets and is dropped
        let r = reading(3, vec![0, 2, 2, 0, 1, 2, 2, 2, 2], vec![vec![0, 1]], 3);
        let first = rule_check(&r, &PlanOptions::default()).expect("this screen tells rules apart");
        let cells: Vec<(usize, usize)> = first.route.steps.iter().map(|s| (s.row, s.col)).collect();
        assert_eq!(cells, vec![(0, 0), (1, 0), (1, 1)]);
        assert_eq!(first.outcomes.len(), 2);
        assert_eq!(
            (first.outcomes[0].uploaded.clone(), first.outcomes[0].then),
            (vec![0], Matcher::NotStrict)
        );
        assert_eq!(
            (first.outcomes[1].uploaded.clone(), first.outcomes[1].then),
            (vec![], Matcher::Strict)
        );
        let known: Vec<Matcher> = first.outcomes.iter().map(|o| o.then).collect();
        assert!(known.iter().all(|m| m.rules().iter().filter(|x| **x).count() < 3));
        // every rule in play is predicted by exactly one outcome
        for rule in 0..3 {
            assert_eq!(known.iter().filter(|m| m.rules()[rule]).count(), 1, "rule {rule}");
        }
        // once the rule is settled there is nothing left to check
        let settled = PlanOptions { matcher: Some(Matcher::Retest), ..PlanOptions::default() };
        assert_eq!(rule_check(&r, &settled), None);
        // with the strict rule ruled out this screen has nothing more to tell, and the daemon counts as safe
        let narrowed = PlanOptions { matcher: Some(Matcher::NotStrict), ..PlanOptions::default() };
        assert_eq!(rule_check(&r, &narrowed), None);
        assert_eq!(plan(&r, &narrowed).route.uploaded, vec![0]);
        assert_eq!(plan(&r, &PlanOptions::default()).route.uploaded, Vec::<usize>::new());
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
