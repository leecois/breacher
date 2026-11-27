use crate::image::Plane;

pub(super) fn median(v: &mut [f32]) -> f32 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

pub(super) fn window_energy(mask: &Plane, xc: f32, yc: f32, hw: f32, hh: f32) -> f32 {
    window_stats(mask, xc, yc, hw, hh).0
}

/// (stroke fraction, stroke extent in x, stroke extent in y); the part of a window outside the frame counts as empty.
pub(super) fn window_stats(mask: &Plane, xc: f32, yc: f32, hw: f32, hh: f32) -> (f32, f32, f32) {
    let x0 = ((xc - hw).round() as i64).clamp(0, mask.w as i64) as usize;
    let x1 = ((xc + hw).round() as i64).clamp(0, mask.w as i64) as usize;
    let y0 = ((yc - hh).round() as i64).clamp(0, mask.h as i64) as usize;
    let y1 = ((yc + hh).round() as i64).clamp(0, mask.h as i64) as usize;
    if x1 <= x0 || y1 <= y0 {
        return (0.0, 0.0, 0.0);
    }
    let full = (2.0 * hw).round().max(1.0) * (2.0 * hh).round().max(1.0);
    let mut on = 0u32;
    let (mut xmin, mut xmax, mut ymin, mut ymax) = (usize::MAX, 0usize, usize::MAX, 0usize);
    for y in y0..y1 {
        let row = &mask.data[y * mask.w + x0..y * mask.w + x1];
        let mut any = false;
        for (i, &b) in row.iter().enumerate() {
            if b != 0 {
                on += 1;
                any = true;
                xmin = xmin.min(x0 + i);
                xmax = xmax.max(x0 + i + 1);
            }
        }
        if any {
            ymin = ymin.min(y);
            ymax = ymax.max(y + 1);
        }
    }
    if on == 0 {
        return (0.0, 0.0, 0.0);
    }
    (on as f32 / full, (xmax - xmin) as f32, (ymax - ymin) as f32)
}
