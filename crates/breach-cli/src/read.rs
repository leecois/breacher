//! `breach read`: one image in, matrix + route out.
use crate::imgio;
use breach_core::glyph::Templates;
use breach_core::plan::{plan, Plan, PlanOptions, Priority, Route};
use breach_core::reader::{read, ReadOptions, Reading};
use breach_core::solver::Matcher;
use breach_core::CODES;
use std::path::Path;
use std::time::Instant;

pub struct ReadArgs {
    pub json: bool,
    pub buffer: Option<usize>,
    pub tier: bool,
    pub matcher: Option<Matcher>,
    pub skip: Vec<usize>,
}

pub fn parse_matcher(s: &str) -> Result<Matcher, String> {
    match s {
        "substring" => Ok(Matcher::Substring),
        "retest" => Ok(Matcher::Retest),
        "strict" => Ok(Matcher::Strict),
        "unanimous" => Ok(Matcher::Unanimous),
        _ => Err(format!("unknown matcher '{s}' (substring, retest, strict, unanimous)")),
    }
}

fn grid(r: &Reading, route: &Route) -> String {
    let mut out = String::new();
    let step_of =
        |row: usize, col: usize| route.steps.iter().position(|s| s.row == row && s.col == col);
    out.push_str("      ");
    for c in 0..r.n {
        out.push_str(&format!("  c{:<3}", c + 1));
    }
    out.push('\n');
    for row in 0..r.n {
        out.push_str(&format!("  r{:<2} ", row + 1));
        for col in 0..r.n {
            let code = CODES[r.matrix[row * r.n + col] as usize];
            match step_of(row, col) {
                Some(i) => out.push_str(&format!("{:>2}>{code} ", i + 1)),
                None => out.push_str(&format!("   {code} ")),
            }
        }
        out.push('\n');
    }
    out
}

fn describe(r: &Reading, p: &Plan, route: &Route) -> String {
    let mut s = grid(r, route);
    s.push('\n');
    for (i, seq) in r.sequences.iter().enumerate() {
        let mark = if route.uploaded.contains(&i) {
            "upload"
        } else if route.missed.contains(&i) {
            "miss  "
        } else {
            "skip  "
        };
        let at = route
            .steps
            .iter()
            .position(|st| st.uploads.contains(&i))
            .map(|k| format!(" (done at pick {})", k + 1))
            .unwrap_or_default();
        s.push_str(&format!(
            "  {mark} #{}  {}{at}\n",
            i + 1,
            seq.iter().map(|&c| CODES[c as usize]).collect::<Vec<_>>().join(" ")
        ));
    }
    s.push_str(&format!(
        "\n  picks {}/{}: {}\n",
        route.steps.len(),
        p.buffer,
        route
            .steps
            .iter()
            .map(|st| format!("r{}c{} {}", st.row + 1, st.col + 1, CODES[st.code as usize]))
            .collect::<Vec<_>>()
            .join(" -> ")
    ));
    s
}

pub fn run(path: &Path, args: &ReadArgs) -> Result<(), String> {
    let img = imgio::load(path)?;
    let t = Instant::now();
    let read_opt = ReadOptions { buffer_fallback: args.buffer, ..ReadOptions::default() };
    let reading =
        read(&imgio::frame(&img), &Templates::builtin(), &read_opt).map_err(|e| e.to_string())?;
    let read_ms = t.elapsed().as_secs_f64() * 1e3;
    let wanted = (!args.skip.is_empty())
        .then(|| (0..reading.sequences.len()).map(|i| !args.skip.contains(&(i + 1))).collect());
    let opt = PlanOptions {
        buffer: args.buffer,
        wanted,
        priority: if args.tier { Priority::Tier } else { Priority::Count },
        matcher: args.matcher,
        ..PlanOptions::default()
    };
    let p = plan(&reading, &opt);
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({ "reading": reading, "plan": p }))
                .map_err(|e| e.to_string())?
        );
        return Ok(());
    }
    println!(
        "{}x{} matrix, buffer {}{}, {} daemon(s)   [read {:.1} ms, solved {:.2} ms, frame {}x{}]\n",
        reading.n,
        reading.n,
        p.buffer,
        if reading.quality.buffer_assumed {
            " (not readable on screen)".to_string()
        } else if args.buffer.is_some() {
            format!(" (screen shows {})", reading.buffer)
        } else {
            String::new()
        },
        reading.sequences.len(),
        read_ms,
        p.solve_us as f64 / 1e3,
        img.width(),
        img.height()
    );
    print!("{}", describe(&reading, &p, &p.route));
    if !p.proven {
        println!("  note: search budget ran out; this is the best route found, not proven optimal");
    }
    match (&p.optimistic, args.matcher) {
        (Some(alt), None) => {
            println!(
                "\n  This route works whatever rule the game uses for a wrong pick mid-sequence."
            );
            println!("  If the game matches plain substrings, this longer shot uploads more ({} vs {} daemons):\n", alt.uploaded.len(), p.route.uploaded.len());
            print!("{}", describe(&reading, &p, alt));
        }
        (None, None) => println!("  works under every matching rule"),
        _ => {}
    }
    Ok(())
}

/// `breach rule-probe`: a route that tells the three candidate wrong-pick rules apart.
pub fn probe(path: &Path, buffer: Option<usize>) -> Result<(), String> {
    use breach_core::solver::{rule_probe, Puzzle};
    let img = imgio::load(path)?;
    let reading = read(&imgio::frame(&img), &Templates::builtin(), &ReadOptions::default())
        .map_err(|e| e.to_string())?;
    let mut puzzle: Puzzle = reading.to_puzzle();
    if let Some(b) = buffer {
        puzzle.buffer = b;
    }
    let Some(pr) = rule_probe(&puzzle, 5_000_000) else {
        println!("No probe on this screen: every legal route gives the same result under all three rules.\nBack out before the first pick, reopen the breach for a new matrix, and try again.");
        return Ok(());
    };
    let n = reading.n;
    let route = Route {
        steps: pr
            .path
            .iter()
            .map(|&c| {
                let (row, col) = (c as usize / n, c as usize % n);
                let (x, y) = reading.geometry.cell_center(row, col);
                breach_core::plan::Step {
                    row,
                    col,
                    code: reading.matrix[c as usize],
                    x,
                    y,
                    uploads: vec![],
                }
            })
            .collect(),
        ..Route::default()
    };
    println!("{}x{} matrix, buffer {}\n", n, n, puzzle.buffer);
    print!("{}", grid(&reading, &route));
    println!(
        "\n  picks: {}\n",
        route
            .steps
            .iter()
            .map(|st| format!("r{}c{} {}", st.row + 1, st.col + 1, CODES[st.code as usize]))
            .collect::<Vec<_>>()
            .join(" -> ")
    );
    let list = |mask: u32| {
        let v: Vec<String> = (0..reading.sequences.len())
            .filter(|d| mask >> d & 1 == 1)
            .map(|d| format!("#{}", d + 1))
            .collect();
        if v.is_empty() {
            "nothing".to_string()
        } else {
            v.join(", ")
        }
    };
    println!("  Follow these picks exactly, then note which daemons the game marks as uploaded:");
    println!(
        "    {:<10} -> the game matches plain substrings   (--matcher substring)",
        list(pr.substring)
    );
    println!(
        "    {:<10} -> progress resets, the pick is re-read (--matcher retest)",
        list(pr.retest)
    );
    println!(
        "    {:<10} -> progress resets, the pick is dropped (--matcher strict)",
        list(pr.strict)
    );
    if pr.outcomes() < 3 {
        println!("\n  This screen separates two of the three rules; another screen may be needed for the third.");
    }
    println!("\n  This route is for finding the rule, not for the best reward.");
    Ok(())
}
