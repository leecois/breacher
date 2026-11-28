//! Screen reader: frame to code matrix, daemon sequences and buffer size. Pixel `i` covers `[i, i + 1)`.

mod buffer;
mod debug;
mod lattice;
mod measure;
mod region;
mod segment;
mod sequences;
mod slots;

use crate::clock::Stopwatch;
use crate::glyph::{self, Feat, Gate, GroupLabels, Templates, NCODES, UNKNOWN};
use crate::image::Frame;
use buffer::{read_buffer, Buffer};
use debug::dbg_log;
use lattice::{find_lattice, Lattice, CELL_HH, CELL_HW};
use segment::segment;
use sequences::read_sequences;
use serde::{Deserialize, Serialize};
pub use slots::buffer_codes;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    /// The frame itself is unusable.
    Frame,
    /// No bright UI strokes at all.
    Segment,
    /// No code matrix found (not on the breach screen).
    Lattice,
    /// A matrix was found but cells are missing: the breach has started or something covers it.
    InProgress,
    Sequences,
    Buffer,
    /// Everything was located but at least one glyph failed the confidence gate.
    Confidence,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReadError {
    pub stage: Stage,
    pub message: String,
}

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.stage, self.message)
    }
}

impl std::error::Error for ReadError {}

fn fail<T>(stage: Stage, message: impl Into<String>) -> Result<T, ReadError> {
    Err(ReadError { stage, message: message.into() })
}

/// Screen layout in units of the matrix pitch; the defaults were measured on the labelled captures.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Layout {
    /// Matrix pitch at 1920x1080, in pixels (used only as a scale prior).
    pub pitch_at_1080p: f32,
    /// Sequence column 0 centre = matrix centre x + (n / 2 + this) * px.
    pub seq_col_offset: f32,
    /// Horizontal pitch of sequence codes and of buffer slots.
    pub seq_pitch_x: f32,
    /// Allowed vertical pitch of sequence rows (differs by language: 1.05 English, 0.97 Chinese).
    pub seq_pitch_y: (f32, f32),
    /// Buffer row centre = matrix row 0 centre + this * py.
    pub buffer_row: f32,
    /// Buffer panel: right border = last slot centre + this * px.
    pub buffer_right_pad: f32,
    pub buffer_slot: f32,
    pub glyph_h: f32,
}

impl Default for Layout {
    fn default() -> Self {
        Layout {
            pitch_at_1080p: 64.5,
            seq_col_offset: 3.22,
            seq_pitch_x: 0.651,
            seq_pitch_y: (0.90, 1.15),
            buffer_row: -2.57,
            buffer_right_pad: 0.565,
            buffer_slot: 0.48,
            glyph_h: 0.33,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Contrast {
    /// Morphological white top-hat: exact removal of solid bars.
    TopHat,
    /// Box-mean subtraction: cheaper, leaves bands along bar edges.
    BoxMean,
}

#[derive(Clone, Copy, Debug)]
pub struct ReadOptions {
    pub layout: Layout,
    pub gate: Gate,
    pub contrast: Contrast,
    /// Buffer size to assume when the buffer panel cannot be read; `None` rejects such frames.
    pub buffer_fallback: Option<usize>,
}

impl Default for ReadOptions {
    fn default() -> Self {
        ReadOptions {
            layout: Layout::default(),
            gate: Gate::default(),
            contrast: Contrast::TopHat,
            buffer_fallback: None,
        }
    }
}

/// Where things are in the frame (full-resolution pixels). Enough to draw an overlay.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Geometry {
    /// Centre of matrix cell (0, 0).
    pub x0: f32,
    pub y0: f32,
    pub px: f32,
    pub py: f32,
    pub n: usize,
    /// Centre x of sequence column 0 (also buffer slot 0).
    pub seq_x0: f32,
    pub seq_pitch_x: f32,
    /// Centre y of each sequence row, in the order of `Reading::sequences`.
    pub seq_rows: Vec<f32>,
    pub buffer_y: f32,
}

impl Geometry {
    pub fn cell_center(&self, row: usize, col: usize) -> (f32, f32) {
        (self.x0 + col as f32 * self.px, self.y0 + row as f32 * self.py)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Timings {
    pub segment_us: u32,
    pub lattice_us: u32,
    pub sequences_us: u32,
    pub buffer_us: u32,
    pub classify_us: u32,
    pub total_us: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Quality {
    pub min_consistency: f32,
    pub min_evidence: f32,
    pub min_margin: f32,
    pub min_separation: f32,
    pub matrix_fragments: usize,
    pub matrix_tokens: usize,
    pub buffer_from_panel_top: Option<usize>,
    pub buffer_from_panel_side: Option<usize>,
    pub buffer_from_slots: Option<usize>,
    /// The estimators that produced a value all agree.
    pub buffer_agreed: bool,
    /// Sequence codes whose token does not occur in the matrix; such a daemon cannot be uploaded.
    #[serde(default)]
    pub foreign_codes: usize,
    /// The buffer panel could not be read; `Reading::buffer` is the caller's fallback value.
    #[serde(default)]
    pub buffer_assumed: bool,
    pub scale_mult: f32,
    pub decimation: usize,
    /// The sequence column was not where the layout predicts and was found by a wide search.
    pub layout_fallback: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Reading {
    pub n: usize,
    /// Row-major token ids.
    pub matrix: Vec<u8>,
    pub sequences: Vec<Vec<u8>>,
    pub buffer: usize,
    pub geometry: Geometry,
    pub quality: Quality,
    pub timings: Timings,
}

impl Reading {
    pub fn to_puzzle(&self) -> crate::solver::Puzzle {
        crate::solver::Puzzle::square(
            self.n,
            self.matrix.clone(),
            self.sequences.clone(),
            self.buffer,
        )
    }
}

/// Everything found before glyphs are classified. Template learning starts from this.
#[derive(Clone, Debug)]
pub struct Located {
    pub geometry: Geometry,
    /// Row-major, one per matrix cell.
    pub matrix_feats: Vec<Option<Feat>>,
    pub seq_feats: Vec<Vec<Option<Feat>>>,
    pub buffer: usize,
    pub quality: Quality,
    pub timings: Timings,
}

/// Scale priors tried in order: the interface scales with the frame unless the game letterboxes or renders at another resolution.
const SCALE_RETRY: [f32; 5] = [1.0, 0.75, 4.0 / 3.0, 0.5625, 16.0 / 9.0];

fn us(t: Stopwatch) -> u32 {
    t.us()
}

/// Find the matrix, the sequences and the buffer, and extract glyph features. No classification.
pub fn locate(frame: &Frame, opt: &ReadOptions) -> Result<Located, ReadError> {
    if !frame.is_valid() || frame.width < 320 || frame.height < 200 {
        return fail(Stage::Frame, "frame is empty or too small");
    }
    let t_total = Stopwatch::start();
    let prior = (frame.width as f32 / 1920.0).min(frame.height as f32 / 1080.0);
    let mut furthest: Option<ReadError> = None;
    // report the error of the most credible attempt: largest matrix found, then furthest stage
    let mut best_rank = (0usize, Stage::Frame);
    let mut keep = |e: ReadError, n: usize, furthest: &mut Option<ReadError>| {
        dbg_log!("  -> {e}");
        if furthest.is_none() || (n, e.stage) > best_rank {
            best_rank = (n, e.stage);
            *furthest = Some(e);
        }
    };
    for mult in SCALE_RETRY {
        dbg_log!("attempt scale x{mult:.3}");
        let s = prior * mult;
        let pitch_full = s * opt.layout.pitch_at_1080p;
        if pitch_full < 20.0 {
            continue;
        }
        // layout runs on a max-pooled plane where the pitch is 28 px or a little more
        let d = ((pitch_full / 28.0).floor() as usize).max(1);
        let p0 = pitch_full / d as f32;
        let t = Stopwatch::start();
        // large frames: everything after the layout works on a frame pooled to a pitch of at least 56 px
        let dd = ((pitch_full / 56.0).floor() as usize).max(1);
        // at that scale strokes are five or more pixels wide: every second pixel is enough
        let step = if pitch_full >= 100.0 { 2 } else { 1 };
        let work_plane = (dd > 1).then(|| frame.v_pooled_with(dd, step));
        let v = match &work_plane {
            Some(wp) if d.is_multiple_of(dd) => wp.max_pool(d / dd),
            _ => frame.v_pooled_with(d, step),
        };
        let seg = match segment(&v, p0, opt.contrast) {
            Ok(s) => s,
            Err(e) => {
                keep(e, 0, &mut furthest);
                continue;
            }
        };
        let segment_us = us(t);
        let t = Stopwatch::start();
        let lat_d = match find_lattice(&seg, p0) {
            Ok(l) => l,
            Err(e) => {
                keep(e, 0, &mut furthest);
                continue;
            }
        };
        // lattice in the coordinates of the working frame
        let df = d as f32 / dd as f32;
        let lat = Lattice {
            x0: lat_d.x0 * df,
            y0: lat_d.y0 * df,
            px: lat_d.px * df,
            py: lat_d.py * df,
            n: lat_d.n,
        };
        let work_frame;
        let frame: &Frame = match &work_plane {
            Some(wp) => {
                work_frame = Frame::new(wp.w, wp.h, crate::image::PixelFormat::Gray8, &wp.data);
                &work_frame
            }
            None => frame,
        };
        let up = dd as f32; // working frame -> original frame
        let lattice_us = us(t);
        let t = Stopwatch::start();
        let seqs = match read_sequences(frame, &lat, seg.refv, opt) {
            Ok(s) => s,
            Err(e) => {
                keep(e, lat.n, &mut furthest);
                continue;
            }
        };
        let sequences_us = us(t);
        let t = Stopwatch::start();
        let mut buffer_assumed = false;
        let buf = match read_buffer(frame, &lat, seqs.x0, seqs.q, seg.refv, opt) {
            Ok(b) => b,
            Err(e) => match opt.buffer_fallback {
                // the hard part read; a buffer size is one small number the player can supply
                Some(size) => {
                    dbg_log!("  buffer not read ({e}); assuming {size}");
                    buffer_assumed = true;
                    Buffer {
                        size,
                        from_top: None,
                        from_side: None,
                        from_slots: None,
                        agreed: false,
                        y: lat.y0 + opt.layout.buffer_row * lat.py,
                    }
                }
                None => {
                    keep(e, lat.n, &mut furthest);
                    continue;
                }
            },
        };
        let buffer_us = us(t);
        let mut matrix_feats = Vec::with_capacity(lat.n * lat.n);
        for j in 0..lat.n {
            for i in 0..lat.n {
                let (xc, yc) = (lat.x0 + i as f32 * lat.px, lat.y0 + j as f32 * lat.py);
                let crop = frame.v_crop(
                    (xc - CELL_HW * lat.px).round() as i64,
                    (yc - CELL_HH * lat.py).round() as i64,
                    (xc + CELL_HW * lat.px).round() as i64,
                    (yc + CELL_HH * lat.py).round() as i64,
                );
                matrix_feats.push(glyph::featurize(&crop));
            }
        }
        return Ok(Located {
            geometry: Geometry {
                x0: lat.x0 * up,
                y0: lat.y0 * up,
                px: lat.px * up,
                py: lat.py * up,
                n: lat.n,
                seq_x0: seqs.x0 * up,
                seq_pitch_x: seqs.q * up,
                seq_rows: seqs.rows.iter().map(|y| y * up).collect(),
                buffer_y: buf.y * up,
            },
            matrix_feats,
            seq_feats: seqs.feats,
            buffer: buf.size,
            quality: Quality {
                buffer_from_panel_top: buf.from_top,
                buffer_from_panel_side: buf.from_side,
                buffer_from_slots: buf.from_slots,
                buffer_agreed: buf.agreed,
                buffer_assumed,
                scale_mult: mult,
                decimation: d,
                layout_fallback: seqs.fallback,
                ..Quality::default()
            },
            timings: Timings {
                segment_us,
                lattice_us,
                sequences_us,
                buffer_us,
                classify_us: 0,
                total_us: us(t_total),
            },
        });
    }
    Err(furthest.unwrap_or(ReadError {
        stage: Stage::Frame,
        message: "frame too small for any UI scale".into(),
    }))
}

/// Classify the glyphs of a located screen and apply the confidence gate.
pub fn classify(loc: Located, templates: &Templates, gate: &Gate) -> Result<Reading, ReadError> {
    let t = Stopwatch::start();
    let n = loc.geometry.n;
    let missing = loc.matrix_feats.iter().filter(|f| f.is_none()).count()
        + loc.seq_feats.iter().flatten().filter(|f| f.is_none()).count();
    if missing > 0 {
        return fail(Stage::Confidence, format!("{missing} glyph(s) could not be isolated"));
    }
    let mfeats: Vec<Feat> = loc.matrix_feats.into_iter().flatten().collect();
    let gm: GroupLabels = glyph::label_group(&mfeats, templates, [0.0; NCODES], None);
    let mut in_matrix = [false; NCODES];
    for l in &gm.labels {
        if l.code != UNKNOWN {
            in_matrix[l.code as usize] = true;
        }
    }
    // a slight preference, not a restriction: see `label_group`
    let prior = in_matrix.map(|present| if present { 0.04 } else { 0.0 });
    let lens: Vec<usize> = loc.seq_feats.iter().map(Vec::len).collect();
    let sfeats: Vec<Feat> = loc.seq_feats.into_iter().flatten().flatten().collect();
    let gs = glyph::label_group(&sfeats, templates, prior, Some(&gm.centroids));
    let foreign_codes =
        gs.labels.iter().filter(|l| l.code != UNKNOWN && !in_matrix[l.code as usize]).count();
    let all = gm.labels.iter().chain(gs.labels.iter());
    let bad = all.clone().filter(|l| !l.ok(gate)).count();
    let total = gm.labels.len() + gs.labels.len();
    let fold = |f: fn(&glyph::Label) -> f32| all.clone().map(f).fold(f32::INFINITY, f32::min);
    let quality = Quality {
        min_consistency: fold(|l| l.consistency),
        min_evidence: fold(|l| l.evidence),
        min_margin: fold(|l| l.margin),
        min_separation: fold(|l| l.separation),
        matrix_fragments: gm.fragments,
        matrix_tokens: gm.tokens,
        foreign_codes,
        ..loc.quality
    };
    if bad > 0 {
        let every = || gm.labels.iter().chain(gs.labels.iter());
        let unknown = every().filter(|l| l.code == UNKNOWN).count();
        let conflicts = every().filter(|l| l.conflict).count();
        let halves = every().filter(|l| !l.halves_agree).count();
        return fail(
            Stage::Confidence,
            format!(
                "{bad} of {total} glyphs below the confidence gate (min consistency {:.2}, evidence {:.2}, margin {:.2}, separation {:.2}; {} matrix fragments for {} tokens; {unknown} unassigned, {conflicts} in conflict, {halves} with disagreeing halves)",
                quality.min_consistency, quality.min_evidence, quality.min_margin, quality.min_separation, gm.fragments, gm.tokens
            ),
        );
    }
    let matrix: Vec<u8> = gm.labels.iter().map(|l| l.code).collect();
    let mut sequences = Vec::with_capacity(lens.len());
    let mut it = gs.labels.iter();
    for len in lens {
        sequences.push(it.by_ref().take(len).map(|l| l.code).collect());
    }
    let mut timings = loc.timings;
    timings.classify_us = us(t);
    timings.total_us += timings.classify_us;
    Ok(Reading {
        n,
        matrix,
        sequences,
        buffer: loc.buffer,
        geometry: loc.geometry,
        quality,
        timings,
    })
}

pub fn read(frame: &Frame, templates: &Templates, opt: &ReadOptions) -> Result<Reading, ReadError> {
    classify(locate(frame, opt)?, templates, &opt.gate)
}
