//! Image primitives for the reader, on single-channel planes of V = max(R, G, B), which separates bright text whatever its hue.

/// Pixel layout of a captured frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PixelFormat {
    Bgra8,
    Rgba8,
    Rgb8,
    Gray8,
}

impl PixelFormat {
    pub fn bytes_per_pixel(self) -> usize {
        match self {
            PixelFormat::Bgra8 | PixelFormat::Rgba8 => 4,
            PixelFormat::Rgb8 => 3,
            PixelFormat::Gray8 => 1,
        }
    }
}

/// A borrowed frame as it comes from a capture API or an image decoder.
#[derive(Clone, Copy, Debug)]
pub struct Frame<'a> {
    pub width: usize,
    pub height: usize,
    /// Bytes per row (>= width * bytes_per_pixel).
    pub stride: usize,
    pub format: PixelFormat,
    pub data: &'a [u8],
}

impl<'a> Frame<'a> {
    pub fn new(width: usize, height: usize, format: PixelFormat, data: &'a [u8]) -> Self {
        Frame { width, height, stride: width * format.bytes_per_pixel(), format, data }
    }

    /// Red, green and blue of one pixel; grey frames give the same value three times.
    pub fn rgb(&self, x: usize, y: usize) -> [u8; 3] {
        let p = &self.data[y * self.stride + x * self.format.bytes_per_pixel()..];
        match self.format {
            PixelFormat::Bgra8 => [p[2], p[1], p[0]],
            PixelFormat::Rgba8 | PixelFormat::Rgb8 => [p[0], p[1], p[2]],
            PixelFormat::Gray8 => [p[0], p[0], p[0]],
        }
    }

    pub fn is_valid(&self) -> bool {
        self.width > 0
            && self.height > 0
            && self.stride >= self.width * self.format.bytes_per_pixel()
            && self.data.len()
                >= self.stride * (self.height - 1) + self.width * self.format.bytes_per_pixel()
    }

    /// V plane max-pooled over `d x d` blocks, which keeps thin bright strokes; from `d = 4` up only every second pixel is sampled.
    pub fn v_pooled(&self, d: usize) -> Plane {
        self.v_pooled_with(d, if d >= 4 { 2 } else { 1 })
    }

    /// [`Frame::v_pooled`] with an explicit sampling step inside each block (1 = every pixel).
    pub fn v_pooled_with(&self, d: usize, step: usize) -> Plane {
        let d = d.max(1);
        let step = step.clamp(1, d);
        let (ow, oh) = (self.width / d, self.height / d);
        let bpp = self.format.bytes_per_pixel();
        let mut out = vec![0u8; ow * oh];
        for oy in 0..oh {
            let dst = &mut out[oy * ow..(oy + 1) * ow];
            let mut dy = 0;
            while dy < d {
                let src = &self.data[(oy * d + dy) * self.stride..];
                match self.format {
                    PixelFormat::Gray8 => {
                        for (ox, slot) in dst.iter_mut().enumerate() {
                            let base = ox * d;
                            let mut m = *slot;
                            let mut dx = 0;
                            while dx < d {
                                m = m.max(src[base + dx]);
                                dx += step;
                            }
                            *slot = m;
                        }
                    }
                    _ => {
                        for (ox, slot) in dst.iter_mut().enumerate() {
                            let base = ox * d;
                            let mut m = *slot;
                            let mut dx = 0;
                            while dx < d {
                                let p = (base + dx) * bpp;
                                m = m.max(src[p]).max(src[p + 1]).max(src[p + 2]);
                                dx += step;
                            }
                            *slot = m;
                        }
                    }
                }
                dy += step;
            }
        }
        Plane { w: ow, h: oh, data: out }
    }

    /// Full-resolution V crop. Coordinates are clamped to the frame; may return an empty plane.
    pub fn v_crop(&self, x0: i64, y0: i64, x1: i64, y1: i64) -> Plane {
        let x0 = x0.clamp(0, self.width as i64) as usize;
        let x1 = x1.clamp(0, self.width as i64) as usize;
        let y0 = y0.clamp(0, self.height as i64) as usize;
        let y1 = y1.clamp(0, self.height as i64) as usize;
        if x1 <= x0 || y1 <= y0 {
            return Plane { w: 0, h: 0, data: Vec::new() };
        }
        let (w, h) = (x1 - x0, y1 - y0);
        let bpp = self.format.bytes_per_pixel();
        let mut out = vec![0u8; w * h];
        for y in 0..h {
            let src = &self.data[(y0 + y) * self.stride + x0 * bpp..];
            let dst = &mut out[y * w..(y + 1) * w];
            match self.format {
                PixelFormat::Gray8 => dst.copy_from_slice(&src[..w]),
                _ => {
                    for (x, px) in src[..w * bpp].chunks_exact(bpp).enumerate() {
                        dst[x] = px[0].max(px[1]).max(px[2]);
                    }
                }
            }
        }
        Plane { w, h, data: out }
    }
}

/// Owned single-channel image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plane {
    pub w: usize,
    pub h: usize,
    pub data: Vec<u8>,
}

impl Plane {
    pub fn new(w: usize, h: usize) -> Self {
        Plane { w, h, data: vec![0; w * h] }
    }

    pub fn is_empty(&self) -> bool {
        self.w == 0 || self.h == 0
    }

    #[inline]
    pub fn at(&self, x: usize, y: usize) -> u8 {
        self.data[y * self.w + x]
    }

    pub fn row(&self, y: usize) -> &[u8] {
        &self.data[y * self.w..(y + 1) * self.w]
    }

    /// Sub-plane copy, clamped.
    pub fn crop(&self, x0: i64, y0: i64, x1: i64, y1: i64) -> Plane {
        let x0 = x0.clamp(0, self.w as i64) as usize;
        let x1 = x1.clamp(0, self.w as i64) as usize;
        let y0 = y0.clamp(0, self.h as i64) as usize;
        let y1 = y1.clamp(0, self.h as i64) as usize;
        if x1 <= x0 || y1 <= y0 {
            return Plane { w: 0, h: 0, data: Vec::new() };
        }
        let (w, h) = (x1 - x0, y1 - y0);
        let mut data = Vec::with_capacity(w * h);
        for y in y0..y1 {
            data.extend_from_slice(&self.data[y * self.w + x0..y * self.w + x1]);
        }
        Plane { w, h, data }
    }

    /// Max-pool by an integer factor.
    pub fn max_pool(&self, f: usize) -> Plane {
        let f = f.max(1);
        if f == 1 {
            return self.clone();
        }
        let (ow, oh) = (self.w / f, self.h / f);
        let mut out = Plane::new(ow, oh);
        for oy in 0..oh {
            let dst = &mut out.data[oy * ow..(oy + 1) * ow];
            for dy in 0..f {
                let src = &self.data[(oy * f + dy) * self.w..];
                for (ox, slot) in dst.iter_mut().enumerate() {
                    let mut m = *slot;
                    for &v in &src[ox * f..ox * f + f] {
                        m = m.max(v);
                    }
                    *slot = m;
                }
            }
        }
        out
    }

    /// Value at percentile `p` in `0..=1` (histogram based).
    pub fn percentile(&self, p: f64) -> u8 {
        let mut hist = [0u32; 256];
        for &v in &self.data {
            hist[v as usize] += 1;
        }
        let target = ((self.data.len() as f64) * p).ceil().max(1.0) as u64;
        let mut acc = 0u64;
        for (i, &c) in hist.iter().enumerate() {
            acc += c as u64;
            if acc >= target {
                return i as u8;
            }
        }
        255
    }

    pub fn min_max(&self) -> (u8, u8) {
        let (mut lo, mut hi) = (255u8, 0u8);
        for &v in &self.data {
            lo = lo.min(v);
            hi = hi.max(v);
        }
        (lo, hi)
    }

    /// Bilinear resize to an exact size.
    pub fn resize_bilinear(&self, nw: usize, nh: usize) -> Plane {
        let mut out = Plane::new(nw, nh);
        if self.is_empty() || nw == 0 || nh == 0 {
            return out;
        }
        let sx = self.w as f32 / nw as f32;
        let sy = self.h as f32 / nh as f32;
        for y in 0..nh {
            let fy = ((y as f32 + 0.5) * sy - 0.5).clamp(0.0, (self.h - 1) as f32);
            let y0 = fy.floor() as usize;
            let y1 = (y0 + 1).min(self.h - 1);
            let wy = fy - y0 as f32;
            for x in 0..nw {
                let fx = ((x as f32 + 0.5) * sx - 0.5).clamp(0.0, (self.w - 1) as f32);
                let x0 = fx.floor() as usize;
                let x1 = (x0 + 1).min(self.w - 1);
                let wx = fx - x0 as f32;
                let a = self.at(x0, y0) as f32 * (1.0 - wx) + self.at(x1, y0) as f32 * wx;
                let b = self.at(x0, y1) as f32 * (1.0 - wx) + self.at(x1, y1) as f32 * wx;
                out.data[y * nw + x] = (a * (1.0 - wy) + b * wy + 0.5) as u8;
            }
        }
        out
    }

    /// Area-weighted resample of `[x0, x1) x [y0, y1)` to `nw x nh` floats by exact box integration; fractional bounds allowed.
    pub fn resample_area(
        &self,
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        nw: usize,
        nh: usize,
    ) -> Vec<f32> {
        let mut out = vec![0f32; nw * nh];
        if self.is_empty() || x1 <= x0 || y1 <= y0 {
            return out;
        }
        let weights = |a: f32, b: f32, n: usize, limit: usize| -> Vec<(usize, usize, Vec<f32>)> {
            let step = (b - a) / n as f32;
            (0..n)
                .map(|i| {
                    let lo = (a + i as f32 * step).max(0.0);
                    let hi = (a + (i + 1) as f32 * step).min(limit as f32);
                    let s = lo.floor() as usize;
                    let e = (hi.ceil() as usize).min(limit).max(s + 1).min(limit.max(1));
                    let mut w: Vec<f32> = (s..e)
                        .map(|p| ((p + 1) as f32).min(hi) - (p as f32).max(lo))
                        .map(|v| v.max(0.0))
                        .collect();
                    let sum: f32 = w.iter().sum();
                    if sum > 0.0 {
                        w.iter_mut().for_each(|v| *v /= sum);
                    }
                    (s.min(limit.saturating_sub(1)), e, w)
                })
                .collect()
        };
        let wx = weights(x0, x1, nw, self.w);
        let wy = weights(y0, y1, nh, self.h);
        for (oy, (ys, _, wyv)) in wy.iter().enumerate() {
            for (ox, (xs, _, wxv)) in wx.iter().enumerate() {
                let mut acc = 0f32;
                for (j, &wv) in wyv.iter().enumerate() {
                    let row = &self.data[(ys + j).min(self.h - 1) * self.w..];
                    let mut racc = 0f32;
                    for (i, &wu) in wxv.iter().enumerate() {
                        racc += row[(xs + i).min(self.w - 1)] as f32 * wu;
                    }
                    acc += racc * wv;
                }
                out[oy * nw + ox] = acc;
            }
        }
        out
    }
}

/// Sliding minimum or maximum over a centred odd window `k` along x, built from power-of-two windows so each pass vectorises.
fn slide_x(src: &Plane, k: usize, is_min: bool) -> Plane {
    if k <= 1 || src.is_empty() {
        return src.clone();
    }
    let r = k / 2;
    let (w, h) = (src.w, src.h);
    let wp = w + 2 * r;
    let ident = if is_min { 255u8 } else { 0u8 };
    let mut a = vec![ident; wp * h];
    for y in 0..h {
        a[y * wp + r..y * wp + r + w].copy_from_slice(src.row(y));
    }
    let mut b = a.clone();
    let mut p = 1usize;
    while 2 * p <= k {
        for y in 0..h {
            let (ra, rb) = (&a[y * wp..(y + 1) * wp], &mut b[y * wp..(y + 1) * wp]);
            let n = wp - p;
            let (lo, hi) = (&ra[..n], &ra[p..p + n]);
            if is_min {
                for i in 0..n {
                    rb[i] = lo[i].min(hi[i]);
                }
            } else {
                for i in 0..n {
                    rb[i] = lo[i].max(hi[i]);
                }
            }
            rb[n..].copy_from_slice(&ra[n..]);
        }
        std::mem::swap(&mut a, &mut b);
        p *= 2;
    }
    let mut out = Plane::new(w, h);
    let off = k - p;
    for y in 0..h {
        let ra = &a[y * wp..(y + 1) * wp];
        let dst = &mut out.data[y * w..(y + 1) * w];
        let (lo, hi) = (&ra[..w], &ra[off..off + w]);
        if is_min {
            for i in 0..w {
                dst[i] = lo[i].min(hi[i]);
            }
        } else {
            for i in 0..w {
                dst[i] = lo[i].max(hi[i]);
            }
        }
    }
    out
}

/// Same as [`slide_x`] along y. Works on whole rows at a time.
fn slide_y(src: &Plane, k: usize, is_min: bool) -> Plane {
    if k <= 1 || src.is_empty() {
        return src.clone();
    }
    let r = k / 2;
    let (w, h) = (src.w, src.h);
    let hp = h + 2 * r;
    let ident = if is_min { 255u8 } else { 0u8 };
    let mut a = vec![ident; w * hp];
    a[r * w..(r + h) * w].copy_from_slice(&src.data);
    let mut b = a.clone();
    let mut p = 1usize;
    while 2 * p <= k {
        let n = (hp - p) * w;
        {
            let (lo, hi) = (&a[..n], &a[p * w..p * w + n]);
            let dst = &mut b[..n];
            if is_min {
                for i in 0..n {
                    dst[i] = lo[i].min(hi[i]);
                }
            } else {
                for i in 0..n {
                    dst[i] = lo[i].max(hi[i]);
                }
            }
        }
        b[n..].copy_from_slice(&a[n..]);
        std::mem::swap(&mut a, &mut b);
        p *= 2;
    }
    let mut out = Plane::new(w, h);
    let off = (k - p) * w;
    let n = w * h;
    let (lo, hi) = (&a[..n], &a[off..off + n]);
    if is_min {
        for i in 0..n {
            out.data[i] = lo[i].min(hi[i]);
        }
    } else {
        for i in 0..n {
            out.data[i] = lo[i].max(hi[i]);
        }
    }
    out
}

/// White top-hat with a `k x k` square, `v - open(v)`: solid bright blocks go to zero, thin strokes keep their contrast.
pub fn top_hat(v: &Plane, k: usize) -> Plane {
    let k = k | 1;
    let eroded = slide_y(&slide_x(v, k, true), k, true);
    let opened = slide_y(&slide_x(&eroded, k, false), k, false);
    let mut out = Plane::new(v.w, v.h);
    for i in 0..v.data.len() {
        out.data[i] = v.data[i].saturating_sub(opened.data[i]);
    }
    out
}

/// Local contrast against a `k x k` box mean; cheaper than `top_hat`, but solid bars leave bands along their edges.
pub fn box_contrast(v: &Plane, k: usize) -> Plane {
    let k = k | 1;
    let r = k / 2;
    let (w, h) = (v.w, v.h);
    let mut out = Plane::new(w, h);
    if v.is_empty() {
        return out;
    }
    let mut hs = vec![0u32; w * h];
    let mut cnt_x = vec![0u32; w];
    for (x, c) in cnt_x.iter_mut().enumerate() {
        *c = ((x + r).min(w - 1) - x.saturating_sub(r) + 1) as u32;
    }
    for y in 0..h {
        let row = v.row(y);
        let o = &mut hs[y * w..(y + 1) * w];
        let mut acc = 0u32;
        for &px in row.iter().take(r.min(w)) {
            acc += px as u32;
        }
        for x in 0..w {
            if x + r < w {
                acc += row[x + r] as u32;
            }
            if x > r {
                acc -= row[x - r - 1] as u32;
            }
            o[x] = acc;
        }
    }
    let mut col = vec![0u32; w];
    for y in 0..r.min(h) {
        for x in 0..w {
            col[x] += hs[y * w + x];
        }
    }
    for y in 0..h {
        if y + r < h {
            let a = &hs[(y + r) * w..(y + r + 1) * w];
            for x in 0..w {
                col[x] += a[x];
            }
        }
        if y > r {
            let a = &hs[(y - r - 1) * w..(y - r) * w];
            for x in 0..w {
                col[x] -= a[x];
            }
        }
        let cnt_y = ((y + r).min(h - 1) - y.saturating_sub(r) + 1) as u32;
        let vr = v.row(y);
        let o = &mut out.data[y * w..(y + 1) * w];
        for x in 0..w {
            let mean = col[x] / (cnt_x[x] * cnt_y);
            o[x] = vr[x].saturating_sub(mean as u8);
        }
    }
    out
}

/// Binary mask: 1 where `p > cut`.
pub fn threshold(p: &Plane, cut: u8) -> Plane {
    Plane { w: p.w, h: p.h, data: p.data.iter().map(|&v| (v > cut) as u8).collect() }
}

/// Connected component summary.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Blob {
    pub x0: usize,
    pub y0: usize,
    /// Exclusive.
    pub x1: usize,
    /// Exclusive.
    pub y1: usize,
    pub area: usize,
}

impl Blob {
    pub fn w(&self) -> usize {
        self.x1 - self.x0
    }
    pub fn h(&self) -> usize {
        self.y1 - self.y0
    }
    pub fn cx(&self) -> f32 {
        (self.x0 + self.x1) as f32 / 2.0
    }
    pub fn cy(&self) -> f32 {
        (self.y0 + self.y1) as f32 / 2.0
    }
}

/// 8-connected components of a mask; horizontal gaps up to `bridge` count as connected, joining the two characters of a code.
pub fn components(mask: &Plane, bridge: usize) -> Vec<Blob> {
    struct Run {
        x0: usize,
        x1: usize, // exclusive
        label: usize,
    }
    fn find(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    let (w, h) = (mask.w, mask.h);
    let mut parent: Vec<usize> = Vec::new();
    let mut blobs: Vec<Blob> = Vec::new();
    let mut prev: Vec<Run> = Vec::new();
    for y in 0..h {
        let row = mask.row(y);
        let mut cur: Vec<Run> = Vec::new();
        let mut x = 0;
        while x < w {
            if row[x] == 0 {
                x += 1;
                continue;
            }
            let start = x;
            let mut end = x;
            let mut area = 0usize;
            // extend across gaps <= bridge
            loop {
                while end < w && row[end] != 0 {
                    end += 1;
                    area += 1;
                }
                let mut g = end;
                while g < w && g - end < bridge && row[g] == 0 {
                    g += 1;
                }
                if g < w && g - end <= bridge && g > end && row[g] != 0 {
                    end = g;
                } else {
                    break;
                }
            }
            let label = parent.len();
            parent.push(label);
            blobs.push(Blob { x0: start, y0: y, x1: end, y1: y + 1, area });
            for p in &prev {
                if p.x0 <= end && start <= p.x1 {
                    let (a, b) = (find(&mut parent, p.label), find(&mut parent, label));
                    if a != b {
                        let (keep, drop) = if a < b { (a, b) } else { (b, a) };
                        parent[drop] = keep;
                    }
                }
            }
            cur.push(Run { x0: start, x1: end, label });
            x = end;
        }
        prev = cur;
    }
    let mut merged: Vec<Option<Blob>> = vec![None; blobs.len()];
    for (i, &b) in blobs.iter().enumerate() {
        let root = find(&mut parent, i);
        merged[root] = Some(match merged[root] {
            None => b,
            Some(m) => Blob {
                x0: m.x0.min(b.x0),
                y0: m.y0.min(b.y0),
                x1: m.x1.max(b.x1),
                y1: m.y1.max(b.y1),
                area: m.area + b.area,
            },
        });
    }
    merged.into_iter().flatten().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noise(w: usize, h: usize, seed: u64) -> Plane {
        let mut s = seed;
        let data = (0..w * h)
            .map(|_| {
                s ^= s << 13;
                s ^= s >> 7;
                s ^= s << 17;
                (s >> 32) as u8
            })
            .collect();
        Plane { w, h, data }
    }

    fn brute_slide(p: &Plane, k: usize, is_min: bool, along_x: bool) -> Plane {
        let r = (k / 2) as i64;
        let mut out = Plane::new(p.w, p.h);
        for y in 0..p.h as i64 {
            for x in 0..p.w as i64 {
                let mut acc: Option<u8> = None;
                for d in -r..=r {
                    let (xx, yy) = if along_x { (x + d, y) } else { (x, y + d) };
                    if xx < 0 || yy < 0 || xx >= p.w as i64 || yy >= p.h as i64 {
                        continue;
                    }
                    let v = p.at(xx as usize, yy as usize);
                    acc = Some(match acc {
                        None => v,
                        Some(a) => {
                            if is_min {
                                a.min(v)
                            } else {
                                a.max(v)
                            }
                        }
                    });
                }
                out.data[(y as usize) * p.w + x as usize] = acc.unwrap();
            }
        }
        out
    }

    #[test]
    fn sliding_min_max_match_brute_force() {
        let p = noise(37, 23, 99);
        for k in [1usize, 3, 5, 7, 9, 15, 17, 21, 31] {
            for is_min in [true, false] {
                assert_eq!(
                    slide_x(&p, k, is_min),
                    brute_slide(&p, k, is_min, true),
                    "x k={k} min={is_min}"
                );
                assert_eq!(
                    slide_y(&p, k, is_min),
                    brute_slide(&p, k, is_min, false),
                    "y k={k} min={is_min}"
                );
            }
        }
    }

    #[test]
    fn top_hat_removes_bars_and_keeps_strokes() {
        let mut p = Plane::new(80, 60);
        p.data.iter_mut().for_each(|v| *v = 20);
        for y in 5..25 {
            for x in 5..75 {
                p.data[y * 80 + x] = 230; // solid bar 70x20
            }
        }
        for y in 35..50 {
            p.data[y * 80 + 40] = 230; // 1px stroke
            p.data[y * 80 + 41] = 230;
        }
        let t = top_hat(&p, 11);
        assert_eq!(t.at(40, 15), 0, "bar interior must vanish");
        assert!(t.at(40, 42) > 200, "stroke must survive");
        assert_eq!(t.at(10, 55), 0);
    }

    #[test]
    fn box_contrast_is_zero_on_flat_regions() {
        let mut p = Plane::new(40, 40);
        p.data.iter_mut().for_each(|v| *v = 77);
        assert!(box_contrast(&p, 9).data.iter().all(|&v| v == 0));
        p.data[20 * 40 + 20] = 255;
        assert!(box_contrast(&p, 9).at(20, 20) > 150);
    }

    #[test]
    fn components_bridge_horizontal_gaps_only() {
        // two 3x3 squares 2px apart horizontally, one more 2px below
        let mut m = Plane::new(20, 12);
        let mut put = |x0: usize, y0: usize| {
            for y in y0..y0 + 3 {
                for x in x0..x0 + 3 {
                    m.data[y * 20 + x] = 1;
                }
            }
        };
        put(2, 1);
        put(7, 1);
        put(2, 6);
        assert_eq!(components(&m, 0).len(), 3);
        let b = components(&m, 2);
        assert_eq!(b.len(), 2);
        let wide = b.iter().find(|b| b.w() == 8).unwrap();
        assert_eq!((wide.x0, wide.x1, wide.y0, wide.y1, wide.area), (2, 10, 1, 4, 18));
    }

    #[test]
    fn components_merge_u_shapes() {
        // a U: two verticals joined at the bottom must be one blob
        let mut m = Plane::new(9, 6);
        for y in 0..5 {
            m.data[y * 9 + 1] = 1;
            m.data[y * 9 + 6] = 1;
        }
        for x in 1..7 {
            m.data[5 * 9 + x] = 1;
        }
        let b = components(&m, 0);
        assert_eq!(b.len(), 1);
        assert_eq!((b[0].x0, b[0].x1, b[0].y0, b[0].y1), (1, 7, 0, 6));
    }

    #[test]
    fn frame_pooling_and_crops() {
        // 4x2 BGRA, one bright pixel
        let mut data = vec![10u8; 4 * 2 * 4];
        data[(4 + 2) * 4 + 1] = 200; // (x=2,y=1) green
        let f = Frame::new(4, 2, PixelFormat::Bgra8, &data);
        assert!(f.is_valid());
        assert_eq!(f.v_pooled(1).data, vec![10, 10, 10, 10, 10, 10, 200, 10]);
        assert_eq!(f.v_pooled(2).data, vec![10, 200]);
        assert_eq!(f.v_crop(1, 1, 4, 2).data, vec![10, 200, 10]);
        assert!(f.v_crop(3, 0, 1, 2).is_empty());
    }

    #[test]
    fn max_pool_matches_pooling_the_frame_directly() {
        let p = noise(24, 12, 3);
        let f = Frame::new(24, 12, PixelFormat::Gray8, &p.data);
        assert_eq!(f.v_pooled(2).max_pool(2), {
            // d = 4 samples every second pixel, so compare against a full 4x4 pool of the plane
            let full = p.max_pool(4);
            let sampled = f.v_pooled(2).max_pool(2);
            assert_eq!(full, sampled);
            sampled
        });
        assert_eq!(p.max_pool(1), p);
    }

    #[test]
    fn area_resample_preserves_mean() {
        let p = noise(13, 9, 5);
        let mean: f32 = p.data.iter().map(|&v| v as f32).sum::<f32>() / p.data.len() as f32;
        let r = p.resample_area(0.0, 0.0, 13.0, 9.0, 5, 4);
        let m2: f32 = r.iter().sum::<f32>() / r.len() as f32;
        assert!((mean - m2).abs() < 1.0, "{mean} vs {m2}");
        // identity
        let id = p.resample_area(0.0, 0.0, 13.0, 9.0, 13, 9);
        for (a, b) in id.iter().zip(&p.data) {
            assert!((a - *b as f32).abs() < 0.01);
        }
    }
}
