use super::lattice::Lattice;
use super::region::Region;
use super::{fail, ReadError, ReadOptions, Stage};
use crate::image::Frame;

pub(super) struct Buffer {
    pub(super) size: usize,
    pub(super) from_top: Option<usize>,
    pub(super) from_side: Option<usize>,
    pub(super) from_slots: Option<usize>,
    pub(super) agreed: bool,
    pub(super) y: f32,
}

pub(super) fn read_buffer(
    frame: &Frame,
    lat: &Lattice,
    seq_x0: f32,
    q: f32,
    refv: f32,
    opt: &ReadOptions,
) -> Result<Buffer, ReadError> {
    let l = &opt.layout;
    let (px, py) = (lat.px, lat.py);
    let yb_pred = lat.y0 + l.buffer_row * py;
    // anchored to the matrix: everything above the sequence list, so the panel is found wherever it sits
    let reg = Region::new(
        frame,
        seq_x0 - 1.2 * px,
        lat.y0 - 5.2 * py,
        seq_x0 + 17.0 * q,
        lat.y0 - 0.9 * py,
        px,
        opt.contrast,
    );
    if reg.v.w < (3.0 * q) as usize || reg.v.h < (1.2 * py) as usize {
        return fail(Stage::Buffer, "the buffer panel is outside the frame");
    }
    let (w, h) = (reg.th.w, reg.th.h);
    // threshold relative to the solid panel border, the brightest thin structure here
    let local = reg.th.percentile(0.995) as f32;
    let soft = reg.mask((0.4 * local).max(0.1 * refv));
    let rel_x = |x: f32| x - reg.x0 as f32;
    let rel_y = |y: f32| y - reg.y0 as f32;
    let to_count =
        |x_right: f32| ((x_right - seq_x0 - l.buffer_right_pad * px) / q).round() as i64 + 1;
    let plausible = |n: i64| (2..=16).contains(&n).then_some(n as usize);

    // (a) the panel by its shape: two border lines over slot 0, about a row apart, ending at the same x
    let xs = (rel_x(seq_x0).round() as i64).clamp(0, w as i64 - 1) as usize;
    let max_gap = ((0.08 * q) as usize).max(2);
    let run_end = |y: usize| -> Option<usize> {
        let row = &soft.data[y * w..(y + 1) * w];
        if row[xs] == 0 {
            return None;
        }
        let (mut x, mut gap, mut end) = (xs, 0usize, xs);
        while x < w && gap <= max_gap {
            if row[x] != 0 {
                end = x + 1;
                gap = 0;
            } else {
                gap += 1;
            }
            x += 1;
        }
        (end < w && (end - xs) as f32 >= 1.5 * q).then_some(end)
    };
    let tol = (0.1 * px).max(2.0);
    let mut lines: Vec<(usize, usize, f32)> = Vec::new(); // first row, last row, right end
    for y in 0..h {
        if let Some(e) = run_end(y) {
            match lines.last_mut() {
                Some(l) if l.1 + 1 == y && (l.2 - e as f32).abs() <= tol => {
                    l.1 = y;
                    l.2 = l.2.max(e as f32);
                }
                _ => lines.push((y, y, e as f32)),
            }
        }
    }
    let mut panel: Option<(f32, f32)> = None; // (centre y, right end), region coordinates
    for i in 0..lines.len() {
        for j in i + 1..lines.len() {
            let (ya, yz) =
                ((lines[i].0 + lines[i].1) as f32 / 2.0, (lines[j].0 + lines[j].1) as f32 / 2.0);
            let dy = yz - ya;
            if dy >= 0.85 * py && dy <= 1.45 * py && (lines[i].2 - lines[j].2).abs() <= tol {
                // the buffer panel is the lowest such box; titles and banners further up must not be taken for it
                let mid = (ya + yz) / 2.0;
                if panel.is_none_or(|p| mid > p.0) {
                    panel = Some((mid, (lines[i].2 + lines[j].2) / 2.0));
                }
            }
        }
    }
    let yb = panel.map_or(yb_pred, |p| reg.y0 as f32 + p.0);
    let from_top = panel.and_then(|p| plausible(to_count(reg.x0 as f32 + p.1)));

    // (b) right border: first column right of slot 0 with a tall vertical run through the buffer row
    let yrow = (rel_y(yb).round() as i64).clamp(0, h as i64 - 1) as usize;
    let mut from_side = None;
    let mut side_x: Option<usize> = None;
    let xa = (rel_x(seq_x0 + 0.3 * q).round() as i64).clamp(0, w as i64) as usize;
    for x in xa..w {
        let col =
            |y: usize| soft.data[y * w + x] != 0 || (x + 1 < w && soft.data[y * w + x + 1] != 0);
        if !(yrow.saturating_sub(2)..(yrow + 3).min(h)).any(col) {
            continue;
        }
        let (mut up, mut dn) = (yrow, yrow);
        while up > 0 && col(up - 1) {
            up -= 1;
        }
        while dn + 1 < h && col(dn + 1) {
            dn += 1;
        }
        if (dn - up + 1) as f32 >= 0.8 * py {
            from_side = plausible(to_count(reg.x0 as f32 + x as f32 + 0.5));
            side_x = Some(x);
            break;
        }
    }
    // a lone side border counts only if a horizontal border line runs from slot 0 up to it and stops there
    let side_confirmed = side_x.is_some_and(|sx| {
        let xa = (rel_x(seq_x0 - 0.4 * px).round() as i64).clamp(0, w as i64) as usize;
        let xb = sx.saturating_sub(2).max(xa + 1).min(w);
        let yz = h;
        let beyond_a = (sx + (0.25 * q) as usize).min(w);
        let beyond_b = (sx + (1.25 * q) as usize).min(w);
        [(yb - 0.78 * py, yb - 0.36 * py), (yb + 0.36 * py, yb + 0.78 * py)].iter().any(
            |&(za, zb)| {
                let (ra, rb) = (
                    (rel_y(za).round() as i64).clamp(0, yz as i64) as usize,
                    (rel_y(zb).round() as i64).clamp(0, yz as i64) as usize,
                );
                (ra..rb).any(|y| {
                    let row = &soft.data[y * w..(y + 1) * w];
                    let inside = row[xa..xb].iter().filter(|&&b| b != 0).count() as f32
                        / (xb - xa).max(1) as f32;
                    let outside = if beyond_b > beyond_a {
                        row[beyond_a..beyond_b].iter().filter(|&&b| b != 0).count() as f32
                            / (beyond_b - beyond_a) as f32
                    } else {
                        0.0
                    };
                    inside >= 0.7 && outside < 0.5
                })
            },
        )
    });

    // (c) count the dashed slots themselves (only resolvable at about 900p and up)
    let lo = reg.mask(0.16 * refv);
    let (bw, bh) = (l.buffer_slot * px, l.buffer_slot * py);
    let ring = |xc: f32, yc: f32| -> f32 {
        let (xa, xb) =
            ((rel_x(xc - bw / 2.0)).round() as i64, (rel_x(xc + bw / 2.0)).round() as i64);
        let (ya, yb2) =
            ((rel_y(yc - bh / 2.0)).round() as i64, (rel_y(yc + bh / 2.0)).round() as i64);
        let t = ((0.10 * bw).round() as i64).max(2);
        if xa - t < 0
            || ya - t < 0
            || xb + t >= w as i64
            || yb2 + t >= h as i64
            || xb <= xa
            || yb2 <= ya
        {
            return 0.0;
        }
        let at = |x: i64, y: i64| lo.data[y as usize * w + x as usize] != 0;
        let horiz = |yy: i64| {
            (xa..xb).filter(|&x| (yy - t..=yy + t).any(|y| at(x, y))).count() as f32
                / (xb - xa) as f32
        };
        let vert = |xx: i64| {
            (ya..yb2).filter(|&y| (xx - t..=xx + t).any(|x| at(x, y))).count() as f32
                / (yb2 - ya) as f32
        };
        let edges = horiz(ya).min(horiz(yb2)).min(vert(xa)).min(vert(xb));
        let (mut inner, mut total) = (0u32, 0u32);
        for y in ya + 3 * t..yb2 - 3 * t {
            for x in xa + 3 * t..xb - 3 * t {
                inner += at(x, y) as u32;
                total += 1;
            }
        }
        edges - if total > 0 { inner as f32 / total as f32 } else { 0.0 }
    };
    let mut best = (0f32, 0f32, 0f32);
    for iy in -8..=8 {
        for ix in -6..=6 {
            let (dx, dy) = (ix as f32 * 0.05 * q, iy as f32 * 0.05 * py);
            let s = (0..3).map(|k| ring(seq_x0 + dx + k as f32 * q, yb + dy)).sum::<f32>() / 3.0;
            if s > best.0 {
                best = (s, dx, dy);
            }
        }
    }
    let mut slots = 0usize;
    for k in 0..17 {
        if ring(seq_x0 + best.1 + k as f32 * q, yb + best.2) < 0.4 {
            break;
        }
        slots += 1;
    }
    let from_slots = (slots >= 2).then_some(slots);

    // border estimates are primary and must agree; slot counting can only under-count, so it vetoes or is the last resort
    let size = match (from_top, from_side) {
        (Some(a), Some(b)) if a == b => a,
        (Some(a), Some(b)) => return fail(Stage::Buffer, format!("buffer border estimates disagree (top {a}, side {b}, slots {from_slots:?})")),
        (Some(a), None) => a,
        (None, Some(b)) if side_confirmed || from_slots == Some(b) => b,
        (None, Some(b)) => return fail(Stage::Buffer, format!("buffer right border found (size {b}) but not confirmed by a border line (slots {from_slots:?})")),
        (None, None) => match from_slots {
            Some(s) if s >= 4 => s,
            _ => return fail(Stage::Buffer, "buffer panel not found"),
        },
    };
    if let Some(s) = from_slots {
        if s > size {
            return fail(
                Stage::Buffer,
                format!("more buffer slots counted ({s}) than the panel holds ({size})"),
            );
        }
    }
    let agreed = [from_top, from_side, from_slots].into_iter().flatten().all(|v| v == size);
    Ok(Buffer {
        size,
        from_top,
        from_side,
        from_slots,
        agreed,
        y: yb + best.2 * (from_slots.is_some() as u8 as f32),
    })
}
