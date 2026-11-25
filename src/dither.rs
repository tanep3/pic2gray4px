//! Pixel-level Floyd–Steinberg error diffusion.
//!
//! An alternative binarization backend to the 2×2 cell optimizer: where
//! the cell optimizer spends its budget on structure (dot arrangement
//! inside 2×2 cells), error diffusion preserves the local mean tone at
//! full pixel resolution, which renders smooth gradations (skin, sky)
//! more faithfully. It consumes the same preprocessed f32 plane as the
//! optimizer — 0.0 is black, 1.0 is white — and returns the same 0/1
//! pixel encoding, so the mixer, tone curve, local contrast, and the
//! contrast/unsharp candidate search (scored on the completed output)
//! all apply unchanged.

/// Binarize a normalized gray plane with serpentine Floyd–Steinberg
/// diffusion. Only the valid region (`valid_w` × `valid_h`) is
/// processed; padding pixels are emitted white, matching the cell
/// optimizer's treatment of the padded plane.
pub(crate) fn floyd_steinberg(
    target: &[f32],
    w: usize,
    h: usize,
    valid_w: usize,
    valid_h: usize,
) -> Vec<u8> {
    let mut buf = target.to_vec();
    let mut out = vec![1u8; w * h];
    for y in 0..valid_h {
        let ltr = y % 2 == 0;
        let xs: Vec<usize> = if ltr {
            (0..valid_w).collect()
        } else {
            (0..valid_w).rev().collect()
        };
        let dir: isize = if ltr { 1 } else { -1 };
        for x in xs {
            let i = y * w + x;
            let old = buf[i];
            let new = if old >= 0.5 { 1.0 } else { 0.0 };
            out[i] = new as u8;
            let err = old - new;
            if err == 0.0 {
                continue;
            }
            // Forward neighbor on this row: 7/16.
            push(&mut buf, w, valid_w, valid_h, x as isize + dir, y, err * (7.0 / 16.0));
            // Next row: behind 3/16, center 5/16, forward 1/16.
            push(&mut buf, w, valid_w, valid_h, x as isize - dir, y + 1, err * (3.0 / 16.0));
            push(&mut buf, w, valid_w, valid_h, x as isize, y + 1, err * (5.0 / 16.0));
            push(&mut buf, w, valid_w, valid_h, x as isize + dir, y + 1, err * (1.0 / 16.0));
        }
    }
    out
}

fn push(buf: &mut [f32], w: usize, valid_w: usize, valid_h: usize, x: isize, y: usize, delta: f32) {
    if x < 0 || x as usize >= valid_w || y >= valid_h {
        return;
    }
    buf[y * w + x as usize] += delta;
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
        // 0.5 gray must diffuse to roughly half white dots: the local
        // mean is the property error diffusion exists to preserve.
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
