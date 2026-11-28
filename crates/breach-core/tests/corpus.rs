//! Regression tests on the labelled capture corpus.
use breach_core::api::breach_on_screen;
use breach_core::follow::{Followed, Sight, Step};
use breach_core::glyph::Templates;
use breach_core::image::{Frame, PixelFormat};
use breach_core::plan::{plan, PlanOptions, Priority};
use breach_core::progress::{advance, look, Fate, Mark};
use breach_core::reader::Geometry;
use breach_core::reader::{buffer_codes, read, ReadOptions, Reading, Stage};
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

const CYAN: image::Rgb<u8> = image::Rgb([85, 220, 232]);
const CODE: image::Rgb<u8> = image::Rgb([215, 245, 90]);
const GREEN: image::Rgb<u8> = image::Rgb([30, 215, 120]);
const RED: image::Rgb<u8> = image::Rgb([240, 70, 75]);

fn fill(img: &mut RgbImage, cx: f32, cy: f32, hw: f32, hh: f32, colour: image::Rgb<u8>) {
    for y in (cy - hh) as u32..=(cy + hh) as u32 {
        for x in (cx - hw) as u32..=(cx + hw) as u32 {
            img.put_pixel(x, y, colour);
        }
    }
}

/// Paints a matrix cell the way the game shows it once the pointer or a pick has been there.
fn mark_cell(img: &mut RgbImage, g: &Geometry, cell: usize, mark: Mark) {
    let (cx, cy) = g.cell_center(cell / g.n, cell % g.n);
    let ground = *img.get_pixel((cx + g.px / 2.0) as u32, (cy + g.py / 2.0) as u32);
    match mark {
        Mark::Code => {}
        Mark::Used => fill(img, cx, cy, g.px * 0.42, g.py * 0.4, ground),
        Mark::Flash => fill(img, cx, cy, g.px * 0.46, g.py * 0.46, CYAN),
        Mark::Hover => {
            // the code turns cyan and a box is drawn round it
            for y in (cy - g.py * 0.4) as u32..=(cy + g.py * 0.4) as u32 {
                for x in (cx - g.px * 0.4) as u32..=(cx + g.px * 0.4) as u32 {
                    let p = img.get_pixel(x, y).0;
                    if p[0] > 120 && p[0] as u32 * 4 > p[2] as u32 * 5 {
                        img.put_pixel(x, y, CYAN);
                    }
                }
            }
            for t in 0..3 {
                let (hw, hh) = (g.px * 0.47 - t as f32, g.py * 0.47 - t as f32);
                fill(img, cx, cy - hh, hw, 0.0, CYAN);
                fill(img, cx, cy + hh, hw, 0.0, CYAN);
                fill(img, cx - hw, cy, 0.0, hh, CYAN);
                fill(img, cx + hw, cy, 0.0, hh, CYAN);
            }
        }
    }
}

/// Paints a buffer slot as filled for good, as flashing, or as the preview of a code under the pointer.
fn mark_slot(img: &mut RgbImage, g: &Geometry, slot: usize, mark: Mark) {
    let (cx, cy, q) = (g.seq_x0 + slot as f32 * g.seq_pitch_x, g.buffer_y, g.seq_pitch_x);
    match mark {
        Mark::Code => {}
        Mark::Used => fill(img, cx, cy, q * 0.22, q * 0.22, CODE),
        Mark::Flash => fill(img, cx, cy, q * 0.45, q * 0.45, CYAN),
        Mark::Hover => fill(img, cx, cy, q * 0.16, q * 0.16, CYAN),
    }
}

fn looked(img: &RgbImage, reading: &Reading) -> breach_core::progress::Look {
    let frame =
        Frame::new(img.width() as usize, img.height() as usize, PixelFormat::Rgb8, img.as_raw());
    look(&frame, &reading.geometry, reading.buffer)
}

#[test]
fn an_untouched_screen_shows_no_pick() {
    for (name, img, label) in corpus() {
        if label.negative {
            continue;
        }
        let reading = run(&img).unwrap();
        let seen = looked(&img, &reading);
        assert_eq!(seen.committed, 0, "{name}");
        assert!(seen.cells.iter().all(|m| *m == Mark::Code), "{name}: {:?}", seen.cells);
        assert!(seen.fates.iter().all(|f| *f == Fate::Pending), "{name}: {:?}", seen.fates);
        assert_eq!(seen.fates.len(), reading.sequences.len(), "{name}");
        assert!(seen.matrix_shown(reading.buffer) && !seen.over(), "{name}");
    }
}

/// The game lays a green bar over a daemon it has installed and a red one over a daemon that has failed.
#[test]
fn the_bars_over_settled_daemons_are_read() {
    for (name, img, label) in corpus() {
        if label.negative {
            continue;
        }
        let reading = run(&img).unwrap();
        let g = &reading.geometry;
        let mut settled = img.clone();
        let mut want = Vec::new();
        for (i, &row) in g.seq_rows.iter().enumerate() {
            let colour = [None, Some(GREEN), Some(RED)][i % 3];
            want.push([Fate::Pending, Fate::Installed, Fate::Failed][i % 3]);
            if let Some(colour) = colour {
                let width = 9.0 * g.seq_pitch_x;
                fill(
                    &mut settled,
                    g.seq_x0 + width / 2.0 - g.seq_pitch_x,
                    row,
                    width / 2.0,
                    g.py * 0.4,
                    colour,
                );
            }
        }
        let seen = looked(&settled, &reading);
        assert_eq!(seen.fates, want, "{name}");
        assert_eq!(seen.over(), !want.contains(&Fate::Pending), "{name}");
        // with the matrix replaced by the screen that follows a breach, nothing of it is taken for picks
        let (cx, cy) = g.cell_center(g.n / 2, g.n / 2);
        let half = g.n as f32 / 2.0;
        fill(&mut settled, cx, cy, half * g.px, half * g.py, image::Rgb([8, 10, 18]));
        assert!(!looked(&settled, &reading).matrix_shown(reading.buffer), "{name}");
    }
}

/// A route is played on a copy of each capture, painted the way the game shows it, and followed pick by pick.
#[test]
fn a_played_route_is_followed_pick_by_pick() {
    for (name, img, label) in corpus() {
        if label.negative {
            continue;
        }
        let reading = run(&img).unwrap();
        let (g, n) = (&reading.geometry, reading.n);
        let route: Vec<usize> = plan(&reading, &PlanOptions::default())
            .route
            .steps
            .iter()
            .map(|s| s.row * n + s.col)
            .collect();
        let mut played: Vec<usize> = Vec::new();
        for k in 0..route.len() {
            // the pointer rests on the next cell: its code is previewed in the buffer, and nothing is picked
            let mut hovering = img.clone();
            for (i, &cell) in route[..k].iter().enumerate() {
                mark_cell(&mut hovering, g, cell, Mark::Used);
                mark_slot(&mut hovering, g, i, Mark::Used);
            }
            mark_cell(&mut hovering, g, route[k], Mark::Hover);
            mark_slot(&mut hovering, g, k, Mark::Hover);
            let seen = looked(&hovering, &reading);
            assert_eq!(seen.committed, k, "{name}: hovering before pick {}", k + 1);
            assert_eq!(advance(n, &played, &seen), Some(played.clone()), "{name}: hover {}", k + 1);
            // the pick flashes in the matrix and in the buffer
            let mut picked = img.clone();
            for (i, &cell) in route[..k].iter().enumerate() {
                mark_cell(&mut picked, g, cell, Mark::Used);
                mark_slot(&mut picked, g, i, Mark::Used);
            }
            mark_cell(&mut picked, g, route[k], Mark::Flash);
            mark_slot(&mut picked, g, k, Mark::Flash);
            let seen = looked(&picked, &reading);
            assert_eq!(seen.committed, k + 1, "{name}: pick {}", k + 1);
            played = advance(n, &played, &seen).unwrap_or_else(|| panic!("{name}: pick {}", k + 1));
            assert_eq!(played, route[..=k], "{name}: pick {}", k + 1);
        }
    }
}

/// Puts the code of one sequence position into a buffer slot, in a box as the game draws one.
fn play_into_slot(img: &mut RgbImage, g: &Geometry, row: f32, from: usize, slot: usize) {
    let q = g.seq_pitch_x;
    let (hw, hh) = ((0.34 * q) as i64, (0.28 * q) as i64);
    let (sx, sy) = ((g.seq_x0 + from as f32 * q) as i64, row as i64);
    let (dx, dy) = ((g.seq_x0 + slot as f32 * q) as i64, g.buffer_y as i64);
    let source = img.clone();
    let ground = *source.get_pixel((dx - (0.5 * q) as i64) as u32, dy as u32);
    fill(img, dx as f32, dy as f32, 0.44 * q, 0.44 * q, ground);
    for y in -hh..=hh {
        for x in -hw..=hw {
            img.put_pixel(
                (dx + x) as u32,
                (dy + y) as u32,
                *source.get_pixel((sx + x) as u32, (sy + y) as u32),
            );
        }
    }
    let side = 0.4 * q;
    for t in 0..2 {
        let s = side - t as f32;
        fill(img, dx as f32, dy as f32 - s, s, 0.0, CODE);
        fill(img, dx as f32, dy as f32 + s, s, 0.0, CODE);
        fill(img, dx as f32 - s, dy as f32, 0.0, s, CODE);
        fill(img, dx as f32 + s, dy as f32, 0.0, s, CODE);
    }
}

/// The codes standing in the buffer are read back, and a buffer that cannot be read is not guessed at.
#[test]
fn the_codes_in_the_buffer_are_read_back() {
    let templates = Templates::builtin();
    let codes_in = |img: &RgbImage, reading: &Reading| {
        let (w, h) = (img.width() as usize, img.height() as usize);
        let frame = Frame::new(w, h, PixelFormat::Rgb8, img.as_raw());
        buffer_codes(&frame, reading, &templates, &breach_core::glyph::Gate::default())
    };
    let mut read_back = 0;
    for (name, img, label) in corpus() {
        if label.negative {
            continue;
        }
        let reading = run(&img).unwrap();
        let g = &reading.geometry;
        assert_eq!(codes_in(&img, &reading), Some(vec![]), "{name}: an untouched buffer is empty");
        // the first sequences are played into the buffer one after the other, as far as it has room
        let mut played = img.clone();
        let mut want: Vec<u8> = Vec::new();
        'fill: for (i, seq) in reading.sequences.iter().enumerate() {
            for (j, &code) in seq.iter().enumerate() {
                if want.len() == reading.buffer {
                    break 'fill;
                }
                play_into_slot(&mut played, g, g.seq_rows[i], j, want.len());
                want.push(code);
            }
        }
        assert_eq!(codes_in(&played, &reading), Some(want.clone()), "{name}");
        read_back += want.len();
        // a slot that flashes is neither a code nor empty: nothing is read
        let mut flashing = played.clone();
        mark_slot(&mut flashing, g, want.len() - 1, Mark::Flash);
        assert_eq!(codes_in(&flashing, &reading), None, "{name}: a flashing slot");
        // nor is a code that stands after an empty slot
        if want.len() + 1 < reading.buffer {
            let mut gap = played.clone();
            play_into_slot(&mut gap, g, g.seq_rows[0], 0, want.len() + 1);
            assert_eq!(codes_in(&gap, &reading), None, "{name}: a code after a gap");
        }
    }
    assert!(read_back > 30, "{read_back}");
}

/// A breach is followed from frames alone: a pick on the route, one off it, and the bars that end it.
#[test]
fn a_breach_is_followed_from_its_frames_to_its_end() {
    let see = |followed: &mut Followed, img: &RgbImage| {
        let (w, h) = (img.width() as usize, img.height() as usize);
        followed.look(&Frame::new(w, h, PixelFormat::Rgb8, img.as_raw()), Priority::Count, None)
    };
    for (name, img, label) in corpus() {
        if label.negative {
            continue;
        }
        let reading = run(&img).unwrap();
        let (g, n) = (reading.geometry.clone(), reading.n);
        let route = plan(&reading, &PlanOptions::default()).route;
        if route.steps.len() < 2 {
            continue;
        }
        let mut followed = Followed::new(reading.clone(), route.clone(), false);
        let untouched = see(&mut followed, &img);
        assert_eq!((untouched.sight, untouched.step), (Sight::Breach, None), "{name}");

        let first = route.steps[0].row * n + route.steps[0].col;
        let mut one = img.clone();
        mark_cell(&mut one, &g, first, Mark::Flash);
        mark_slot(&mut one, &g, 0, Mark::Flash);
        let seen = see(&mut followed, &one);
        assert_eq!((seen.sight, seen.step), (Sight::Breach, Some(Step::Onward)), "{name}");
        assert_eq!(followed.route.done, 1, "{name}");

        // the second pick goes to another cell of the same column than the route asked for
        let next = route.steps[1].row * n + route.steps[1].col;
        let wrong = (0..n).map(|r| r * n + first % n).find(|c| *c != first && *c != next).unwrap();
        let mut two = img.clone();
        mark_cell(&mut two, &g, first, Mark::Used);
        mark_slot(&mut two, &g, 0, Mark::Used);
        mark_cell(&mut two, &g, wrong, Mark::Flash);
        mark_slot(&mut two, &g, 1, Mark::Flash);
        let seen = see(&mut followed, &two);
        assert_eq!(seen.step, Some(Step::Replanned), "{name}");
        assert_eq!(followed.played, vec![first, wrong], "{name}");
        let drawn: Vec<usize> = followed.route.steps.iter().map(|s| s.row * n + s.col).collect();
        assert_eq!((&drawn[..2], followed.route.done), (&[first, wrong][..], 2), "{name}");

        // the matrix gives way to another panel: for now that is neither a breach nor its end
        let (cx, cy) = g.cell_center(n / 2, n / 2);
        let half = n as f32 / 2.0;
        let mut gone = two.clone();
        fill(&mut gone, cx, cy, half * g.px, half * g.py, image::Rgb([8, 10, 18]));
        let seen = see(&mut followed, &gone);
        assert_eq!(
            (seen.sight, seen.step, followed.played.len()),
            (Sight::Other, None, 2),
            "{name}"
        );

        // the game lays a bar over every daemon, and the breach is over
        let mut want = Vec::new();
        for (i, &row) in g.seq_rows.iter().enumerate() {
            let installed = i % 2 == 0;
            want.push((i + 1, if installed { Fate::Installed } else { Fate::Failed }));
            let width = 9.0 * g.seq_pitch_x;
            let colour = if installed { GREEN } else { RED };
            fill(
                &mut gone,
                g.seq_x0 + width / 2.0 - g.seq_pitch_x,
                row,
                width / 2.0,
                g.py * 0.4,
                colour,
            );
        }
        let seen = see(&mut followed, &gone);
        assert_eq!((seen.sight, &seen.settled), (Sight::Over, &want), "{name}");
        let (installed, failed) = followed.outcome().unwrap();
        assert_eq!(installed.len() + failed.len(), reading.sequences.len(), "{name}");
        // the same frame again is no news
        assert!(see(&mut followed, &gone).settled.is_empty());
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
