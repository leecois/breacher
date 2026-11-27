//! Glyph recognition: per frame, crops are normalised, grouped by mutual similarity, then mapped to tokens against the templates.

use crate::image::Plane;

pub const GW: usize = 32;
pub const GH: usize = 24;
pub const GN: usize = GW * GH;
pub const NCODES: usize = 6;
/// Marker for "no token assigned".
pub const UNKNOWN: u8 = 255;

/// Column ranges of the two characters after normalisation (a small centre strip is ignored).
const LEFT: std::ops::Range<usize> = 0..14;
const RIGHT: std::ops::Range<usize> = 18..GW;

pub type Feat = Vec<f32>;

#[inline]
pub fn dot(a: &[f32], b: &[f32]) -> f32 {
    let mut acc = [0f32; 8];
    let n = a.len().min(b.len());
    let chunks = n / 8;
    for c in 0..chunks {
        let (x, y) = (&a[c * 8..c * 8 + 8], &b[c * 8..c * 8 + 8]);
        for i in 0..8 {
            acc[i] += x[i] * y[i];
        }
    }
    let mut s: f32 = acc.iter().sum();
    for i in chunks * 8..n {
        s += a[i] * b[i];
    }
    s
}

fn unit(v: &mut [f32]) -> bool {
    let mean = v.iter().sum::<f32>() / v.len() as f32;
    let mut n2 = 0f32;
    for x in v.iter_mut() {
        *x -= mean;
        n2 += *x * *x;
    }
    if n2 < 1e-9 {
        return false;
    }
    let inv = 1.0 / n2.sqrt();
    v.iter_mut().for_each(|x| *x *= inv);
    true
}

fn half(v: &[f32], cols: std::ops::Range<usize>) -> Vec<f32> {
    let mut out = Vec::with_capacity(cols.len() * GH);
    for y in 0..GH {
        out.extend_from_slice(&v[y * GW + cols.start..y * GW + cols.end]);
    }
    unit(&mut out);
    out
}

/// Normalises a crop holding one code (bright strokes on a darker ground); `None` when it holds no usable strokes.
pub fn featurize(crop: &Plane) -> Option<Feat> {
    if crop.w < 3 || crop.h < 3 {
        return None;
    }
    // small glyphs: work on an enlarged copy so the bounding box is sub-pixel accurate
    let up;
    let src = if crop.h < 40 {
        up = crop.resize_bilinear(crop.w * 3, crop.h * 3);
        &up
    } else {
        crop
    };
    // background and stroke level of this crop (the window is mostly background)
    let lo = src.percentile(0.10) as f32;
    let hi = src.percentile(0.995) as f32;
    if hi - lo < 12.0 {
        return None;
    }
    let cut = lo + 0.45 * (hi - lo);
    let (w, h) = (src.w, src.h);
    let mut col_on = vec![0u32; w];
    for y in 0..h {
        let row = src.row(y);
        for x in 0..w {
            if row[x] as f32 > cut {
                col_on[x] += 1;
            }
        }
    }
    // strokes touching the window edge belong to a neighbour or a rule: drop them as long as something else remains
    fn span(on: &[u32]) -> Option<(usize, usize)> {
        let n = on.len();
        let mut groups: Vec<(usize, usize, u32)> = Vec::new(); // start, end (exclusive), mass
        let mut i = 0;
        while i < n {
            if on[i] == 0 {
                i += 1;
                continue;
            }
            let start = i;
            let mut mass = 0;
            let mut end = i;
            // a group ends at a gap of two or more empty lines
            while i < n && (on[i] > 0 || (i + 1 < n && on[i + 1] > 0)) {
                if on[i] > 0 {
                    mass += on[i];
                    end = i + 1;
                }
                i += 1;
            }
            groups.push((start, end, mass));
        }
        // an intruder is a thin sliver at the very edge; a wide edge-touching group is the code itself, off-centre
        let sliver = (0.12 * n as f32).max(2.0) as usize;
        let inner: Vec<&(usize, usize, u32)> =
            groups.iter().filter(|g| !((g.0 == 0 || g.1 == n) && g.1 - g.0 <= sliver)).collect();
        let keep: Vec<&(usize, usize, u32)> = if inner.iter().map(|g| g.2).sum::<u32>() >= 6 {
            inner
        } else {
            groups.iter().collect()
        };
        let lo = keep.iter().map(|g| g.0).min()?;
        let hi = keep.iter().map(|g| g.1).max()?;
        Some((lo, hi))
    }
    let (x0, x1) = span(&col_on)?;
    // rows: recount inside the kept columns so a dropped neighbour does not stretch the box
    let mut row_in = vec![0u32; h];
    for (y, count) in row_in.iter_mut().enumerate() {
        *count = src.row(y)[x0..x1].iter().filter(|&&v| v as f32 > cut).count() as u32;
    }
    let (y0, y1) = span(&row_in)?;
    if x1 - x0 < 3 || y1 - y0 < 3 {
        return None;
    }
    // soft binarisation between 30% and 70% of the local range: features follow stroke position, not tone or background
    let (a, b) = (lo + 0.30 * (hi - lo), lo + 0.70 * (hi - lo));
    let scale = 255.0 / (b - a);
    let mut soft = Plane::new(x1 - x0, y1 - y0);
    for y in y0..y1 {
        let row = &src.row(y)[x0..x1];
        let dst = &mut soft.data[(y - y0) * (x1 - x0)..(y - y0 + 1) * (x1 - x0)];
        for (d, &v) in dst.iter_mut().zip(row) {
            *d = ((v as f32 - a) * scale).clamp(0.0, 255.0) as u8;
        }
    }
    let mut f = soft.resample_area(0.0, 0.0, soft.w as f32, soft.h as f32, GW, GH);
    if !unit(&mut f) {
        return None;
    }
    Some(f)
}

/// One template per token for one rendering style (font, size class).
#[derive(Clone, Debug)]
pub struct TemplateSet {
    pub name: String,
    /// `NCODES` entries; `None` when the style has no sample of that token.
    pub codes: Vec<Option<Feat>>,
}

#[derive(Clone, Debug, Default)]
pub struct Templates {
    pub sets: Vec<TemplateSet>,
}

impl Templates {
    /// Templates shipped with the crate (learned from labelled captures plus font renders).
    pub fn builtin() -> Templates {
        let mut sets = Vec::new();
        for (name, data) in crate::templates_data::SETS {
            let codes = data
                .iter()
                .map(|q| {
                    if q.iter().all(|&b| b == 0) {
                        return None;
                    }
                    let mut f: Feat = q.iter().map(|&b| b as f32).collect();
                    unit(&mut f).then_some(f)
                })
                .collect();
            sets.push(TemplateSet { name: name.to_string(), codes });
        }
        Templates { sets }
    }

    /// Best score per token over all sets (`-1` when no set has that token).
    pub fn scores(&self, f: &[f32]) -> [f32; NCODES] {
        let mut out = [-1f32; NCODES];
        for set in &self.sets {
            for (k, t) in set.codes.iter().enumerate().take(NCODES) {
                if let Some(t) = t {
                    let s = dot(f, t);
                    if s > out[k] {
                        out[k] = s;
                    }
                }
            }
        }
        out
    }

    pub fn is_empty(&self) -> bool {
        self.sets.iter().all(|s| s.codes.iter().all(Option::is_none))
    }
}

/// Build a template set by averaging labelled features.
pub fn learn_set(name: &str, samples: &[(u8, Feat)]) -> TemplateSet {
    let mut sums: Vec<Vec<f32>> = vec![vec![0.0; GN]; NCODES];
    let mut counts = [0usize; NCODES];
    for (code, f) in samples {
        let k = *code as usize;
        if k < NCODES {
            for (sum, v) in sums[k].iter_mut().zip(f) {
                *sum += v;
            }
            counts[k] += 1;
        }
    }
    let codes = sums
        .into_iter()
        .zip(counts)
        .map(|(mut s, c)| if c > 0 && unit(&mut s) { Some(s) } else { None })
        .collect();
    TemplateSet { name: name.to_string(), codes }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Label {
    /// Token id, or [`UNKNOWN`].
    pub code: u8,
    /// Similarity to the centroid of its fragment (1.0 for a fragment of one).
    pub consistency: f32,
    /// Template evidence for the fragment's token (mean over the fragment).
    pub evidence: f32,
    /// Evidence for the chosen token minus the best other token the fragment could have taken.
    pub margin: f32,
    /// Own-fragment similarity minus the best similarity to a fragment of a different token.
    pub separation: f32,
    /// Left and right characters, matched separately, both point at this token.
    pub halves_agree: bool,
    /// The similarity structure blocked a token that the evidence clearly prefers.
    pub conflict: bool,
    pub fragment: u8,
    pub fragment_size: u16,
}

impl Label {
    /// Per-glyph acceptance gate.
    pub fn ok(&self, g: &Gate) -> bool {
        self.code != UNKNOWN
            && !self.conflict
            && self.evidence >= g.min_evidence
            && self.margin >= g.min_margin
            && self.halves_agree
            && (self.fragment_size < 2 || self.consistency >= g.min_consistency)
            && self.separation >= g.min_separation
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Gate {
    pub min_evidence: f32,
    pub min_consistency: f32,
    pub min_margin: f32,
    pub min_separation: f32,
}

impl Default for Gate {
    fn default() -> Self {
        Gate { min_evidence: 0.45, min_consistency: 0.6, min_margin: 0.05, min_separation: 0.05 }
    }
}

#[derive(Clone, Debug)]
pub struct GroupLabels {
    pub labels: Vec<Label>,
    /// Unit centroid per token (for cross-checking a second group against this one).
    pub centroids: Vec<Option<Feat>>,
    /// Number of fragments the group split into.
    pub fragments: usize,
    /// Number of distinct tokens assigned.
    pub tokens: usize,
}

/// Glyphs at least this similar share a fragment; set above the highest similarity seen between two different tokens (0.73).
pub const FRAGMENT_SIM: f32 = 0.80;
/// Fragments less similar than this cannot be the same token.
pub const SPLIT_SIM: f32 = 0.50;
/// A fragment whose evidence prefers a blocked token by more than this is a conflict, not a guess.
const CONFLICT_GAP: f32 = 0.12;

struct Fragment {
    sum: Vec<f32>,
    centroid: Vec<f32>,
    members: Vec<usize>,
}

impl Fragment {
    fn refresh(&mut self) {
        self.centroid = self.sum.clone();
        unit(&mut self.centroid);
    }
}

fn fragments(feats: &[Feat]) -> Vec<Fragment> {
    let mut frags: Vec<Fragment> = Vec::new();
    for (g, f) in feats.iter().enumerate() {
        let best = frags
            .iter()
            .enumerate()
            .map(|(i, c)| (i, dot(f, &c.centroid)))
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        match best {
            Some((i, s)) if s >= FRAGMENT_SIM => {
                let c = &mut frags[i];
                for (sum, v) in c.sum.iter_mut().zip(f) {
                    *sum += v;
                }
                c.members.push(g);
                c.refresh();
            }
            _ => frags.push(Fragment { sum: f.clone(), centroid: f.clone(), members: vec![g] }),
        }
    }
    // merge fragments whose centroids drifted together, then reassign; repeat until stable enough
    for _ in 0..3 {
        let mut merged = true;
        while merged {
            merged = false;
            'outer: for i in 0..frags.len() {
                for j in i + 1..frags.len() {
                    if dot(&frags[i].centroid, &frags[j].centroid) >= FRAGMENT_SIM {
                        let c = frags.remove(j);
                        for k in 0..GN {
                            frags[i].sum[k] += c.sum[k];
                        }
                        frags[i].members.extend(c.members);
                        frags[i].refresh();
                        merged = true;
                        break 'outer;
                    }
                }
            }
        }
        let cents: Vec<Vec<f32>> = frags.iter().map(|c| c.centroid.clone()).collect();
        let mut next: Vec<Fragment> = cents
            .iter()
            .map(|_| Fragment { sum: vec![0.0; GN], centroid: Vec::new(), members: Vec::new() })
            .collect();
        for (g, f) in feats.iter().enumerate() {
            let (bi, bs) = cents
                .iter()
                .enumerate()
                .map(|(i, c)| (i, dot(f, c)))
                .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
                .unwrap();
            if bs >= FRAGMENT_SIM {
                for (sum, v) in next[bi].sum.iter_mut().zip(f) {
                    *sum += v;
                }
                next[bi].members.push(g);
            } else {
                next.push(Fragment { sum: f.clone(), centroid: Vec::new(), members: vec![g] });
            }
        }
        next.retain(|c| !c.members.is_empty());
        next.iter_mut().for_each(Fragment::refresh);
        frags = next;
    }
    frags.sort_by(|a, b| {
        b.members.len().cmp(&a.members.len()).then(a.members[0].cmp(&b.members[0]))
    });
    frags
}

/// Labels one group of glyphs: fragments take the tokens with the most template evidence, and unlike fragments never share one.
pub fn label_group(
    feats: &[Feat],
    templates: &Templates,
    prior: [f32; NCODES],
    anchors: Option<&[Option<Feat>]>,
) -> GroupLabels {
    let n = feats.len();
    let frags = fragments(feats);
    let m = frags.len();
    let mut ev = vec![[-1f32; NCODES]; m];
    for (i, c) in frags.iter().enumerate() {
        let mut acc = [0f32; NCODES];
        for &g in &c.members {
            let s = templates.scores(&feats[g]);
            for k in 0..NCODES {
                acc[k] += s[k];
            }
        }
        for k in 0..NCODES {
            let mut e = acc[k] / c.members.len() as f32;
            if e <= -1.0 {
                if let Some(a) = anchors.and_then(|a| a.get(k)).and_then(|a| a.as_ref()) {
                    e = dot(&c.centroid, a);
                }
            }
            ev[i][k] = if e <= -1.0 { -1.0 } else { e + prior[k] };
        }
    }
    let mut sim = vec![vec![1f32; m]; m];
    for i in 0..m {
        for j in i + 1..m {
            let s = dot(&frags[i].centroid, &frags[j].centroid);
            sim[i][j] = s;
            sim[j][i] = s;
        }
    }
    // exhaustive assignment with a bound; the first leaf is the greedy solution, which is almost always optimal
    const UNASSIGNED_COST: f32 = -0.6;
    struct Search<'a> {
        frags: &'a [Fragment],
        ev: &'a [[f32; NCODES]],
        sim: &'a [Vec<f32>],
        best: f32,
        best_labels: Vec<u8>,
        labels: Vec<u8>,
        nodes: u32,
        rest_max: Vec<f32>,
    }
    impl Search<'_> {
        fn feasible(&self, i: usize, k: u8) -> bool {
            (0..i).all(|j| self.labels[j] != k || self.sim[i][j] >= SPLIT_SIM)
        }
        fn go(&mut self, i: usize, total: f32) {
            self.nodes += 1;
            if i == self.frags.len() {
                if total > self.best {
                    self.best = total;
                    self.best_labels = self.labels.clone();
                }
                return;
            }
            if self.nodes > 200_000 || total + self.rest_max[i] <= self.best {
                return;
            }
            let size = self.frags[i].members.len() as f32;
            let mut order: Vec<u8> =
                (0..NCODES as u8).filter(|&k| self.ev[i][k as usize] > -1.0).collect();
            order.sort_by(|&a, &b| {
                self.ev[i][b as usize].partial_cmp(&self.ev[i][a as usize]).unwrap()
            });
            for k in order {
                if self.feasible(i, k) {
                    self.labels[i] = k;
                    self.go(i + 1, total + size * self.ev[i][k as usize]);
                }
            }
            self.labels[i] = UNKNOWN;
            self.go(i + 1, total + size * UNASSIGNED_COST);
        }
    }
    let mut rest_max = vec![0f32; m + 1];
    for i in (0..m).rev() {
        let top = ev[i].iter().cloned().fold(UNASSIGNED_COST, f32::max);
        rest_max[i] = rest_max[i + 1] + frags[i].members.len() as f32 * top;
    }
    let mut search = Search {
        frags: &frags,
        ev: &ev,
        sim: &sim,
        best: f32::NEG_INFINITY,
        best_labels: vec![UNKNOWN; m],
        labels: vec![UNKNOWN; m],
        nodes: 0,
        rest_max,
    };
    search.go(0, 0.0);
    let mapping = search.best_labels;

    // per-fragment diagnostics against the final assignment
    let blocked =
        |i: usize, k: u8| (0..m).any(|j| j != i && mapping[j] == k && sim[i][j] < SPLIT_SIM);
    let mut frag_margin = vec![0f32; m];
    let mut frag_conflict = vec![false; m];
    for i in 0..m {
        let t = mapping[i];
        if t == UNKNOWN {
            continue;
        }
        let own = ev[i][t as usize];
        let mut alt = -1f32;
        for k in 0..NCODES as u8 {
            if k == t || ev[i][k as usize] <= -1.0 {
                continue;
            }
            if blocked(i, k) {
                if ev[i][k as usize] - own > CONFLICT_GAP {
                    frag_conflict[i] = true;
                }
            } else {
                alt = alt.max(ev[i][k as usize]);
            }
        }
        frag_margin[i] = own - alt;
    }
    if std::env::var_os("BREACH_DEBUG_GLYPH").is_some() {
        eprintln!(
            "  group of {n}: {m} fragments, anchors={}, search nodes {}",
            anchors.is_some(),
            search.nodes
        );
        for (i, c) in frags.iter().enumerate() {
            let t = mapping[i];
            let row: Vec<String> =
                (0..NCODES).map(|k| format!("{}:{:.2}", crate::CODES[k], ev[i][k])).collect();
            let near: Vec<String> = (0..m)
                .filter(|&j| j != i && sim[i][j] > 0.45)
                .map(|j| format!("f{j}:{:.2}", sim[i][j]))
                .collect();
            eprintln!(
                "    f{i} size {:>2} -> {} margin {:.2}{} | {} | near {}",
                c.members.len(),
                if t == UNKNOWN { "??" } else { crate::CODES[t as usize] },
                frag_margin[i],
                if frag_conflict[i] { " CONFLICT" } else { "" },
                row.join(" "),
                near.join(" ")
            );
        }
    }

    let lefts: Vec<Vec<f32>> = frags.iter().map(|c| half(&c.centroid, LEFT)).collect();
    let rights: Vec<Vec<f32>> = frags.iter().map(|c| half(&c.centroid, RIGHT)).collect();
    let mut labels = vec![
        Label {
            code: UNKNOWN,
            consistency: 0.0,
            evidence: -1.0,
            margin: 0.0,
            separation: 0.0,
            halves_agree: false,
            conflict: false,
            fragment: 0,
            fragment_size: 0
        };
        n
    ];
    for (i, c) in frags.iter().enumerate() {
        let code = mapping[i];
        for &g in &c.members {
            let f = &feats[g];
            let own = if c.members.len() > 1 { dot(f, &c.centroid) } else { 1.0 };
            let other = (0..m)
                .filter(|&j| mapping[j] != code)
                .map(|j| dot(f, &frags[j].centroid))
                .fold(-1f32, f32::max);
            let halves_agree = {
                let (fl, fr) = (half(f, LEFT), half(f, RIGHT));
                let arg = |hs: &[Vec<f32>], h: &[f32]| {
                    hs.iter()
                        .enumerate()
                        .map(|(j, c)| (j, dot(h, c)))
                        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
                        .map(|x| mapping[x.0])
                };
                arg(&lefts, &fl) == Some(code) && arg(&rights, &fr) == Some(code)
            };
            labels[g] = Label {
                code,
                consistency: own,
                evidence: if code == UNKNOWN { -1.0 } else { ev[i][code as usize] },
                margin: frag_margin[i],
                separation: own - other,
                halves_agree,
                conflict: frag_conflict[i],
                fragment: i as u8,
                fragment_size: c.members.len() as u16,
            };
        }
    }
    let mut sums: Vec<Vec<f32>> = vec![vec![0.0; GN]; NCODES];
    let mut have = [false; NCODES];
    for (i, c) in frags.iter().enumerate() {
        if mapping[i] != UNKNOWN {
            let k = mapping[i] as usize;
            have[k] = true;
            for x in 0..GN {
                sums[k][x] += c.sum[x];
            }
        }
    }
    let centroids = sums
        .into_iter()
        .zip(have)
        .map(|(mut s, h)| if h && unit(&mut s) { Some(s) } else { None })
        .collect();
    GroupLabels { labels, centroids, fragments: m, tokens: have.iter().filter(|&&h| h).count() }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tiny 5x7 bitmap font for the characters used by the six tokens.
    fn glyph_bits(ch: char) -> [u8; 7] {
        match ch {
            '1' => [0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
            'C' => [0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110],
            '5' => [0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110],
            '7' => [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000],
            'A' => [0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
            'B' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110],
            'D' => [0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110],
            'E' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111],
            '9' => [0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00001, 0b01110],
            'F' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000],
            _ => [0; 7],
        }
    }

    /// Render a token at `scale` px per font pixel with a brightness and a background level.
    pub(crate) fn render(code: usize, scale: usize, fg: u8, bg: u8, pad: usize) -> Plane {
        let text: Vec<char> = crate::CODES[code].chars().collect();
        let (w, h) = ((5 + 2 + 5) * scale + 2 * pad, 7 * scale + 2 * pad);
        let mut p = Plane::new(w, h);
        p.data.iter_mut().for_each(|v| *v = bg);
        for (ci, ch) in text.iter().enumerate() {
            let bits = glyph_bits(*ch);
            for (ry, row) in bits.iter().enumerate() {
                for rx in 0..5 {
                    if row >> (4 - rx) & 1 == 1 {
                        for dy in 0..scale {
                            for dx in 0..scale {
                                let x = pad + (ci * 7 + rx) * scale + dx;
                                let y = pad + ry * scale + dy;
                                p.data[y * w + x] = fg;
                            }
                        }
                    }
                }
            }
        }
        p
    }

    fn synthetic_templates() -> Templates {
        let samples: Vec<(u8, Feat)> =
            (0..NCODES).map(|k| (k as u8, featurize(&render(k, 4, 230, 20, 6)).unwrap())).collect();
        Templates { sets: vec![learn_set("synthetic", &samples)] }
    }

    #[test]
    fn features_ignore_scale_exposure_and_padding() {
        let a = featurize(&render(3, 4, 230, 20, 6)).unwrap();
        let b = featurize(&render(3, 2, 120, 60, 3)).unwrap();
        let c = featurize(&render(4, 4, 230, 20, 6)).unwrap();
        assert!(dot(&a, &b) > 0.9, "same token at another scale/exposure: {}", dot(&a, &b));
        assert!(dot(&a, &c) < 0.7, "different tokens: {}", dot(&a, &c));
    }

    #[test]
    fn flat_crops_have_no_feature() {
        let mut p = Plane::new(20, 20);
        p.data.iter_mut().for_each(|v| *v = 90);
        assert!(featurize(&p).is_none());
    }

    #[test]
    fn group_labelling_reads_a_matrix_and_flags_an_outlier() {
        let t = synthetic_templates();
        let truth: Vec<usize> = (0..36).map(|i| (i * 7 + i / 5) % 5).collect();
        let mut feats: Vec<Feat> = truth
            .iter()
            .enumerate()
            .map(|(i, &k)| featurize(&render(k, 3, 200 + (i % 5) as u8 * 10, 25, 4)).unwrap())
            .collect();
        let g = label_group(&feats, &t, [0.0; NCODES], None);
        assert_eq!(g.tokens, 5);
        for (l, &k) in g.labels.iter().zip(&truth) {
            assert_eq!(l.code as usize, k);
            assert!(l.ok(&Gate::default()), "{l:?}");
        }
        // an occluded cell: a box drawn over the glyph
        let mut bad = render(2, 3, 220, 25, 4);
        let (w, h) = (bad.w, bad.h);
        for y in 0..h {
            for x in 0..w {
                if x < 3 || y < 3 || x + 3 >= w || y + 3 >= h || (x + y) % 5 == 0 {
                    bad.data[y * w + x] = 240;
                }
            }
        }
        feats[10] = featurize(&bad).unwrap();
        let g = label_group(&feats, &t, [0.0; NCODES], None);
        assert!(!g.labels[10].ok(&Gate::default()), "outlier must not pass: {:?}", g.labels[10]);
        let good = g
            .labels
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != 10)
            .all(|(i, l)| l.code as usize == truth[i] && l.ok(&Gate::default()));
        assert!(good);
    }

    #[test]
    fn one_to_one_assignment_overrides_weak_templates() {
        // both templates favour BD slightly; the two distinct clusters must still get distinct tokens
        let mut t = synthetic_templates();
        let bd = t.sets[0].codes[3].clone().unwrap();
        let e9 = t.sets[0].codes[4].clone().unwrap();
        let mut blend: Feat = bd.iter().zip(&e9).map(|(a, b)| 0.6 * a + 0.4 * b).collect();
        unit(&mut blend);
        t.sets[0].codes[4] = Some(blend); // E9 template now leans towards BD
        let truth = [3usize, 4, 3, 4, 4, 3, 0, 1];
        let feats: Vec<Feat> =
            truth.iter().map(|&k| featurize(&render(k, 3, 220, 20, 4)).unwrap()).collect();
        let g = label_group(&feats, &t, [0.0; NCODES], None);
        for (l, &k) in g.labels.iter().zip(&truth) {
            assert_eq!(l.code as usize, k);
        }
    }

    #[test]
    fn sequences_fall_back_on_matrix_anchors_without_templates() {
        let t = synthetic_templates();
        let m_truth = [0usize, 1, 2, 4, 0, 1, 2, 4, 0];
        let m: Vec<Feat> =
            m_truth.iter().map(|&k| featurize(&render(k, 4, 230, 20, 5)).unwrap()).collect();
        let gm = label_group(&m, &t, [0.0; NCODES], None);
        let s_truth = [4usize, 0, 2, 1];
        let s: Vec<Feat> =
            s_truth.iter().map(|&k| featurize(&render(k, 2, 250, 30, 3)).unwrap()).collect();
        let gs = label_group(&s, &Templates::default(), [0.0; NCODES], Some(&gm.centroids));
        for (l, &k) in gs.labels.iter().zip(&s_truth) {
            assert_eq!(l.code as usize, k, "anchors alone must be enough");
        }
    }
}
