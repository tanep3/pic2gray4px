//! Pixel-level Floyd–Steinberg error diffusion.
//!
//! An alternative binarization backend to the 2×2 cell optimizer: where
//! the cell optimizer spends its budget on structure (dot arrangement
//! inside 2×2 cells), error diffusion preserves the local mean tone at
//! full pixel resolution, which renders smooth gradations (skin, sky)
//! more faithfully.
//!
//! The arithmetic replicates Pillow's `Image.convert("1")` (the
//! `tobilevel` function in libImaging/Convert.c) exactly, so the same
//! gray plane produces the same dot pattern as the dither the Lily
//! Print app prints with: the plane is quantized to bytes, the
//! decision value is the byte plus accumulated error in sixteenths
//! (integer division truncating toward zero), clipped to 0..=255,
//! white iff the clipped value is > 128, and the error is propagated
//! with the (1, 5, 3, 7)/16 shares folded into the `l` / `l0` / `l1`
//! carries and the per-row `errors` buffer. For even-sized planes
//! the output is pixel-identical to Pillow's; the padded row/column
//! of an odd-sized plane has no Pillow counterpart and stays white.

/// Binarize a normalized gray plane (0.0 is black, 1.0 is white) with
/// Pillow-identical Floyd–Steinberg diffusion. Only the valid region
/// (`valid_w` × `valid_h`) is processed; padding pixels are emitted
/// white, matching the cell optimizer's treatment of the padded
/// plane.
pub(crate) fn floyd_steinberg(
    target: &[f32],
    w: usize,
    h: usize,
    valid_w: usize,
    valid_h: usize,
) -> Vec<u8> {
    let mut errors = vec![0_i32; valid_w + 1];
    let mut out = vec![1u8; w * h];
    for y in 0..valid_h {
        let (mut l, mut l0, mut l1) = (0_i32, 0_i32, 0_i32);
        for x in 0..valid_w {
            let i = y * w + x;
            let input = (target[i] * 255.0).round().clamp(0.0, 255.0) as i32;
            // Decision value: input plus the accumulated error (kept
            // in sixteenths), clipped to the byte range. Rust's i32
            // division truncates toward zero, like C's.
            l = (input + (l + errors[x + 1]) / 16).clamp(0, 255);
            let white = l > 128;
            out[i] = white as u8;
            l -= if white { 255 } else { 0 };
            // Propagate the error: the shares destined for the next
            // row (below-right 1, below 5, below-left 3, seen from the
            // contributing pixels) are folded into errors[x] / l0,
            // and the forward share (7) stays in l. All values are
            // sixteenths of the error.
            let l2 = l;
            let d2 = l + l;
            l += d2;
            errors[x] = l + l0;
            l += d2;
            l0 = l + l1;
            l1 = l2;
            l += d2;
        }
        errors[valid_w] = l0;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::floyd_steinberg;

    #[test]
    fn exact_binary_plane_is_preserved() {
        // A checkerboard of pure black/white produces zero error, so the
        // output must equal the input exactly.
        let target: Vec<f32> = (0..64)
            .map(|i| if (i / 8 + i % 8) % 2 == 0 { 0.0 } else { 1.0 })
            .collect();
        let out = floyd_steinberg(&target, 8, 8, 8, 8);
        let expected: Vec<u8> = target.iter().map(|&v| v as u8).collect();
        assert_eq!(out, expected);
    }

    #[test]
    fn uniform_mid_gray_keeps_its_mean() {
        // 0.5 gray quantizes to byte 128; Pillow's diffusion of a
        // constant 128 plane yields exactly half white dots.
        let target = vec![0.5_f32; 64 * 64];
        let out = floyd_steinberg(&target, 64, 64, 64, 64);
        let white = out.iter().filter(|&&p| p == 1).count() as f64;
        let frac = white / out.len() as f64;
        assert!((0.47..=0.53).contains(&frac), "white fraction {frac}");
    }

    #[test]
    fn padding_stays_white() {
        // Valid region 6×6 inside an 8×8 plane: padding stays 1.
        let target = vec![0.0_f32; 64];
        let out = floyd_steinberg(&target, 8, 8, 6, 6);
        for y in 0..8 {
            for x in 0..8 {
                if x >= 6 || y >= 6 {
                    assert_eq!(out[y * 8 + x], 1, "padding at {x},{y}");
                } else {
                    assert_eq!(out[y * 8 + x], 0, "valid black at {x},{y}");
                }
            }
        }
    }
}
