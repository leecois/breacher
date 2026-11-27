use super::debug::dbg_log;
use super::measure::{median, window_stats};
use super::segment::Segmented;
use super::{fail, ReadError, Stage};
use crate::image::{self, Blob, Plane};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug)]
pub(super) struct Lattice {
    pub(super) x0: f32,
    pub(super) y0: f32,
    pub(super) px: f32,
    pub(super) py: f32,
    pub(super) n: usize,
}

/// Histogram peak in `[lo, hi]`, refined to the median of the samples around it.
fn peak(values: &[f32], lo: f32, hi: f32, bin: f32) -> Option<f32> {
    let nb = (((hi - lo) / bin).ceil() as usize).max(1);
    let mut hist = vec![0u32; nb + 2];
    let mut count = 0;
    for &v in values {
        if v >= lo && v <= hi {
            hist[(((v - lo) / bin) as usize).min(nb - 1) + 1] += 1;
            count += 1;
        }
    }
    if count < 3 {
        return None;
    }
    let best = (1..=nb).max_by_key(|&i| hist[i - 1] + hist[i] + hist[i + 1])?;
    let centre = lo + (best as f32 - 0.5) * bin;
    let mut near: Vec<f32> =
        values.iter().copied().filter(|v| (v - centre).abs() <= 1.5 * bin).collect();
    if near.is_empty() {
        return None;
    }
    Some(median(&mut near))
}

/// Dominant phase of `values` modulo `pitch`.
fn phase(values: &[f32], pitch: f32) -> f32 {
    let tau = std::f32::consts::TAU;
    let ang: Vec<f32> = values.iter().map(|v| v.rem_euclid(pitch) / pitch * tau).collect();
    let diff = |a: f32, b: f32| {
        let d = (a - b).rem_euclid(tau);
        if d > tau / 2.0 {
            d - tau
        } else {
            d
        }
    };
    let window = tau * 0.12;
    let mut best = (0usize, 0f32);
    for &a in &ang {
        let c = ang.iter().filter(|&&b| diff(b, a).abs() < window).count();
        if c > best.0 {
            best = (c, a);
        }
    }
    let (mut sum, mut cnt) = (0f32, 0f32);
    for &b in &ang {
        let d = diff(b, best.1);
        if d.abs() < window {
            sum += d;
            cnt += 1.0;
        }
    }
    (best.1 + sum / cnt.max(1.0)).rem_euclid(tau) / tau * pitch
}

/// A matrix cell holds a code: enough stroke, and shaped like a glyph rather than a rule or a border.
fn cell_has_glyph(mask: &Plane, xc: f32, yc: f32, px: f32, py: f32) -> bool {
    let (e, wx, wy) = window_stats(mask, xc, yc, CELL_HW * px, CELL_HH * py);
    e >= CELL_ON && wy >= 0.15 * py && wx >= 0.18 * px
}

/// Stricter than `cell_has_glyph`: the strokes have the size of one code, unlike legal text, tables or a border.
fn cell_has_code(mask: &Plane, xc: f32, yc: f32, px: f32, py: f32) -> bool {
    let (e, wx, wy) = window_stats(mask, xc, yc, CELL_HW * px, CELL_HH * py);
    e >= CELL_ON && wy >= 0.18 * py && wy <= 0.5 * py && wx >= 0.2 * px && wx <= 0.68 * px
}

pub(super) const CELL_HW: f32 = 0.40;
pub(super) const CELL_HH: f32 = 0.34;
const CELL_ON: f32 = 0.035;

pub(super) fn find_lattice(seg: &Segmented, p0: f32) -> Result<Lattice, ReadError> {
    let bridge = ((0.11 * p0).round() as usize).max(1);
    let blobs: Vec<Blob> = image::components(&seg.mask, bridge)
        .into_iter()
        .filter(|b| {
            let (w, h) = (b.w() as f32, b.h() as f32);
            h >= 0.14 * p0 && h <= 0.55 * p0 && w >= 0.22 * p0 && w <= 0.78 * p0
        })
        .collect();
    if blobs.len() < 9 {
        return fail(Stage::Lattice, format!("only {} code-sized blobs", blobs.len()));
    }
    if blobs.len() > 4000 {
        return fail(Stage::Lattice, "frame too busy");
    }
    let mut hs: Vec<f32> = blobs.iter().map(|b| b.h() as f32).collect();
    let tol = (0.35 * median(&mut hs)).max(1.5);
    let (cx, cy): (Vec<f32>, Vec<f32>) = blobs.iter().map(|b| (b.cx(), b.cy())).unzip();
    let m = blobs.len();
    let (mut dxs, mut dys) = (Vec::new(), Vec::new());
    for i in 0..m {
        for j in i + 1..m {
            let (dx, dy) = ((cx[i] - cx[j]).abs(), (cy[i] - cy[j]).abs());
            if dy < tol && dx > 1.0 {
                dxs.push(dx);
            }
            if dx < 1.5 * tol && dy > 1.0 {
                dys.push(dy);
            }
        }
    }
    let bin = (0.03 * p0).max(1.0);
    let Some(px) = peak(&dxs, 0.775 * p0, 1.27 * p0, bin) else {
        return fail(Stage::Lattice, "no horizontal pitch");
    };
    let Some(py) = peak(&dys, 0.62 * px, 1.25 * px, bin) else {
        return fail(Stage::Lattice, "no vertical pitch");
    };
    let has_nb: Vec<bool> = (0..m)
        .map(|i| {
            (0..m).any(|j| {
                j != i
                    && (((cy[i] - cy[j]).abs() < tol
                        && ((cx[i] - cx[j]).abs() - px).abs() < 0.12 * px)
                        || ((cx[i] - cx[j]).abs() < 1.5 * tol
                            && ((cy[i] - cy[j]).abs() - py).abs() < 0.12 * py))
            })
        })
        .collect();
    let members: Vec<usize> = (0..m).filter(|&i| has_nb[i]).collect();
    if members.len() < 9 {
        return fail(Stage::Lattice, "too few pitch-consistent blobs");
    }
    let phx = phase(&members.iter().map(|&i| cx[i]).collect::<Vec<_>>(), px);
    let phy = phase(&members.iter().map(|&i| cy[i]).collect::<Vec<_>>(), py);
    let mut cells: HashMap<(i32, i32), usize> = HashMap::new();
    for &i in &members {
        let gi = ((cx[i] - phx) / px).round();
        let gj = ((cy[i] - phy) / py).round();
        if (cx[i] - (phx + gi * px)).abs() < 0.2 * px && (cy[i] - (phy + gj * py)).abs() < 0.2 * py
        {
            cells.entry((gi as i32, gj as i32)).or_insert(i);
        }
    }
    // largest 4-connected block
    let mut seen: HashMap<(i32, i32), bool> = HashMap::new();
    let mut best: Vec<(i32, i32)> = Vec::new();
    let mut keys: Vec<(i32, i32)> = cells.keys().copied().collect();
    keys.sort();
    for start in keys {
        if seen.contains_key(&start) {
            continue;
        }
        let mut comp = Vec::new();
        let mut stack = vec![start];
        seen.insert(start, true);
        while let Some(c) = stack.pop() {
            comp.push(c);
            for d in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let nb = (c.0 + d.0, c.1 + d.1);
                if cells.contains_key(&nb) && !seen.contains_key(&nb) {
                    seen.insert(nb, true);
                    stack.push(nb);
                }
            }
        }
        if comp.len() > best.len() {
            best = comp;
        }
    }
    if best.len() < 6 {
        return fail(Stage::Lattice, "no grid of codes");
    }
    // least-squares refit of origin and pitch on the block
    let fit = |pairs: &[(f32, f32)], fallback: (f32, f32)| -> (f32, f32) {
        let n = pairs.len() as f32;
        let (sx, sy) = pairs.iter().fold((0f32, 0f32), |a, p| (a.0 + p.0, a.1 + p.1));
        let (mx, my) = (sx / n, sy / n);
        let (mut sxx, mut sxy) = (0f32, 0f32);
        for p in pairs {
            sxx += (p.0 - mx) * (p.0 - mx);
            sxy += (p.0 - mx) * (p.1 - my);
        }
        if sxx < 1e-3 {
            return fallback;
        }
        let slope = sxy / sxx;
        (my - slope * mx, slope)
    };
    let xs: Vec<(f32, f32)> = best.iter().map(|k| (k.0 as f32, cx[cells[k]])).collect();
    let ys: Vec<(f32, f32)> = best.iter().map(|k| (k.1 as f32, cy[cells[k]])).collect();
    let (ox, px) = fit(&xs, (phx, px));
    let (oy, py) = fit(&ys, (phy, py));
    let (mut i0, mut i1) =
        (best.iter().map(|k| k.0).min().unwrap(), best.iter().map(|k| k.0).max().unwrap());
    let (mut j0, mut j1) =
        (best.iter().map(|k| k.1).min().unwrap(), best.iter().map(|k| k.1).max().unwrap());
    // occupancy is judged against the matrix's own stroke level: on soft frames codes are much fainter than headings
    let local = {
        let region = seg.th.crop(
            (ox + (i0 as f32 - 0.5) * px).round() as i64,
            (oy + (j0 as f32 - 0.5) * py).round() as i64,
            (ox + (i1 as f32 + 0.5) * px).round() as i64,
            (oy + (j1 as f32 + 0.5) * py).round() as i64,
        );
        if region.is_empty() {
            seg.refv
        } else {
            region.percentile(0.95) as f32
        }
    };
    let local_cut = (0.42 * local).min(0.42 * seg.refv).max(0.15 * seg.refv);
    let local_mask = image::threshold(&seg.th, local_cut as u8);
    let on = |i: i32, j: i32| {
        cell_has_glyph(&local_mask, ox + i as f32 * px, oy + j as f32 * py, px, py)
    };
    // a non-square block lost a row or column: grow the short side over lines of glyph-shaped cells, never a square block
    for _ in 0..4 {
        let (nc, nr) = (i1 - i0 + 1, j1 - j0 + 1);
        if nc == nr {
            break;
        }
        if nc < nr {
            let support = |i: i32| (j0..=j1).filter(|&j| on(i, j)).count() as f32 / nr as f32;
            let (l, r) = (support(i0 - 1), support(i1 + 1));
            if l.max(r) < 0.6 {
                break;
            }
            if l >= r {
                i0 -= 1;
            } else {
                i1 += 1;
            }
        } else {
            let support = |j: i32| (i0..=i1).filter(|&i| on(i, j)).count() as f32 / nc as f32;
            let (u, d) = (support(j0 - 1), support(j1 + 1));
            if u.max(d) < 0.6 {
                break;
            }
            if u >= d {
                j0 -= 1;
            } else {
                j1 += 1;
            }
        }
    }
    // a square block can still be too small: a side whose neighbouring line holds code-sized strokes belongs to the matrix
    let code =
        |i: i32, j: i32| cell_has_code(&local_mask, ox + i as f32 * px, oy + j as f32 * py, px, py);
    for _ in 0..3 {
        let cols = (i1 - i0 + 1) as f32;
        let rows = (j1 - j0 + 1) as f32;
        let side = [
            (j0..=j1).filter(|&j| code(i0 - 1, j)).count() as f32 / rows,
            (j0..=j1).filter(|&j| code(i1 + 1, j)).count() as f32 / rows,
            (i0..=i1).filter(|&i| code(i, j0 - 1)).count() as f32 / cols,
            (i0..=i1).filter(|&i| code(i, j1 + 1)).count() as f32 / cols,
        ];
        if side.iter().all(|&v| v < 0.6) {
            break;
        }
        if side[0] >= 0.6 {
            i0 -= 1;
        }
        if side[1] >= 0.6 {
            i1 += 1;
        }
        if side[2] >= 0.6 {
            j0 -= 1;
        }
        if side[3] >= 0.6 {
            j1 += 1;
        }
    }
    let (ncol, nrow) = ((i1 - i0 + 1) as usize, (j1 - j0 + 1) as usize);
    dbg_log!("  lattice: p0={p0:.1} blobs={} members={} block={} px={px:.2} py={py:.2} origin=({ox:.1},{oy:.1}) i={i0}..{i1} j={j0}..{j1}", blobs.len(), members.len(), best.len());
    if !(3..=9).contains(&ncol) || !(3..=9).contains(&nrow) {
        return fail(Stage::Lattice, format!("implausible grid {nrow}x{ncol}"));
    }
    if ncol != nrow {
        return fail(Stage::Lattice, format!("grid is not square ({nrow}x{ncol})"));
    }
    let empty =
        (j0..=j1).flat_map(|j| (i0..=i1).map(move |i| (i, j))).filter(|&(i, j)| !on(i, j)).count();
    if empty > 0 {
        return fail(Stage::InProgress, format!("{empty} of {} matrix cells are empty (breach already started, or something covers the matrix)", ncol * nrow));
    }
    Ok(Lattice { x0: ox + i0 as f32 * px, y0: oy + j0 as f32 * py, px, py, n: ncol })
}
