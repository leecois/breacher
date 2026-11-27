//! Image loading and the perturbations used to stress the reader.
use breach_core::image::{Frame, PixelFormat};
use image::{imageops, ImageBuffer, Rgb, RgbImage};
use std::path::Path;

pub fn load(path: &Path) -> Result<RgbImage, String> {
    image::open(path).map(|i| i.to_rgb8()).map_err(|e| format!("{}: {e}", path.display()))
}

pub fn frame(img: &RgbImage) -> Frame<'_> {
    Frame::new(img.width() as usize, img.height() as usize, PixelFormat::Rgb8, img.as_raw())
}

fn srgb_to_lin(v: u8) -> f32 {
    let x = v as f32 / 255.0;
    if x <= 0.04045 {
        x / 12.92
    } else {
        ((x + 0.055) / 1.055).powf(2.4)
    }
}

fn lin_to_srgb(x: f32) -> u8 {
    let x = x.clamp(0.0, 1.0);
    let y = if x <= 0.0031308 { x * 12.92 } else { 1.055 * x.powf(1.0 / 2.4) - 0.055 };
    (y * 255.0 + 0.5) as u8
}

fn map_px(img: &RgbImage, f: impl Fn([f32; 3]) -> [f32; 3]) -> RgbImage {
    let mut out = img.clone();
    for p in out.pixels_mut() {
        let lin = [srgb_to_lin(p[0]), srgb_to_lin(p[1]), srgb_to_lin(p[2])];
        let o = f(lin);
        *p = Rgb([lin_to_srgb(o[0]), lin_to_srgb(o[1]), lin_to_srgb(o[2])]);
    }
    out
}

/// HDR scene grabbed into SDR without tone mapping: over-bright, clipped, slightly desaturated.
pub fn hdr_clip(img: &RgbImage, gain: f32, desat: f32) -> RgbImage {
    map_px(img, |l| {
        let l = [l[0] * gain, l[1] * gain, l[2] * gain];
        let y = 0.2126 * l[0] + 0.7152 * l[1] + 0.0722 * l[2];
        [
            l[0] * (1.0 - desat) + y * desat,
            l[1] * (1.0 - desat) + y * desat,
            l[2] * (1.0 - desat) + y * desat,
        ]
    })
}

/// PQ-encoded signal shown as if it were sRGB: flat and grey.
pub fn hdr_pq_misread(img: &RgbImage, paper_white: f32) -> RgbImage {
    let (m1, m2, c1, c2, c3) =
        (0.159_301_76f32, 78.84375f32, 0.8359375f32, 18.851_563f32, 18.6875f32);
    let mut out = img.clone();
    for p in out.pixels_mut() {
        for c in 0..3 {
            let l = (srgb_to_lin(p[c]) * paper_white / 10000.0).clamp(0.0, 1.0).powf(m1);
            let pq = ((c1 + c2 * l) / (1.0 + c3 * l)).powf(m2);
            p[c] = (pq.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
        }
    }
    out
}

pub fn gamma(img: &RgbImage, g: f32) -> RgbImage {
    let lut: Vec<u8> = (0..256).map(|v| (255.0 * (v as f32 / 255.0).powf(g) + 0.5) as u8).collect();
    let mut out = img.clone();
    for p in out.pixels_mut() {
        for c in 0..3 {
            p[c] = lut[p[c] as usize];
        }
    }
    out
}

pub fn rescale(img: &RgbImage, h: u32) -> RgbImage {
    let w = (img.width() as f64 * h as f64 / img.height() as f64).round() as u32;
    let filter = if h < img.height() {
        imageops::FilterType::Triangle
    } else {
        imageops::FilterType::CatmullRom
    };
    imageops::resize(img, w, h, filter)
}

/// Centre the frame in a wider one with dim blurred filler, as a 16:9 UI sits on an ultrawide screen.
pub fn pad_wide(img: &RgbImage, aspect: f32) -> RgbImage {
    let (w, h) = (img.width(), img.height());
    let wn = (h as f32 * aspect).round() as u32;
    if wn <= w {
        return img.clone();
    }
    let small =
        imageops::resize(img, (wn / 16).max(2), (h / 16).max(2), imageops::FilterType::Triangle);
    let blurred = imageops::blur(&small, 2.0);
    let mut out = imageops::resize(&blurred, wn, h, imageops::FilterType::Triangle);
    for p in out.pixels_mut() {
        for c in 0..3 {
            p[c] = (p[c] as f32 * 0.6) as u8;
        }
    }
    imageops::replace(&mut out, img, ((wn - w) / 2) as i64, 0);
    out
}

pub fn blur(img: &RgbImage, sigma: f32) -> RgbImage {
    imageops::blur(img, sigma)
}

pub fn jpeg(img: &RgbImage, quality: u8) -> RgbImage {
    let mut buf = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, quality)
        .encode_image(img)
        .expect("jpeg encode");
    image::load_from_memory_with_format(&buf, image::ImageFormat::Jpeg)
        .expect("jpeg decode")
        .to_rgb8()
}

pub fn noise(img: &RgbImage, sigma: f32, seed: u64) -> RgbImage {
    let mut s = seed | 1;
    let mut next = || {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        (s >> 40) as f32 / (1u64 << 24) as f32
    };
    let mut out = img.clone();
    for p in out.pixels_mut() {
        for c in 0..3 {
            // Box-Muller
            let (u1, u2) = (next().max(1e-7), next());
            let n = (-2.0 * u1.ln()).sqrt() * (std::f32::consts::TAU * u2).cos();
            p[c] = (p[c] as f32 + n * sigma).clamp(0.0, 255.0) as u8;
        }
    }
    out
}

pub type Perturb = (&'static str, Box<dyn Fn(&RgbImage) -> RgbImage>);

pub fn suite(full: bool) -> Vec<Perturb> {
    let mut v: Vec<Perturb> = vec![("native", Box::new(|i: &RgbImage| i.clone()))];
    if !full {
        return v;
    }
    for h in [720u32, 900, 1080, 1440, 2160] {
        let name: &'static str = Box::leak(format!("res_{h}p").into_boxed_str());
        v.push((name, Box::new(move |i| rescale(i, h))));
    }
    v.push(("wide_21:9", Box::new(|i| pad_wide(i, 21.0 / 9.0))));
    v.push(("wide_32:9", Box::new(|i| pad_wide(i, 32.0 / 9.0))));
    v.push(("wide_32:9@1440", Box::new(|i| pad_wide(&rescale(i, 1440), 32.0 / 9.0))));
    v.push(("hdr_clip_x2", Box::new(|i| hdr_clip(i, 2.0, 0.25))));
    v.push(("hdr_clip_x4", Box::new(|i| hdr_clip(i, 4.0, 0.25))));
    v.push(("hdr_clip_x8", Box::new(|i| hdr_clip(i, 8.0, 0.4))));
    v.push(("hdr_pq_misread", Box::new(|i| hdr_pq_misread(i, 200.0))));
    v.push(("gamma_0.6", Box::new(|i| gamma(i, 0.6))));
    v.push(("gamma_1.8", Box::new(|i| gamma(i, 1.8))));
    v.push(("blur_1.2", Box::new(|i| blur(i, 1.2))));
    v.push(("jpeg_q30", Box::new(|i| jpeg(i, 30))));
    v.push(("noise_s8", Box::new(|i| noise(i, 8.0, 7))));
    v.push(("720p+blur+jpeg", Box::new(|i| jpeg(&blur(&rescale(i, 720), 0.8), 40))));
    v
}

pub fn save(img: &RgbImage, path: &Path) -> Result<(), String> {
    img.save(path).map_err(|e| e.to_string())
}

#[allow(dead_code)]
pub fn blank(w: u32, h: u32) -> RgbImage {
    ImageBuffer::from_pixel(w, h, Rgb([0, 0, 0]))
}
