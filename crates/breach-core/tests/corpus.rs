//! Regression tests on the labelled capture corpus.
use breach_core::api::breach_on_screen;
use breach_core::glyph::Templates;
use breach_core::image::{Frame, PixelFormat};
use breach_core::plan::{plan, PlanOptions};
use breach_core::reader::{read, ReadOptions, Reading, Stage};
use breach_core::solver::{self, Matcher};
use image::{imageops, RgbImage};
use std::path::PathBuf;

#[derive(serde::Deserialize)]
struct Label {
    #[serde(default)]
    negative: bool,
    #[serde(default)]
    matrix: Vec<Vec<u8>>,
    #[serde(default)]
    sequences: Vec<Vec<u8>>,
    #[serde(default)]
    buffer: usize,
}

fn corpus() -> Vec<(String, RgbImage, Label)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corpus/cpah");
    let labels: std::collections::BTreeMap<String, Label> =
        serde_json::from_str(&std::fs::read_to_string(dir.join("labels.json")).unwrap()).unwrap();
    labels
        .into_iter()
        .map(|(name, l)| (name.clone(), image::open(dir.join(&name)).unwrap().to_rgb8(), l))
        .collect()
}

fn run(img: &RgbImage) -> Result<Reading, breach_core::reader::ReadError> {
    let frame =
        Frame::new(img.width() as usize, img.height() as usize, PixelFormat::Rgb8, img.as_raw());
    read(&frame, &Templates::builtin(), &ReadOptions::default())
}

fn exact(r: &Reading, l: &Label) -> bool {
    r.matrix == l.matrix.iter().flatten().copied().collect::<Vec<u8>>()
        && r.sequences == l.sequences
        && r.buffer == l.buffer
}

#[test]
fn every_labelled_capture_reads_exactly() {
    for (name, img, label) in corpus() {
        let res = run(&img);
        if label.negative {
            assert!(res.is_err(), "{name}: a non-readable screen was accepted");
            continue;
        }
        let r = res.unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(
            exact(&r, &label),
            "{name}: read {:?} / {:?} / buffer {}",
            r.matrix,
            r.sequences,
            r.buffer
        );
        // geometry is usable for an overlay
        let (x, y) = r.geometry.cell_center(r.n - 1, r.n - 1);
        assert!(
            x > 0.0 && y > 0.0 && (x as u32) < img.width() && (y as u32) < img.height(),
            "{name}"
        );
        // and the plan is a legal route under every rule
        let p = plan(&r, &PlanOptions::default());
        assert!(p.proven, "{name}");
        let path: Vec<u16> = p.route.steps.iter().map(|s| (s.row * r.n + s.col) as u16).collect();
        let puzzle = r.to_puzzle();
        for m in [Matcher::Substring, Matcher::Retest, Matcher::Strict] {
            let rp = solver::verify(&puzzle, &path, m).unwrap_or_else(|e| panic!("{name}: {e}"));
            for d in &p.route.uploaded {
                assert!(rp.uploaded >> d & 1 == 1, "{name}: daemon {d} not uploaded under {m:?}");
            }
        }
    }
}

#[test]
fn in_progress_and_foreign_screens_are_rejected_for_the_right_reason() {
    for (name, img, label) in corpus() {
        if !label.negative {
            continue;
        }
        let e = run(&img).unwrap_err();
        match name.as_str() {
            // breach already started, cursor over a cell: located, but a glyph fails the gate
            "2.png" => {
                assert!(matches!(e.stage, Stage::Confidence | Stage::InProgress), "{name}: {e}")
            }
            // not the breach screen at all
            "6.png" => assert!(matches!(e.stage, Stage::Lattice | Stage::Segment), "{name}: {e}"),
            _ => {}
        }
    }
}

fn seen(img: &RgbImage) -> bool {
    let frame =
        Frame::new(img.width() as usize, img.height() as usize, PixelFormat::Rgb8, img.as_raw());
    breach_on_screen(&frame)
}

#[test]
fn a_breach_screen_is_seen_whatever_its_state() {
    for (name, img, _) in corpus() {
        // 6.png is the one capture that shows something else
        assert_eq!(seen(&img), name != "6.png", "{name}");
    }
    let flat = RgbImage::from_pixel(1920, 1080, image::Rgb([12, 14, 20]));
    assert!(!seen(&flat), "a flat frame");
    let mut state = 7u32;
    let noise = RgbImage::from_fn(1280, 720, |_, _| {
        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        image::Rgb([(state >> 24) as u8, (state >> 16) as u8, (state >> 8) as u8])
    });
    assert!(!seen(&noise), "noise");
}

/// A route being played empties its cells one by one; the screen must stay recognisable until the last pick.
#[test]
fn a_breach_being_played_is_still_seen() {
    for (name, mut img, label) in corpus() {
        if label.negative {
            continue;
        }
        let reading = run(&img).unwrap();
        let g = &reading.geometry;
        let route = plan(&reading, &PlanOptions::default()).route;
        assert!(!route.steps.is_empty(), "{name}: no route to play");
        for (k, step) in route.steps.iter().enumerate() {
            let (cx, cy) = g.cell_center(step.row, step.col);
            let ground = *img.get_pixel((cx + g.px / 2.0) as u32, (cy + g.py / 2.0) as u32);
            let (hw, hh) = (g.px * 0.42, g.py * 0.4);
            for y in (cy - hh) as u32..=(cy + hh) as u32 {
                for x in (cx - hw) as u32..=(cx + hw) as u32 {
                    img.put_pixel(x, y, ground);
                }
            }
            assert!(seen(&img), "{name}: lost after pick {}", k + 1);
        }
    }
}

/// Degraded copies must read exactly or be rejected; a wrong answer is never acceptable.
#[test]
fn degraded_copies_never_read_wrong() {
    let lut = |g: f32| -> Vec<u8> {
        (0..256).map(|v| (255.0 * (v as f32 / 255.0).powf(g) + 0.5) as u8).collect()
    };
    let tone = |img: &RgbImage, g: f32| {
        let t = lut(g);
        let mut o = img.clone();
        o.pixels_mut().for_each(|p| p.0.iter_mut().for_each(|c| *c = t[*c as usize]));
        o
    };
    let (mut ok, mut rejected) = (0, 0);
    for (name, img, label) in corpus() {
        if label.negative {
            continue;
        }
        let variants: Vec<(&str, RgbImage)> = vec![
            (
                "720p",
                imageops::resize(
                    &img,
                    img.width() * 720 / img.height(),
                    720,
                    imageops::FilterType::Triangle,
                ),
            ),
            (
                "1440p",
                imageops::resize(
                    &img,
                    img.width() * 1440 / img.height(),
                    1440,
                    imageops::FilterType::CatmullRom,
                ),
            ),
            ("gamma 0.6", tone(&img, 0.6)),
            ("gamma 1.8", tone(&img, 1.8)),
            ("blur", imageops::blur(&img, 1.0)),
        ];
        for (vname, v) in variants {
            match run(&v) {
                Ok(r) => {
                    assert!(
                        exact(&r, &label),
                        "{name} ({vname}): silent wrong read {:?} / {:?} / {}",
                        r.matrix,
                        r.sequences,
                        r.buffer
                    );
                    ok += 1;
                }
                Err(_) => rejected += 1,
            }
        }
    }
    assert!(ok >= 40, "only {ok} of 45 degraded copies read ({rejected} rejected)");
}
