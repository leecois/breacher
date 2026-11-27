use super::debug::{dbg_log, dump_plane};
use super::lattice::Lattice;
use super::measure::{median, window_energy};
use super::region::{remove_lines, Region};
use super::{fail, ReadError, ReadOptions, Stage};
use crate::glyph::{self, Feat};
use crate::image::{self, Frame, Plane};

pub(super) struct Sequences {
    pub(super) x0: f32,
    pub(super) q: f32,
    pub(super) rows: Vec<f32>,
    pub(super) feats: Vec<Vec<Option<Feat>>>,
    pub(super) fallback: bool,
}

/// A candidate band of the sequence panel: first row, last row, centre x of column 0, row runs.
type Band = (usize, usize, f32, Vec<(usize, usize)>);

pub(super) fn read_sequences(
    frame: &Frame,
    lat: &Lattice,
    refv: f32,
    opt: &ReadOptions,
) -> Result<Sequences, ReadError> {
    let l = &opt.layout;
    let (px, py, n) = (lat.px, lat.py, lat.n as f32);
    let q = l.seq_pitch_x * px;
    let gh = l.glyph_h * py;
    let xc = lat.x0 + (n - 1.0) / 2.0 * px;
    let x_pred = xc + (n / 2.0 + l.seq_col_offset) * px;
    // anchored to the matrix, not the prediction: the leftmost column of codes right of it is sequence column 0
    let matrix_right = lat.x0 + (n - 1.0) * px + 0.5 * px;
    let reg = Region::new(
        frame,
        matrix_right,
        lat.y0 - 3.0 * py,
        (x_pred + 9.2 * q).max(matrix_right + 16.0 * q),
        lat.y0 + (n + 1.5).max(9.0) * py,
        px,
        opt.contrast,
    );
    if reg.v.w < (2.5 * q) as usize || reg.v.h < (2.0 * gh) as usize {
        return fail(Stage::Sequences, "the sequence panel is outside the frame");
    }
    let mut mask = reg.mask(0.42 * refv);
    let (w, h) = (mask.w, mask.h);
    // long horizontal rules: the panel's borders, the separator under its title, frame lines further down
    let long = (5.0 * q) as usize;
    let soft = reg.mask(0.2 * refv);
    let mut rules: Vec<(usize, usize)> = Vec::new(); // first row, last row
    for y in 0..h {
        let row = &soft.data[y * w..(y + 1) * w];
        let (mut run, mut gap, mut is_rule) = (0usize, 0usize, false);
        for &b in row {
            if b != 0 {
                run += 1 + gap;
                gap = 0;
            } else if run > 0 && gap < 2 {
                gap += 1;
            } else {
                run = 0;
                gap = 0;
            }
            if run >= long {
                is_rule = true;
                break;
            }
        }
        if is_rule {
            match rules.last_mut() {
                Some(r) if r.1 + 1 == y => r.1 = y,
                _ => rules.push((y, y)),
            }
        }
    }
    remove_lines(&mut mask, (1.3 * q) as usize, (1.6 * gh) as usize);
    // keep only glyph-sized pieces, so small legal text and wide tables cannot pose as a column of codes
    let mut glyphs = Plane::new(w, h);
    for b in image::components(&mask, 0) {
        let (bw, bh) = (b.w() as f32, b.h() as f32);
        if bh >= 0.4 * gh && bh <= 1.6 * gh && bw <= 1.1 * q {
            for y in b.y0..b.y1 {
                glyphs.data[y * w + b.x0..y * w + b.x1]
                    .copy_from_slice(&mask.data[y * w + b.x0..y * w + b.x1]);
            }
        }
    }
    dump_plane("seq_v", &reg.v, 1);
    dump_plane("seq_mask", &mask, 255);
    dump_plane("seq_glyphs", &glyphs, 255);
    // The rules cut the region into bands (above the panel, its title, its list, below it).
    let mut bounds: Vec<(usize, usize)> = Vec::new();
    let mut from = 0usize;
    for &(ra, rb) in &rules {
        if ra > from {
            bounds.push((from, ra));
        }
        from = rb + 1;
    }
    if from < h {
        bounds.push((from, h));
    }
    let step = (0.02 * q).max(0.5);
    let rx = reg.x0 as f32;
    // column 0 is the leftmost column of codes; `glyphs` only locates it, slot occupancy is judged on the full mask
    let find = |top: usize, bottom: usize| -> Option<(f32, Vec<(usize, usize)>)> {
        let mut colp = vec![0f32; w];
        for y in top..bottom {
            for (c, &b) in colp.iter_mut().zip(&glyphs.data[y * w..(y + 1) * w]) {
                *c += b as f32;
            }
        }
        let (mut cum, mut wide) = (vec![0f32; w + 1], vec![0f32; w + 1]);
        for x in 0..w {
            cum[x + 1] = cum[x] + colp[x];
            wide[x + 1] = wide[x] + (colp[x] > 0.0) as u8 as f32;
        }
        let span = |xc: f32, hw: f32| {
            (
                ((xc - hw - rx).round() as i64).clamp(0, w as i64) as usize,
                ((xc + hw - rx).round() as i64).clamp(0, w as i64) as usize,
            )
        };
        let energy = |xc: f32, hw: f32| {
            let (a, b) = span(xc, hw);
            cum[b.max(a)] - cum[a]
        };
        // a column of codes is also wide; a border broken into glyph-height pieces is only a couple of pixels across
        let spread = |xc: f32, hw: f32| {
            let (a, b) = span(xc, hw);
            wide[b.max(a)] - wide[a]
        };
        let comb = |xc: f32| {
            energy(xc, 0.3 * q) + energy(xc + q, 0.3 * q)
                - 3.0 * (energy(xc + 0.5 * q, 0.07 * q) + energy(xc - 0.5 * q, 0.07 * q))
                - 2.0 * energy(xc - q, 0.3 * q)
        };
        let column = |x: f32| energy(x, 0.3 * q) >= 1.2 * gh && spread(x, 0.3 * q) >= 0.3 * q;
        let (lo, hi) = (rx + 1.3 * q, rx + w as f32 - 1.5 * q);
        let mut x = lo;
        let mut x0 = None;
        while x <= hi {
            if column(x) && column(x + q) && energy(x - q, 0.3 * q) < 0.2 * energy(x, 0.3 * q) {
                // settle on the best alignment within the next half pitch
                let mut best = (f32::NEG_INFINITY, x);
                let mut xx = x;
                while xx <= (x + 0.6 * q).min(hi) {
                    let sc = comb(xx);
                    if sc > best.0 {
                        best = (sc, xx);
                    }
                    xx += step;
                }
                x0 = Some(best.1);
                break;
            }
            x += step;
        }
        let x0 = x0?;
        let (ca, cb) = span(x0, 0.42 * q);
        let mut runs: Vec<(usize, usize)> = Vec::new();
        let mut start: Option<usize> = None;
        for y in top..=bottom {
            let on = y < bottom && mask.data[y * w + ca..y * w + cb].iter().any(|&b| b != 0);
            match (on, start) {
                (true, None) => start = Some(y),
                (false, Some(s)) => {
                    let len = (y - s) as f32;
                    if len >= 0.5 * gh && len <= 1.9 * gh {
                        runs.push((s, y));
                    }
                    start = None;
                }
                _ => {}
            }
        }
        Some((x0, runs))
    };
    // the list is the band whose first column holds the most rows; on a tie the lowest, since a title sits above its list
    let mut chosen: Option<Band> = None;
    for &(a, b) in &bounds {
        if let Some((x0, runs)) = find(a, b) {
            dbg_log!(
                "  band {:.2}..{:.2} (rows from matrix row 0): column at {x0:.1}, {} rows",
                (reg.y0 as f32 + a as f32 - lat.y0) / py,
                (reg.y0 as f32 + b as f32 - lat.y0) / py,
                runs.len()
            );
            if !runs.is_empty() && chosen.as_ref().is_none_or(|c| runs.len() >= c.3.len()) {
                chosen = Some((a, b, x0, runs));
            }
        }
    }
    let Some((top, bottom, x0, mut runs)) = chosen else {
        return fail(Stage::Sequences, "no sequence column found");
    };
    // of two runs closer than the smallest row pitch the upper one is the panel title, not a sequence
    let centre = |r: (usize, usize)| (r.0 + r.1) as f32 / 2.0;
    while runs.len() >= 2 && centre(runs[1]) - centre(runs[0]) < 0.92 * l.seq_pitch_y.0 * py {
        runs.remove(0);
    }
    // the layout prediction is only a report: how far the panel is from where captures put it
    let fallback = (x0 - x_pred).abs() > 0.35 * q;
    dbg_log!("  sequence column: x_pred={x_pred:.1} x0={x0:.1} (off by {:.2} q) q={q:.1} gh={gh:.1} list band {top}..{bottom} of {h} ({} rules), {} rows", (x0 - x_pred) / q, rules.len(), runs.len());
    if runs.is_empty() {
        return fail(Stage::Sequences, "no glyph rows in the sequence column");
    }
    let vmax = reg.v.percentile(0.999) as f32;
    let filled = |yc: f32| {
        // bright filled row: an already-installed daemon's banner
        let wv = reg.v_window(x0 - 0.4 * q, yc - 0.2 * py, x0 + 4.5 * q, yc + 0.2 * py);
        !wv.is_empty()
            && wv.percentile(0.5) as f32 > 0.5 * vmax
            && wv.percentile(0.2) as f32 > 0.35 * vmax
    };
    let mut rows: Vec<f32> = Vec::new();
    let mut feats: Vec<Vec<Option<Feat>>> = Vec::new();
    let mut centres: Vec<(f32, bool)> = Vec::new(); // (centre, is_banner)
    for &(ya, yb) in &runs {
        let yc = reg.y0 as f32 + (ya + yb) as f32 / 2.0;
        if filled(yc) {
            centres.push((yc, true));
            continue;
        }
        let (wy0, wy1) =
            (reg.y0 as f32 + ya as f32 - 0.2 * gh, reg.y0 as f32 + yb as f32 + 0.2 * gh);
        let mut row = Vec::new();
        for k in 0..9 {
            let xk = x0 + k as f32 * q;
            let e = window_energy(
                &mask,
                xk - reg.x0 as f32,
                (wy0 + wy1) / 2.0 - reg.y0 as f32,
                0.46 * q,
                (wy1 - wy0) / 2.0,
            );
            if e < 0.04 {
                break;
            }
            // one full pitch wide: the widest token keeps a margin and the neighbours' edges stay out
            row.push(glyph::featurize(&reg.v_window(xk - 0.5 * q, wy0, xk + 0.5 * q, wy1)));
        }
        match row.len() {
            0 => continue,
            1 => return fail(Stage::Sequences, "a sequence row has a single readable code"),
            _ => {
                centres.push((yc, false));
                rows.push(yc);
                feats.push(row);
            }
        }
    }
    dbg_log!(
        "  sequences: x_pred={x_pred:.1} x0={x0:.1} q={q:.1} gh={gh:.1} fallback={fallback} runs(y/py rel row0, h/gh)={:?} rows={:?}",
        runs.iter().map(|r| (((reg.y0 as f32 + (r.0 + r.1) as f32 / 2.0 - lat.y0) / py * 100.0).round() / 100.0, ((r.1 - r.0) as f32 / gh * 100.0).round() / 100.0)).collect::<Vec<_>>(),
        centres.iter().map(|c| (((c.0 - lat.y0) / py * 100.0).round() / 100.0, c.1)).collect::<Vec<_>>()
    );
    if rows.is_empty() {
        return fail(Stage::Sequences, "no sequence with two or more codes");
    }
    // rows sit on a fixed vertical pitch; a hole between two rows must be an installed-daemon banner
    if centres.len() >= 2 {
        let diffs: Vec<f32> = centres.windows(2).map(|p| p[1].0 - p[0].0).collect();
        let (lo, hi) = (l.seq_pitch_y.0 * py, l.seq_pitch_y.1 * py);
        let mut unit: Vec<f32> = diffs.iter().copied().filter(|d| *d >= lo && *d <= hi).collect();
        let pitch = if unit.is_empty() { (lo + hi) / 2.0 } else { median(&mut unit) };
        for (i, d) in diffs.iter().enumerate() {
            let steps = (d / pitch).round();
            if steps < 1.0 || (d - steps * pitch).abs() > 0.22 * pitch {
                return fail(
                    Stage::Sequences,
                    format!("sequence rows are off-pitch ({:.2} pitches apart)", d / pitch),
                );
            }
            for g in 1..steps as usize {
                if !filled(centres[i].0 + g as f32 * pitch) {
                    return fail(
                        Stage::Sequences,
                        "an unreadable row sits between two sequence rows",
                    );
                }
            }
        }
    }
    Ok(Sequences { x0, q, rows, feats, fallback })
}
