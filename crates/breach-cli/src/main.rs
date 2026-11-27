mod bench_solver;
mod corpus;
mod eval;
mod gen;
mod imgio;
mod read;

use breach_core::reader::{Contrast, ReadOptions};
use std::path::{Path, PathBuf};

fn usage() -> ! {
    eprintln!(
        "breach <command>

  read <image> [--json] [--buffer N] [--tier] [--matcher substring|retest|strict] [--skip 1,3]
        read a breach screen and print the route
  rule-probe <image> [--buffer N]
        print a route whose result reveals how the game treats a wrong pick mid-sequence
  eval <corpus-dir> [--full] [--loo] [--boxmean] [--no-soft] [--verbose] [--only N.png] [--pert NAME] [--seq-shift X] [--buffer-shift Y]
        score the reader on a labelled corpus (--loo: templates learned without the image under test)
  learn-templates <corpus-dir> [--out <file.rs>]
        learn glyph templates from the labelled corpus and write the embedded template file
  export-rgba <image> <out>   write raw RGBA bytes (prints width height); for testing other front ends
  convert <image> <out> [--height H]   re-encode (format from the extension), optionally resized
  calibrate <corpus-dir>      print measured layout constants in pitch units
  separation <corpus-dir>     print glyph similarity statistics
  bench-solver [--stress]     solver latency over game-like puzzles"
    );
    std::process::exit(2)
}

fn flag(args: &[String], name: &str) -> bool {
    args.iter().any(|a| a == name)
}

fn value(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = || PathBuf::from(args.get(1).cloned().unwrap_or_else(|| usage()));
    let res: Result<(), String> = match args.first().map(String::as_str) {
        Some("bench-solver") => {
            bench_solver::run(flag(&args, "--stress"));
            Ok(())
        }
        Some("eval") => eval::run(
            &dir(),
            &eval::EvalArgs {
                full: flag(&args, "--full"),
                loo: flag(&args, "--loo"),
                contrast: if flag(&args, "--boxmean") {
                    Contrast::BoxMean
                } else {
                    Contrast::TopHat
                },
                verbose: flag(&args, "--verbose"),
                only: value(&args, "--only"),
                only_pert: value(&args, "--pert"),
                soft: !flag(&args, "--no-soft"),
                seq_shift: value(&args, "--seq-shift").and_then(|v| v.parse().ok()).unwrap_or(0.0),
                buffer_shift: value(&args, "--buffer-shift")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0.0),
            },
        ),
        Some("learn-templates") => {
            let items = corpus::load(&dir());
            items.and_then(|items| {
                let all = eval::collect(&items, &ReadOptions::default(), !flag(&args, "--no-soft"));
                let sets = eval::build_sets(&all, None);
                let out = value(&args, "--out")
                    .unwrap_or_else(|| "crates/breach-core/src/templates_data.rs".into());
                for s in &sets {
                    println!(
                        "{:<14} tokens: {}",
                        s.name,
                        s.codes
                            .iter()
                            .enumerate()
                            .filter(|(_, c)| c.is_some())
                            .map(|(k, _)| breach_core::CODES[k])
                            .collect::<Vec<_>>()
                            .join(" ")
                    );
                }
                eval::write_templates(&sets, Path::new(&out))
                    .map(|_| println!("wrote {out} from {} images", all.len()))
            })
        }
        Some("read") => {
            let matcher = value(&args, "--matcher").map(|m| read::parse_matcher(&m)).transpose();
            matcher.and_then(|matcher| {
                read::run(
                    &dir(),
                    &read::ReadArgs {
                        json: flag(&args, "--json"),
                        buffer: value(&args, "--buffer").and_then(|v| v.parse().ok()),
                        tier: flag(&args, "--tier"),
                        matcher,
                        skip: value(&args, "--skip")
                            .map(|v| v.split(',').filter_map(|x| x.trim().parse().ok()).collect())
                            .unwrap_or_default(),
                    },
                )
            })
        }
        Some("export-rgba") => imgio::load(&dir()).and_then(|img| {
            let out = args.get(2).cloned().unwrap_or_else(|| usage());
            let rgba = image::DynamicImage::ImageRgb8(img).to_rgba8();
            println!("{} {}", rgba.width(), rgba.height());
            std::fs::write(out, rgba.as_raw()).map_err(|e| e.to_string())
        }),
        Some("convert") => imgio::load(&dir()).and_then(|img| {
            let out = args.get(2).cloned().unwrap_or_else(|| usage());
            let img = match value(&args, "--height").and_then(|h| h.parse::<u32>().ok()) {
                Some(h) => imgio::rescale(&img, h),
                None => img,
            };
            imgio::save(&img, std::path::Path::new(&out))
        }),
        Some("rule-probe") => {
            read::probe(&dir(), value(&args, "--buffer").and_then(|v| v.parse().ok()))
        }
        Some("calibrate") => eval::calibrate(&dir()),
        Some("separation") => eval::separation(&dir()),
        _ => usage(),
    };
    if let Err(e) = res {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
