//! Port of the Python VM experiment's 2x2 coordinate-descent optimizer.

pub(crate) struct Optimized {
    pub pixels: Vec<u8>,
    pub changes: Vec<usize>,
}

const COORDS: [(usize, usize); 4] = [(0, 0), (0, 1), (1, 0), (1, 1)];
const FIXED_ORDER: [usize; 4] = [0, 3, 1, 2];

fn value(pattern: u8, bit: usize) -> f64 {
    ((pattern >> (3 - bit)) & 1) as f64
}

fn gaussian(sigma: f64) -> [[f64; 9]; 9] {
    let mut g = [[0.0; 9]; 9];
    let mut sum = 0.0;
    for (y, row) in g.iter_mut().enumerate() {
        for (x, v) in row.iter_mut().enumerate() {
            let dx = x as f64 - 4.0;
            let dy = y as f64 - 4.0;
            *v = (-(dx * dx + dy * dy) / (2.0 * sigma * sigma)).exp();
            sum += *v;
        }
    }
    for row in &mut g {
        for v in row {
            *v /= sum;
        }
    }
    g
}

fn self_convolution_center(g: &[[f64; 9]; 9], y: usize, x: usize) -> f64 {
    let full_y = y + 4;
    let full_x = x + 4;
    let mut sum = 0.0;
    for gy in 0..9 {
        for gx in 0..9 {
            if full_y >= gy && full_x >= gx && full_y - gy < 9 && full_x - gx < 9 {
                sum += g[gy][gx] * g[full_y - gy][full_x - gx];
            }
        }
    }
    sum
}

fn objective_kernel() -> [[f64; 9]; 9] {
    let mut kernel = [[0.0; 9]; 9];
    for (sigma, weight) in [(0.65, 0.55), (1.25, 0.30)] {
        let g = gaussian(sigma);
        for (y, row) in kernel.iter_mut().enumerate() {
            for (x, v) in row.iter_mut().enumerate() {
                *v += weight * self_convolution_center(&g, y, x);
            }
        }
    }
    kernel[4][4] += 0.15;
    kernel
}

fn convolve_residual(
    out: &[f64],
    read: &impl Fn(usize) -> f64,
    w: usize,
    h: usize,
    kernel: &[[f64; 9]; 9],
) -> Vec<f64> {
    let mut residual = vec![0.0; out.len()];
    for y in 0..h {
        for x in 0..w {
            let mut sum = 0.0;
            for (ky, row) in kernel.iter().enumerate() {
                for (kx, &k) in row.iter().enumerate() {
                    let ny = y as isize + ky as isize - 4;
                    let nx = x as isize + kx as isize - 4;
                    if ny >= 0 && nx >= 0 && ny < h as isize && nx < w as isize {
                        let i = ny as usize * w + nx as usize;
                        sum += (out[i] - read(i)) * k;
                    }
                }
            }
            residual[y * w + x] = sum;
        }
    }
    residual
}

fn required_white(x: usize, y: usize, valid_w: usize, valid_h: usize) -> u8 {
    let mut mask = 0;
    for (bit, &(dy, dx)) in COORDS.iter().enumerate() {
        if x + dx >= valid_w || y + dy >= valid_h {
            mask |= 1 << (3 - bit);
        }
    }
    mask
}

fn counts(
    read: &impl Fn(usize) -> f64,
    w: usize,
    h: usize,
    valid_w: usize,
    valid_h: usize,
    diffusion: bool,
) -> Vec<u8> {
    let bw = w / 2;
    let bh = h / 2;
    let mut counts = vec![0; bw * bh];
    let mut carry = vec![0.0; bw * bh];
    for by in 0..bh {
        for step in 0..bw {
            let forward = by % 2 == 0;
            let bx = if forward { step } else { bw - 1 - step };
            let cell = by * bw + bx;
            let x = bx * 2;
            let y = by * 2;
            let sum: f64 = COORDS
                .iter()
                .map(|&(dy, dx)| read((y + dy) * w + x + dx))
                .sum();
            let adjusted = sum + carry[cell];
            let min_count = required_white(x, y, valid_w, valid_h).count_ones() as i32;
            let k = ((adjusted + 0.5).floor() as i32).clamp(min_count, 4);
            counts[cell] = k as u8;
            if diffusion {
                let error = adjusted - k as f64;
                let next = if forward { bx + 1 } else { bx.wrapping_sub(1) };
                let below_back = if forward { bx.wrapping_sub(1) } else { bx + 1 };
                let neighbors = [
                    (next, by, 7.0 / 16.0),
                    (below_back, by + 1, 3.0 / 16.0),
                    (bx, by + 1, 5.0 / 16.0),
                    (next, by + 1, 1.0 / 16.0),
                ];
                for &(nx, ny, weight) in &neighbors {
                    if nx < bw && ny < bh {
                        carry[ny * bw + nx] += error * weight;
                    }
                }
            }
        }
    }
    counts
}

/// Main-pipeline entry: the target plane stays f32 and is read through an
/// accessor that widens each value to f64 (an exact conversion), so no
/// full-size f64 copy of the plane is ever materialized.
pub(crate) fn optimize(
    target: &[f32],
    w: usize,
    h: usize,
    valid_w: usize,
    valid_h: usize,
    passes: usize,
    diffusion: bool,
) -> Optimized {
    optimize_impl(
        &|i| target[i] as f64,
        target.len(),
        w,
        h,
        valid_w,
        valid_h,
        passes,
        diffusion,
    )
}

/// Exact-f64 entry (the gray8 reference path, whose target values are
/// v/255 computed in f64 and pinned by golden tests).
pub(crate) fn optimize_f64(
    target: &[f64],
    w: usize,
    h: usize,
    valid_w: usize,
    valid_h: usize,
    passes: usize,
    diffusion: bool,
) -> Optimized {
    optimize_impl(
        &|i| target[i],
        target.len(),
        w,
        h,
        valid_w,
        valid_h,
        passes,
        diffusion,
    )
}

fn optimize_impl(
    read: &impl Fn(usize) -> f64,
    len: usize,
    w: usize,
    h: usize,
    valid_w: usize,
    valid_h: usize,
    passes: usize,
    diffusion: bool,
) -> Optimized {
    assert!(w > 0 && h > 0 && w % 2 == 0 && h % 2 == 0 && len == w * h);
    assert!(valid_w > 0 && valid_h > 0 && valid_w <= w && valid_h <= h);
    let bw = w / 2;
    let bh = h / 2;
    let counts = counts(read, w, h, valid_w, valid_h, diffusion);
    let mut out = vec![0.0; len];
    for by in 0..bh {
        for bx in 0..bw {
            let k = counts[by * bw + bx] as usize;
            let required = required_white(bx * 2, by * 2, valid_w, valid_h);
            let mut order = Vec::with_capacity(4);
            for bit in 0..4 {
                if required & (1 << (3 - bit)) != 0 {
                    order.push(bit);
                }
            }
            for &bit in &FIXED_ORDER {
                if required & (1 << (3 - bit)) == 0 {
                    order.push(bit);
                }
            }
            for &bit in order.iter().take(k) {
                let (dy, dx) = COORDS[bit];
                out[(by * 2 + dy) * w + bx * 2 + dx] = 1.0;
            }
        }
    }
    let kernel = objective_kernel();
    let mut residual = convolve_residual(&out, read, w, h, &kernel);
    let mut q = [[0.0; 4]; 4];
    for (a, &(ay, ax)) in COORDS.iter().enumerate() {
        for (b, &(by, bx)) in COORDS.iter().enumerate() {
            q[a][b] = kernel[(4 + ay).wrapping_sub(by)][(4 + ax).wrapping_sub(bx)];
        }
    }
    let mut changes = Vec::new();
    for sweep in 0..passes {
        let mut changed = 0;
        for step in 0..counts.len() {
            let cell = if sweep % 2 == 0 {
                step
            } else {
                counts.len() - 1 - step
            };
            let bx = cell % bw;
            let by = cell / bw;
            let k = counts[cell];
            if k == 0 || k == 4 {
                continue;
            }
            let x = bx * 2;
            let y = by * 2;
            let required = required_white(x, y, valid_w, valid_h);
            let mut current = [0.0; 4];
            let mut r = [0.0; 4];
            for (bit, &(dy, dx)) in COORDS.iter().enumerate() {
                current[bit] = out[(y + dy) * w + x + dx];
                r[bit] = residual[(y + dy) * w + x + dx];
            }
            let mut best_pattern = 0;
            let mut best_cost = f64::INFINITY;
            let mut best_delta = [0.0; 4];
            for pattern in 0..16_u8 {
                if pattern.count_ones() != k as u32 || pattern & required != required {
                    continue;
                }
                let mut delta = [0.0; 4];
                for bit in 0..4 {
                    delta[bit] = value(pattern, bit) - current[bit];
                }
                let linear: f64 = (0..4).map(|i| delta[i] * r[i]).sum();
                let quadratic: f64 = (0..4)
                    .map(|i| (0..4).map(|j| delta[i] * q[i][j] * delta[j]).sum::<f64>())
                    .sum();
                let cost = 2.0 * linear + quadratic;
                if cost < best_cost {
                    best_cost = cost;
                    best_pattern = pattern;
                    best_delta = delta;
                }
            }
            if best_cost >= -1e-10 {
                continue;
            }
            changed += 1;
            for (bit, &(dy, dx)) in COORDS.iter().enumerate() {
                out[(y + dy) * w + x + dx] = value(best_pattern, bit);
                let v = best_delta[bit];
                if v == 0.0 {
                    continue;
                }
                let cy = y + dy;
                let cx = x + dx;
                for py in cy.saturating_sub(4)..=(cy + 4).min(h - 1) {
                    for px in cx.saturating_sub(4)..=(cx + 4).min(w - 1) {
                        residual[py * w + px] += v * kernel
                            [(py as isize - cy as isize + 4) as usize]
                            [(px as isize - cx as isize + 4) as usize];
                    }
                }
            }
        }
        changes.push(changed);
        if changed == 0 {
            break;
        }
    }
    Optimized {
        pixels: out.into_iter().map(|p| p as u8).collect(),
        changes,
    }
}
