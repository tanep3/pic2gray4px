//! A bounded, deterministic proxy for the readability of a completed 1-bit image.

fn box_blur(read: &impl Fn(usize) -> f64, len: usize, w: usize, h: usize, radius: usize) -> Vec<f64> {
    let mut sums = vec![0.0; (w + 1) * (h + 1)];
    for y in 0..h {
        let mut row = 0.0;
        for x in 0..w {
            row += read(y * w + x);
            sums[(y + 1) * (w + 1) + x + 1] = sums[y * (w + 1) + x + 1] + row;
        }
    }
    let mut out = vec![0.0; len];
    for y in 0..h {
        for x in 0..w {
            let x0 = x.saturating_sub(radius);
            let y0 = y.saturating_sub(radius);
            let x1 = (x + radius + 1).min(w);
            let y1 = (y + radius + 1).min(h);
            let sum = sums[y1 * (w + 1) + x1] - sums[y0 * (w + 1) + x1] - sums[y1 * (w + 1) + x0]
                + sums[y0 * (w + 1) + x0];
            out[y * w + x] = sum / ((x1 - x0) * (y1 - y0)) as f64;
        }
    }
    out
}

pub(crate) fn score(
    original: &[f32],
    output: &[u8],
    w: usize,
    h: usize,
    valid_w: usize,
    valid_h: usize,
) -> f64 {
    // The f32/u8 inputs are widened to f64 on read (an exact conversion)
    // instead of being materialized as full-size f64 planes, and the two
    // blur scales are accumulated in separate passes so only one scale's
    // blurred planes are alive at a time. Each accumulator still adds the
    // same terms in the same order, so the score is bit-identical.
    let read_source = |i: usize| original[i] as f64;
    let read_binary = |i: usize| output[i] as f64;
    let src3 = box_blur(&read_source, original.len(), w, h, 1);
    let out3 = box_blur(&read_binary, output.len(), w, h, 1);
    let mut e3 = 0.0;
    let mut edges = 0.0;
    let mut flat_noise = 0.0;
    for y in 0..valid_h {
        for x in 0..valid_w {
            let i = y * w + x;
            e3 += (src3[i] - out3[i]).powi(2);
            if x + 1 < valid_w && y + 1 < valid_h {
                let sx = src3[i + 1] - src3[i];
                let sy = src3[i + w] - src3[i];
                let ox = out3[i + 1] - out3[i];
                let oy = out3[i + w] - out3[i];
                edges += (sx - ox).powi(2) + (sy - oy).powi(2);
                if sx.abs() + sy.abs() < 0.02 {
                    flat_noise += ox * ox + oy * oy;
                }
            }
        }
    }
    drop(src3);
    drop(out3);
    let src9 = box_blur(&read_source, original.len(), w, h, 4);
    let out9 = box_blur(&read_binary, output.len(), w, h, 4);
    let mut e9 = 0.0;
    for y in 0..valid_h {
        for x in 0..valid_w {
            let i = y * w + x;
            e9 += (src9[i] - out9[i]).powi(2);
        }
    }
    drop(src9);
    drop(out9);
    let n = (valid_w * valid_h) as f64;
    let mut false_saturated = 0.0;
    for y in (0..valid_h.saturating_sub(1)).step_by(2) {
        for x in (0..valid_w.saturating_sub(1)).step_by(2) {
            let sum = output[y * w + x]
                + output[y * w + x + 1]
                + output[(y + 1) * w + x]
                + output[(y + 1) * w + x + 1];
            if sum == 0 || sum == 4 {
                let mean = (read_source(y * w + x)
                    + read_source(y * w + x + 1)
                    + read_source((y + 1) * w + x)
                    + read_source((y + 1) * w + x + 1))
                    / 4.0;
                if (0.125..0.875).contains(&mean) {
                    false_saturated += 1.0;
                }
            }
        }
    }
    e3 / n
        + 0.75 * e9 / n
        + 0.25 * edges / n
        + 0.08 * flat_noise / n
        + 0.02 * false_saturated / (n / 4.0)
}

#[cfg(test)]
mod tests {
    use super::score;

    #[test]
    fn exact_binary_input_scores_zero() {
        let original: Vec<f32> = (0..64)
            .map(|i| if (i / 8 + i % 8) % 2 == 0 { 0.0 } else { 1.0 })
            .collect();
        let output: Vec<u8> = original.iter().map(|&v| v as u8).collect();
        assert_eq!(score(&original, &output, 8, 8, 8, 8), 0.0);
    }

    #[test]
    fn false_saturation_is_worse_than_balanced_tone() {
        let original = vec![0.5; 64];
        let white = vec![1; 64];
        let balanced: Vec<u8> = (0..64).map(|i| ((i / 8 + i % 8) % 2) as u8).collect();
        assert!(score(&original, &balanced, 8, 8, 8, 8) < score(&original, &white, 8, 8, 8, 8));
    }
}
