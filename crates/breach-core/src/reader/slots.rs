//! The codes standing in the buffer of a screen read earlier: what was played, once a breach is under way or over.

use super::region::remove_lines;
use super::Reading;
use crate::glyph::{self, Feat, Gate, Templates, NCODES};
use crate::image::{self, Frame, Plane};

/// What one buffer slot shows.
enum Slot {
    /// A code, cut free of the box drawn round it.
    Code(Feat),
    Empty,
    /// Something that is neither: a slot that flashes, or one the frame does not cover.
    Unclear,
}

fn slot(frame: &Frame, cx: f32, cy: f32, q: f32) -> Slot {
    let half = 0.55 * q;
    let (x0, y0) = ((cx - half).round() as i64, (cy - half).round() as i64);
    let v = frame.v_crop(x0, y0, (cx + half).round() as i64, (cy + half).round() as i64);
    if v.w < 8 || v.h < 8 {
        return Slot::Unclear;
    }
    // an empty slot is one flat tone in its middle, well inside its box wherever that sits within the window
    let inner = (0.22 * q) as i64;
    let (mx, my) = (v.w as i64 / 2, v.h as i64 / 2);
    let (low, high) = v.crop(mx - inner, my - inner, mx + inner, my + inner).min_max();
    if high < low.saturating_add(24) {
        // flat and as dark as the panel is empty; flat and lit is a slot that flashes
        let dark = low <= v.percentile(0.05).saturating_add(24);
        return if dark { Slot::Empty } else { Slot::Unclear };
    }
    match glyph_in(&v, q) {
        Some(feat) => Slot::Code(feat),
        None => Slot::Unclear,
    }
}

fn glyph_in(v: &Plane, q: f32) -> Option<Feat> {
    let (ground, top) = (v.percentile(0.2) as f32, v.percentile(0.995) as f32);
    if top < ground + 24.0 {
        return None;
    }
    let mut mask: Plane = image::threshold(v, (ground + 0.5 * (top - ground)) as u8);
    // the box is made of runs longer than any stroke of a code
    remove_lines(&mut mask, (0.5 * q) as usize, (0.55 * q) as usize);
    let (mut bx0, mut by0, mut bx1, mut by1) = (usize::MAX, usize::MAX, 0usize, 0usize);
    let (mx, my) = (v.w as f32 / 2.0, v.h as f32 / 2.0);
    for b in image::components(&mask, 0) {
        // strokes of the code lie about the middle of the slot; a stroke may be broken where the picture is soft, so pieces count wherever they are whole specks
        let (bcx, bcy) = (b.cx(), b.cy());
        if b.area >= 3 && (bcx - mx).abs() <= 0.36 * q && (bcy - my).abs() <= 0.36 * q {
            (bx0, by0) = (bx0.min(b.x0), by0.min(b.y0));
            (bx1, by1) = (bx1.max(b.x1), by1.max(b.y1));
        }
    }
    if bx1 <= bx0 || by1 <= by0 {
        return None;
    }
    let (wide, tall) = ((bx1 - bx0) as f32, (by1 - by0) as f32);
    // a code is two characters side by side: anything of another size is not one
    if !(0.3 * q..=0.75 * q).contains(&wide) || !(0.25 * q..=0.62 * q).contains(&tall) {
        return None;
    }
    let pad = 1usize;
    let crop = v.crop(
        bx0.saturating_sub(pad) as i64,
        by0.saturating_sub(pad) as i64,
        (bx1 + pad).min(v.w) as i64,
        (by1 + pad).min(v.h) as i64,
    );
    glyph::featurize(&crop)
}

/// Reads the codes in the buffer of a screen whose reading is known: codes first, then empty slots. `None` when a slot is neither, or a code does not pass the gate: what cannot be read with confidence is not guessed at.
pub fn buffer_codes(
    frame: &Frame,
    reading: &Reading,
    templates: &Templates,
    gate: &Gate,
) -> Option<Vec<u8>> {
    let g = &reading.geometry;
    if !frame.is_valid() || g.seq_pitch_x < 8.0 {
        return None;
    }
    let mut feats = Vec::new();
    let mut ended = false;
    for i in 0..reading.buffer {
        let cx = g.seq_x0 + i as f32 * g.seq_pitch_x;
        match slot(frame, cx, g.buffer_y, g.seq_pitch_x) {
            Slot::Code(feat) if !ended => feats.push(feat),
            Slot::Empty => ended = true,
            // a code after an empty slot is as little a buffer as a slot that cannot be told
            Slot::Code(_) | Slot::Unclear => return None,
        }
    }
    if feats.is_empty() {
        return Some(Vec::new());
    }
    // every code in the buffer was picked from the matrix
    let mut prior = [0f32; NCODES];
    for &code in &reading.matrix {
        if let Some(p) = prior.get_mut(code as usize) {
            *p = 0.04;
        }
    }
    let labels = glyph::label_group(&feats, templates, prior, None).labels;
    let known = |code: u8| reading.matrix.contains(&code);
    labels
        .iter()
        .all(|l| l.ok(gate) && known(l.code))
        .then(|| labels.iter().map(|l| l.code).collect())
}
