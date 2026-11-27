use super::segment::{contrast, odd};
use super::Contrast;
use crate::image::{self, Frame, Plane};

pub(super) struct Region {
    pub(super) x0: i64,
    pub(super) y0: i64,
    pub(super) v: Plane,
    pub(super) th: Plane,
}

impl Region {
    pub(super) fn new(
        frame: &Frame,
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        pitch: f32,
        mode: Contrast,
    ) -> Region {
        let (x0, y0) = (x0.floor().max(0.0) as i64, y0.floor().max(0.0) as i64);
        let (x1, y1) = (
            (x1.ceil() as i64).min(frame.width as i64),
            (y1.ceil() as i64).min(frame.height as i64),
        );
        let v = frame.v_crop(x0, y0, x1, y1);
        let th = if v.is_empty() { v.clone() } else { contrast(&v, odd(0.47 * pitch), mode) };
        Region { x0, y0, v, th }
    }

    pub(super) fn mask(&self, cut: f32) -> Plane {
        image::threshold(&self.th, cut.clamp(0.0, 254.0) as u8)
    }

    /// Crop of V given frame coordinates.
    pub(super) fn v_window(&self, xa: f32, ya: f32, xb: f32, yb: f32) -> Plane {
        self.v.crop(
            xa.round() as i64 - self.x0,
            ya.round() as i64 - self.y0,
            xb.round() as i64 - self.x0,
            yb.round() as i64 - self.y0,
        )
    }
}

/// Clears horizontal runs longer than `max_w` and vertical runs longer than `max_h`: borders and rules, never glyph strokes.
pub(super) fn remove_lines(mask: &mut Plane, max_w: usize, max_h: usize) {
    let (w, h) = (mask.w, mask.h);
    let src = mask.data.clone();
    for y in 0..h {
        let mut x = 0;
        while x < w {
            if src[y * w + x] == 0 {
                x += 1;
                continue;
            }
            let s = x;
            while x < w && src[y * w + x] != 0 {
                x += 1;
            }
            if x - s > max_w {
                for xx in s..x {
                    for yy in y.saturating_sub(1)..(y + 2).min(h) {
                        mask.data[yy * w + xx] = 0;
                    }
                }
            }
        }
    }
    for x in 0..w {
        let mut y = 0;
        while y < h {
            if src[y * w + x] == 0 {
                y += 1;
                continue;
            }
            let s = y;
            while y < h && src[y * w + x] != 0 {
                y += 1;
            }
            if y - s > max_h {
                for yy in s..y {
                    for xx in x.saturating_sub(1)..(x + 2).min(w) {
                        mask.data[yy * w + xx] = 0;
                    }
                }
            }
        }
    }
}
