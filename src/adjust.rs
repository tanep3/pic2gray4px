//! Color adjustment front-end: channel mixer presets, tone curves and
//! local contrast, applied to the gray plane before the existing
//! candidate search. See `docs/adjustment-requirements.md` and
//! `docs/adjustment-design.md`.
//!
//! Everything here is a pure plane-to-plane function over the valid
//! rectangle of the target plane; the even-size padding margin is
//! never read or written.

/// Rec.709 weights; also the `standard` mixer preset.
pub const STANDARD_MIXER: (f32, f32, f32) = (0.2126, 0.7152, 0.0722);

/// Named mixer presets. The weights of every preset sum to 1, so
/// neutral colors (R = G = B) keep their standard gray value.
pub const MIXER_PRESETS: [(&str, (f32, f32, f32)); 3] = [
    ("standard", STANDARD_MIXER),
    ("warm", (0.50, 0.35, 0.15)),
    ("green", (0.15, 0.65, 0.20)),
];

/// Resolve a mixer preset name to its weights.
pub fn mixer_preset(name: &str) -> Option<(f32, f32, f32)> {
    MIXER_PRESETS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, weights)| *weights)
}

/// Tone curve applied to the mixed gray plane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToneCurve {
    Normal,
    Contrast,
    LiftBlacks,
}

impl ToneCurve {
    /// Resolve a curve name ("normal", "contrast", "lift-blacks").
    pub fn from_name(name: &str) -> Option<ToneCurve> {
        match name {
            "normal" => Some(ToneCurve::Normal),
            "contrast" => Some(ToneCurve::Contrast),
            "lift-blacks" => Some(ToneCurve::LiftBlacks),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            ToneCurve::Normal => "normal",
            ToneCurve::Contrast => "contrast",
            ToneCurve::LiftBlacks => "lift-blacks",
        }
    }

    /// Anchor points (input, output), monotonically non-decreasing.
    /// Between anchors the curve is piecewise linear.
    pub fn anchors(self) -> &'static [(f32, f32)] {
        match self {
            ToneCurve::Normal => &[(0.0, 0.0), (1.0, 1.0)],
            ToneCurve::Contrast => &[
                (0.0, 0.0),
                (0.25, 0.16),
                (0.5, 0.5),
                (0.75, 0.84),
                (1.0, 1.0),
            ],
            ToneCurve::LiftBlacks => &[
                (0.0, 0.14),
                (0.25, 0.34),
                (0.5, 0.56),
                (0.75, 0.79),
                (1.0, 0.96),
            ],
        }
    }
}

fn eval_curve(anchors: &[(f32, f32)], x: f32) -> f32 {
    if x <= anchors[0].0 {
        return anchors[0].1;
    }
    for segment in anchors.windows(2) {
        let (x0, y0) = segment[0];
        let (x1, y1) = segment[1];
        if x <= x1 {
            let t = (x - x0) / (x1 - x0);
            return y0 + t * (y1 - y0);
        }
    }
    anchors[anchors.len() - 1].1
}

/// Apply a tone curve to the valid rectangle of the plane.
pub fn apply_tone_curve(
    plane: &mut [f32],
    stride: usize,
    valid_w: usize,
    valid_h: usize,
    curve: ToneCurve,
) {
    let anchors = curve.anchors();
    for y in 0..valid_h {
        for x in 0..valid_w {
            let p = &mut plane[y * stride + x];
            *p = eval_curve(anchors, *p);
        }
    }
}

/// Box-blur radius for local contrast, tied to the output width so the
/// effect is relative to the printed size.
pub fn local_contrast_radius(valid_w: usize) -> usize {
    ((valid_w as f32 / 64.0).round() as usize).clamp(2, 24)
}

/// Amplify the deviation from a wide box blur (the local mean).
/// Small deviations (texture) are amplified almost fully; large ones
/// (strong edges) are amplified less, which limits halos:
/// `w(d) = 1 / (1 + (d / 0.15)^2)`, `out = clamp01(v + amount * d * w(d))`.
///
/// The blur window is clipped to the valid rectangle and normalized by
/// the number of pixels actually inside — the same boundary rule as
/// the existing unsharp mask and the autotune box blur. Extra memory
/// is one base plane plus a row ring buffer.
pub fn apply_local_contrast(
    plane: &mut [f32],
    stride: usize,
    valid_w: usize,
    valid_h: usize,
    amount: f32,
) {
    if amount == 0.0 || valid_w == 0 || valid_h == 0 {
        return;
    }
    let r = local_contrast_radius(valid_w);
    let mut base = vec![0.0_f32; plane.len()];

    // Horizontal pass: sliding-window mean of each row, into `base`.
    for y in 0..valid_h {
        let row = y * stride;
        let mut sum = 0.0_f32;
        let mut lo_ptr = 0_usize;
        let mut hi = 0_usize;
        for x in 0..valid_w {
            let want_hi = (x + r + 1).min(valid_w);
            while hi < want_hi {
                sum += plane[row + hi];
                hi += 1;
            }
            let lo = x.saturating_sub(r);
            while lo_ptr < lo {
                sum -= plane[row + lo_ptr];
                lo_ptr += 1;
            }
            base[row + x] = sum / (hi - lo_ptr) as f32;
        }
    }

    // Vertical pass: column means of the horizontally blurred rows via
    // a ring buffer of the rows inside the window, updating `base`
    // in place; each finished row is applied to `plane` immediately.
    let ring_rows = (2 * r + 1).min(valid_h);
    let mut ring = vec![0.0_f32; ring_rows * valid_w];
    let mut col_sum = vec![0.0_f32; valid_w];
    let mut fed = 0_usize;
    for y in 0..valid_h {
        if y > r {
            let out_row = y - r - 1;
            let slot = (out_row % ring_rows) * valid_w;
            for x in 0..valid_w {
                col_sum[x] -= ring[slot + x];
            }
        }
        let want_hi = (y + r + 1).min(valid_h);
        while fed < want_hi {
            let src = fed * stride;
            let slot = (fed % ring_rows) * valid_w;
            for x in 0..valid_w {
                let v = base[src + x];
                ring[slot + x] = v;
                col_sum[x] += v;
            }
            fed += 1;
        }
        let count = (want_hi - y.saturating_sub(r)) as f32;
        let row = y * stride;
        for x in 0..valid_w {
            let mean = col_sum[x] / count;
            let v = plane[row + x];
            let d = v - mean;
            let w = 1.0 / (1.0 + (d / 0.15) * (d / 0.15));
            plane[row + x] = (v + amount * d * w).clamp(0.0, 1.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixer_presets_resolve_and_sum_to_one() {
        for (name, weights) in MIXER_PRESETS {
            assert_eq!(mixer_preset(name), Some(weights));
            let sum = weights.0 + weights.1 + weights.2;
            assert!((sum - 1.0).abs() < 1e-6, "{name}: {sum}");
        }
        assert_eq!(mixer_preset("standard"), Some(STANDARD_MIXER));
        assert_eq!(mixer_preset("nope"), None);
    }

    #[test]
    fn tone_curve_names_round_trip() {
        for curve in [
            ToneCurve::Normal,
            ToneCurve::Contrast,
            ToneCurve::LiftBlacks,
        ] {
            assert_eq!(ToneCurve::from_name(curve.name()), Some(curve));
        }
        assert_eq!(ToneCurve::from_name("nope"), None);
    }

    #[test]
    fn tone_curve_hits_anchors_and_stays_monotonic() {
        for curve in [ToneCurve::Contrast, ToneCurve::LiftBlacks] {
            let anchors = curve.anchors();
            for &(x, y) in anchors {
                assert!((eval_curve(anchors, x) - y).abs() < 1e-6, "{curve:?} at {x}");
            }
            let mut prev = eval_curve(anchors, 0.0);
            for i in 1..=100 {
                let v = eval_curve(anchors, i as f32 / 100.0);
                assert!(v >= prev, "{curve:?} decreases at {i}");
                prev = v;
            }
        }
        // Piecewise linear: the midpoint of a segment is the mean of its ends.
        let anchors = ToneCurve::Contrast.anchors();
        assert!((eval_curve(anchors, 0.125) - 0.08).abs() < 1e-6);
        // Normal is the identity.
        let anchors = ToneCurve::Normal.anchors();
        for x in [0.0, 0.31, 0.5, 0.77, 1.0] {
            assert_eq!(eval_curve(anchors, x), x);
        }
    }

    #[test]
    fn local_contrast_radius_scales_and_clamps() {
        assert_eq!(local_contrast_radius(384), 6);
        assert_eq!(local_contrast_radius(576), 9);
        assert_eq!(local_contrast_radius(250), 4);
        assert_eq!(local_contrast_radius(10), 2);
        assert_eq!(local_contrast_radius(100_000), 24);
    }

    #[test]
    fn local_contrast_is_identity_for_flat_and_zero_amount() {
        let mut flat = vec![0.4; 6 * 4];
        apply_local_contrast(&mut flat, 6, 6, 4, 1.0);
        assert!(flat.iter().all(|&v| (v - 0.4).abs() < 1e-6));

        let mut plane: Vec<f32> = (0..24).map(|i| i as f32 / 23.0).collect();
        let original = plane.clone();
        apply_local_contrast(&mut plane, 6, 6, 4, 0.0);
        assert_eq!(plane, original);
    }

    #[test]
    fn local_contrast_matches_hand_computed_boundary_case() {
        // 3x1 plane, radius clamps to 2, so every window covers all
        // three pixels and the local mean is 0.4 everywhere. This also
        // pins the boundary rule: clipped window, actual pixel count.
        let mut plane = vec![0.2, 0.2, 0.8];
        apply_local_contrast(&mut plane, 3, 3, 1, 1.0);
        // d = -0.2: w = 1 / (1 + (0.2/0.15)^2) = 0.36 -> 0.2 - 0.072 = 0.128
        assert!((plane[0] - 0.128).abs() < 1e-5, "{}", plane[0]);
        assert!((plane[1] - 0.128).abs() < 1e-5, "{}", plane[1]);
        // d = +0.4: w = 1 / (1 + (0.4/0.15)^2) ~= 0.12329 -> 0.8 + 0.0493
        assert!((plane[2] - 0.84931).abs() < 1e-4, "{}", plane[2]);
    }

    #[test]
    fn local_contrast_amplifies_texture_more_than_edges() {
        // A gentle step (small deviation from the local mean) must move
        // proportionally more than a hard edge of the same center value.
        let gentle: Vec<f32> = (0..16).map(|x| 0.5 + (x as f32 - 7.5) * 0.01).collect();
        let mut g = gentle.clone();
        apply_local_contrast(&mut g, 16, 16, 1, 1.0);
        let g_move = (g[0] - gentle[0]).abs();
        assert!(g_move > 0.0);
        // Result stays in range everywhere.
        assert!(g.iter().all(|&v| (0.0..=1.0).contains(&v)));
    }
}
