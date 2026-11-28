//! Screens the corpus does not contain, composed from real glyph crops along the reader's own layout model.
use breach_core::glyph::Templates;
use breach_core::image::{Frame, PixelFormat};
use breach_core::reader::{read, Layout, ReadOptions, Reading};
use image::{imageops, Rgb, RgbImage};
use std::path::PathBuf;

fn open(name: &str) -> RgbImage {
    image::open(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corpus/cpah").join(name))
        .unwrap()
        .to_rgb8()
}

fn run(img: &RgbImage) -> Result<Reading, breach_core::reader::ReadError> {
    read(
        &Frame::new(img.width() as usize, img.height() as usize, PixelFormat::Rgb8, img.as_raw()),
        &Templates::builtin(),
        &ReadOptions::default(),
    )
}

struct Rng(u64);
impl Rng {
    fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % n as u64) as usize
    }
}

/// Real glyph bitmaps, one per token: matrix size and sequence size.
struct Glyphs {
    matrix: Vec<RgbImage>,
    seq: Vec<Option<RgbImage>>,
    pitch: f32,
}

fn harvest() -> Glyphs {
    // 1.png: 7x7 with all six tokens. 5.png: five daemons covering tokens 0-4 in the sequence font.
    let (a, b) = (open("1.png"), open("5.png"));
    let (ra, rb) = (run(&a).unwrap(), run(&b).unwrap());
    let ga = &ra.geometry;
    let crop = |img: &RgbImage, cx: f32, cy: f32, hw: f32, hh: f32| {
        imageops::crop_imm(
            img,
            (cx - hw).round() as u32,
            (cy - hh).round() as u32,
            (2.0 * hw).round() as u32,
            (2.0 * hh).round() as u32,
        )
        .to_image()
    };
    let mut matrix: Vec<Option<RgbImage>> = vec![None; 6];
    for row in 1..ra.n {
        // row 0 is highlighted in the capture; take glyphs from plain rows
        for col in 0..ra.n {
            let t = ra.matrix[row * ra.n + col] as usize;
            if matrix[t].is_none() {
                let (x, y) = ga.cell_center(row, col);
                matrix[t] = Some(crop(&a, x, y, 0.40 * ga.px, 0.30 * ga.py));
            }
        }
    }
    let gb = &rb.geometry;
    let mut seq: Vec<Option<RgbImage>> = vec![None; 6];
    for (r, s) in rb.sequences.iter().enumerate() {
        for (k, &t) in s.iter().enumerate().skip(1) {
            // column 0 sits on the selection strip; take glyphs from the other columns
            if seq[t as usize].is_none() {
                seq[t as usize] = Some(crop(
                    &b,
                    gb.seq_x0 + k as f32 * gb.seq_pitch_x,
                    gb.seq_rows[r],
                    0.48 * gb.seq_pitch_x,
                    0.2 * gb.py,
                ));
            }
        }
    }
    Glyphs { matrix: matrix.into_iter().map(Option::unwrap).collect(), seq, pitch: ga.px }
}

struct Screen {
    n: usize,
    matrix: Vec<u8>,
    sequences: Vec<Vec<u8>>,
    buffer: usize,
    /// Sequence panel moved down by this many pitches from where captures put it.
    seq_dy: f32,
    /// Sequence and buffer panels moved right by this many pitches.
    seq_dx: f32,
}

fn compose(g: &Glyphs, s: &Screen) -> RgbImage {
    let l = Layout::default();
    let p = g.pitch;
    let q = l.seq_pitch_x * p;
    let mut img = RgbImage::from_pixel(1920, 1080, Rgb([18, 18, 28]));
    let line = Rgb([208, 236, 88]);
    let paste = |img: &mut RgbImage, glyph: &RgbImage, cx: f32, cy: f32| {
        imageops::replace(
            img,
            glyph,
            (cx - glyph.width() as f32 / 2.0).round() as i64,
            (cy - glyph.height() as f32 / 2.0).round() as i64,
        );
    };
    let hline = |img: &mut RgbImage, x0: f32, x1: f32, y: f32| {
        for x in x0.round() as u32..x1.round() as u32 {
            for t in 0..2 {
                img.put_pixel(x, y.round() as u32 + t, line);
            }
        }
    };
    let vline = |img: &mut RgbImage, x: f32, y0: f32, y1: f32| {
        for y in y0.round() as u32..y1.round() as u32 {
            for t in 0..2 {
                img.put_pixel(x.round() as u32 + t, y, line);
            }
        }
    };
    // matrix: centred on x = 490.5 with its first row at y = 377, as in 1080p captures
    let n = s.n as f32;
    let (xc, y0) = (490.5f32, 377.0f32);
    let x0 = xc - (n - 1.0) / 2.0 * p;
    for r in 0..s.n {
        for c in 0..s.n {
            paste(
                &mut img,
                &g.matrix[s.matrix[r * s.n + c] as usize],
                x0 + c as f32 * p,
                y0 + r as f32 * p,
            );
        }
    }
    vline(&mut img, x0 + (n - 0.5) * p + 0.35 * p, y0 - 0.6 * p, y0 + (n - 0.4) * p); // matrix panel border
                                                                                      // sequences: a header line of text, a separator rule, the rows, the panel's bottom border
    let sx = xc + (n / 2.0 + l.seq_col_offset + s.seq_dx) * p;
    let first = y0 - 0.32 * p + s.seq_dy * p;
    for (r, seq) in s.sequences.iter().enumerate() {
        let y = first + r as f32 * 1.055 * p;
        for (k, &t) in seq.iter().enumerate() {
            paste(&mut img, g.seq[t as usize].as_ref().unwrap(), sx + k as f32 * q, y);
        }
    }
    for k in 0..7 {
        // stands in for the panel title: glyph-sized text above the separator
        paste(
            &mut img,
            g.seq[(k % 5) as usize].as_ref().unwrap(),
            sx + 0.3 * q + k as f32 * 0.8 * q,
            first - 0.72 * p,
        );
    }
    hline(&mut img, sx - 0.56 * p, sx + 13.0 * q, first - 0.37 * p);
    let rows = s.sequences.len() as f32;
    let panel_bottom = first + (rows - 1.0) * 1.055 * p + 0.75 * p;
    vline(&mut img, sx - 0.56 * p, first - 1.0 * p, panel_bottom);
    hline(&mut img, sx - 0.56 * p, sx + 13.0 * q, panel_bottom);
    // buffer panel: a box holding `buffer` slots
    let (top, bottom) = (y0 - 3.13 * p, y0 - 2.01 * p);
    let (left, right) = (sx - 0.56 * p, sx + (s.buffer as f32 - 1.0) * q + l.buffer_right_pad * p);
    hline(&mut img, left, right, top);
    hline(&mut img, left, right, bottom);
    vline(&mut img, left, top, bottom);
    vline(&mut img, right, top, bottom + 2.0);
    img
}

fn random_screen(rng: &mut Rng, n: usize, buffer: usize, daemons: usize) -> Screen {
    let tokens = if n <= 6 { 5 } else { 6 };
    let matrix: Vec<u8> = (0..n * n).map(|_| rng.below(tokens) as u8).collect();
    // sequences use codes the matrix contains; sequence-size samples exist for tokens 0-4 only
    let pool: Vec<u8> = (0..5u8).filter(|t| matrix.contains(t)).collect();
    let sequences = (0..daemons)
        .map(|_| (0..2 + rng.below(4)).map(|_| pool[rng.below(pool.len())]).collect())
        .collect();
    Screen { n, matrix, sequences, buffer, seq_dy: 0.0, seq_dx: 0.0 }
}

fn check(img: &RgbImage, s: &Screen, what: &str) {
    let r = run(img).unwrap_or_else(|e| panic!("{what}: {e}"));
    assert_eq!(r.n, s.n, "{what}: matrix size");
    assert_eq!(r.matrix, s.matrix, "{what}: matrix");
    assert_eq!(r.sequences, s.sequences, "{what}: sequences");
    assert_eq!(r.buffer, s.buffer, "{what}: buffer");
}

#[test]
fn matrix_sizes_buffers_and_daemon_counts_beyond_the_corpus() {
    let g = harvest();
    let mut rng = Rng(0x5EED_1234_ABCD_0001);
    let mut count = 0;
    for n in [5usize, 6, 7, 8] {
        for buffer in [4usize, 5, 6, 7, 8, 9, 10, 12] {
            let daemons = 1 + rng.below(7);
            let s = random_screen(&mut rng, n, buffer, daemons);
            check(&compose(&g, &s), &s, &format!("{n}x{n}, buffer {buffer}, {daemons} daemons"));
            count += 1;
        }
    }
    assert_eq!(count, 32);
}

#[test]
fn composed_screens_at_other_resolutions() {
    let g = harvest();
    let mut rng = Rng(0x0BAD_CAFE_5EED_0002);
    for (n, buffer, daemons) in [(5usize, 4usize, 3usize), (5, 7, 2), (8, 10, 5), (6, 5, 1)] {
        let s = random_screen(&mut rng, n, buffer, daemons);
        let base = compose(&g, &s);
        for (w, h) in [(1280u32, 720u32), (2560, 1440), (3840, 2160)] {
            let filter = if h < 1080 {
                imageops::FilterType::Triangle
            } else {
                imageops::FilterType::CatmullRom
            };
            check(
                &imageops::resize(&base, w, h, filter),
                &s,
                &format!("{n}x{n}, buffer {buffer}, {daemons} daemons at {w}x{h}"),
            );
        }
    }
}

#[test]
fn a_composed_screen_in_a_wider_frame_reads_the_same() {
    let g = harvest();
    let mut rng = Rng(0x7777_0000_1111_0003);
    let s = random_screen(&mut rng, 5, 6, 3);
    let base = compose(&g, &s);
    // 32:9: the 16:9 screen centred, dark filler either side
    let mut wide = RgbImage::from_pixel(3840, 1080, Rgb([10, 10, 14]));
    imageops::replace(&mut wide, &base, 960, 0);
    check(&wide, &s, "5x5 centred in 3840x1080");
}

/// A sequence code whose token is not in the matrix is read as what it is and reported, never relabelled.
#[test]
fn a_code_missing_from_the_matrix_is_read_and_reported() {
    let g = harvest();
    // matrix without BD (3); one sequence needs it
    let matrix: Vec<u8> = (0..36).map(|i| [0u8, 1, 2, 4][(i * 5 + i / 6) % 4]).collect();
    let s = Screen {
        n: 6,
        matrix,
        sequences: vec![vec![0, 4, 4, 2, 3], vec![4, 1, 1], vec![2, 3, 0]],
        buffer: 6,
        seq_dy: 0.0,
        seq_dx: 0.0,
    };
    let r = run(&compose(&g, &s)).unwrap();
    assert_eq!(r.sequences, s.sequences);
    assert_eq!(r.quality.foreign_codes, 2);
    // the plan simply cannot upload those daemons
    let p = breach_core::plan::plan(&r, &breach_core::plan::PlanOptions::default());
    assert!(!p.route.uploaded.contains(&0) && !p.route.uploaded.contains(&2));
}

/// The list a fixed way from the matrix's first column, as the game has drawn it since the matrix grows from the left, is as usual as the list that follows the matrix's width.
#[test]
fn both_known_arrangements_of_the_panels_are_usual() {
    let g = harvest();
    let mut rng = Rng(0x1357_9BDF_2468_0005);
    let from_first = Layout::default().seq_from_first;
    for n in [5usize, 6, 7] {
        // where the list sits when it keeps its distance from column 0, measured from where it sits when it follows the width
        let fixed =
            from_first - (n as f32 - 1.0) / 2.0 - n as f32 / 2.0 - Layout::default().seq_col_offset;
        for (dx, usual) in [(0.0, true), (fixed, true), (fixed + 0.12, true), (2.4, false)] {
            let mut s = random_screen(&mut rng, n, 7, 3);
            s.seq_dx = dx;
            let what = format!("{n}x{n} with the panels moved {dx:+.2}");
            let img = compose(&g, &s);
            check(&img, &s, &what);
            assert_eq!(run(&img).unwrap().quality.layout_fallback, !usual, "{what}");
        }
    }
}

/// A sequence panel higher or lower than in the captures gives an exact read or a refusal, never a wrong list.
#[test]
fn sequence_panel_moved_vertically_never_reads_wrong() {
    let g = harvest();
    let mut rng = Rng(0x1357_9BDF_2468_0004);
    let mut read_ok = Vec::new();
    for dy in [-1.2f32, -0.8, -0.5, -0.3, -0.15, 0.0, 0.15, 0.3, 0.5, 0.8, 1.2, 2.0] {
        let mut s = random_screen(&mut rng, 6, 7, 3);
        s.seq_dy = dy;
        match run(&compose(&g, &s)) {
            Ok(r) => {
                assert_eq!(r.matrix, s.matrix, "dy {dy}: matrix");
                assert_eq!(
                    r.sequences, s.sequences,
                    "dy {dy}: sequences read wrong instead of refused"
                );
                read_ok.push(dy);
            }
            Err(e) => eprintln!("dy {dy:+.2}: refused ({e})"),
        }
    }
    eprintln!("read at dy = {read_ok:?}");
    assert!(read_ok.contains(&0.0));
}
