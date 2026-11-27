use super::{fail, Contrast, ReadError, Stage};
use crate::image::{self, Plane};

pub(super) struct Segmented {
    pub(super) th: Plane,
    pub(super) mask: Plane,
    pub(super) refv: f32,
}

pub(super) fn odd(v: f32) -> usize {
    ((v.round() as usize).max(3)) | 1
}

pub(super) fn contrast(v: &Plane, k: usize, mode: Contrast) -> Plane {
    match mode {
        Contrast::TopHat => image::top_hat(v, k),
        Contrast::BoxMean => image::box_contrast(v, k),
    }
}

pub(super) fn segment(v: &Plane, pitch: f32, mode: Contrast) -> Result<Segmented, ReadError> {
    let th = contrast(v, odd(0.47 * pitch), mode);
    let refv = th.percentile(0.997) as f32;
    if refv < 24.0 {
        return fail(Stage::Segment, format!("no bright strokes (reference contrast {refv:.0})"));
    }
    let mask = image::threshold(&th, (0.42 * refv) as u8);
    Ok(Segmented { th, mask, refv })
}
