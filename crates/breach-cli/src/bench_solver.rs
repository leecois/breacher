use crate::gen::{puzzle, Rng};
use breach_core::solver::*;
use std::time::Instant;

fn pct(v: &mut [f64], p: f64) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[((v.len() - 1) as f64 * p).round() as usize]
}

#[allow(clippy::print_literal)]
pub fn run(stress: bool) {
    let profiles: [(&str, &[usize]); 2] =
        [("datamine-3", &[2, 3, 4]), ("legacy-7", &[3, 3, 4, 4, 3, 3, 4])];
    let grid: Vec<(usize, usize, usize, u64)> = if stress {
        vec![
            (8, 10, 30, 20_000_000),
            (8, 12, 30, 20_000_000),
            (9, 12, 20, 20_000_000),
            (10, 14, 10, 20_000_000),
        ]
    } else {
        let mut g = Vec::new();
        for n in [5usize, 6, 7] {
            for b in [4usize, 6, 8, 9, 10, 12] {
                g.push((n, b, 200, u64::MAX));
            }
        }
        g
    };
    println!(
        "{:<11} {:>2} {:>3} | {:>8} {:>8} {:>9} {:>9} | {:>8} {:>9} {:>10} | {:>9} | {:>6} {:>6} | {}",
        "profile", "n", "buf", "med µs", "p99 µs", "max µs", "max nodes", "old med", "old max", "old nodes", "robust µs", "full%", "sens%", "unproven"
    );
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    for (pname, lens) in profiles {
        for &(n, buffer, count, budget) in &grid {
            let codes = if n <= 6 { 5 } else { 6 };
            let (mut t, mut told, mut trob) = (Vec::new(), Vec::new(), Vec::new());
            let (mut maxn, mut maxn_old, mut full, mut sens, mut unproven) =
                (0u64, 0u64, 0u32, 0u32, 0u32);
            for _ in 0..count {
                let p = puzzle(&mut rng, n, codes, lens, buffer);
                let t0 = Instant::now();
                let s = solve_with(
                    &p,
                    &SolveOptions {
                        matcher: Matcher::Substring,
                        node_budget: budget,
                        dp_bound: true,
                    },
                );
                t.push(t0.elapsed().as_secs_f64() * 1e6);
                maxn = maxn.max(s.nodes);
                unproven += !s.proven as u32;
                if s.reward > 0 {
                    assert_eq!(verify(&p, &s.path, Matcher::Substring).unwrap().reward, s.reward);
                }
                // the cheap bound for comparison; skipped in stress, where it does not finish
                if !stress {
                    let t1 = Instant::now();
                    let o = solve_with(
                        &p,
                        &SolveOptions {
                            matcher: Matcher::Substring,
                            node_budget: budget,
                            dp_bound: false,
                        },
                    );
                    told.push(t1.elapsed().as_secs_f64() * 1e6);
                    maxn_old = maxn_old.max(o.nodes);
                    assert_eq!((o.reward, o.path.len()), (s.reward, s.path.len()));
                }
                let t2 = Instant::now();
                let r = solve_robust(&p, budget);
                trob.push(t2.elapsed().as_secs_f64() * 1e6);
                sens += r.rule_sensitive as u32;
                full += (s.reward == p.total_weight()) as u32;
            }
            let (med, p99) = (pct(&mut t, 0.5), pct(&mut t, 0.99));
            let mx = t.iter().cloned().fold(0.0, f64::max);
            let (omed, omax) = if told.is_empty() {
                (f64::NAN, f64::NAN)
            } else {
                (pct(&mut told, 0.5), told.iter().cloned().fold(0.0, f64::max))
            };
            println!(
                "{:<11} {:>2} {:>3} | {:>8.1} {:>8.1} {:>9.1} {:>9} | {:>8.1} {:>9.1} {:>10} | {:>9.1} | {:>5.0}% {:>5.1}% | {}/{}",
                pname, n, buffer, med, p99, mx, maxn, omed, omax, maxn_old, pct(&mut trob, 0.5),
                100.0 * full as f64 / count as f64, 100.0 * sens as f64 / count as f64, unproven, count
            );
        }
    }
}
