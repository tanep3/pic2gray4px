//! 2×2 neighborhood gray, represented by real black and white pixels.

mod adjust;
mod autotune;
mod optimizer;

pub use adjust::{mixer_preset, ToneCurve, MIXER_PRESETS, STANDARD_MIXER};

/// A named conversion mode: a bundle of the diffusion and exchange
/// settings that suit a class of input. Modes are resolved by the core
/// so the CLI, the Web app, and library callers share one definition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModePreset {
    /// Cell white-count diffusion on/off.
    pub diffusion: bool,
    /// Inter-cell white-dot exchange rounds.
    pub exchange: usize,
}

/// Exchange rounds applied by both named modes. Measured on the
/// comparison set: round 2 captures most of the gain and round 3 a
/// small consistent tail; the judged comparison sheets used 3.
pub const MODE_EXCHANGE_ROUNDS: usize = 3;

/// Resolve a mode name ("photo" / "illustration") to its settings.
/// - photo: diffusion on + exchange — smooth tonal gradation wins.
/// - illustration: diffusion off + exchange — flat areas stay clean.
pub fn mode_preset(name: &str) -> Option<ModePreset> {
    match name {
        "photo" => Some(ModePreset {
            diffusion: true,
            exchange: MODE_EXCHANGE_ROUNDS,
        }),
        "illustration" => Some(ModePreset {
            diffusion: false,
            exchange: MODE_EXCHANGE_ROUNDS,
        }),
        _ => None,
    }
}

use image::{imageops, DynamicImage, RgbaImage};
#[cfg(feature = "load-image")]
use image::{ImageDecoder, ImageReader};
use std::error::Error;
#[cfg(feature = "load-image")]
use std::fs::File;
use std::fs::OpenOptions;
#[cfg(feature = "load-image")]
use std::io::BufReader;
use std::io::{BufWriter, Write};
use std::path::Path;

pub type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(Clone, Copy, Debug)]
pub struct ConversionOptions {
    pub width: Option<u32>,
    pub passes: usize,
    /// None searches completed 4px outputs; Some(1.0) disables contrast.
    pub contrast: Option<f32>,
    /// None searches completed 4px outputs; Some(0.0) disables sharpening.
    pub unsharp: Option<f32>,
    /// Diffuse cell white-count rounding residuals to neighboring cells.
    pub diffusion: bool,
    /// Inter-cell white-dot exchange rounds (0 disables). Adjacent cells
    /// may trade one white dot — the pair's total is preserved — when the
    /// joint placement lowers the perceptual cost. Applied once, to the
    /// winning candidate only, after the contrast/unsharp search.
    pub exchange: usize,
    /// Channel mixer weights for the color-to-gray stage.
    /// None selects the standard preset (omission means the standard
    /// preset, unlike contrast/unsharp where omission means search).
    pub mixer: Option<(f32, f32, f32)>,
    /// Tone curve for the mixed gray plane.
    pub tone_curve: ToneCurve,
    /// Local contrast amount 0..1; 0.0 skips the stage entirely.
    pub local_contrast: f32,
}

pub struct ConversionResult {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
    pub contrast: f32,
    pub unsharp: f32,
    /// Mixer weights actually applied.
    pub mixer: (f32, f32, f32),
    /// Tone curve actually applied.
    pub tone_curve: ToneCurve,
    /// Local contrast amount actually applied.
    pub local_contrast: f32,
    /// Exchange rounds actually applied.
    pub exchange: usize,
}

impl Default for ConversionOptions {
    fn default() -> Self {
        Self {
            width: None,
            passes: 10,
            contrast: None,
            unsharp: None,
            diffusion: true,
            exchange: 0,
            mixer: None,
            tone_curve: ToneCurve::Normal,
            local_contrast: 0.0,
        }
    }
}

/// Load an image, respecting EXIF orientation where the decoder supports it.
/// Only available with the `load-image` feature (default, used by the CLI);
/// the format decoders are not part of the conversion core.
#[cfg(feature = "load-image")]
pub fn load_image(path: &Path) -> Result<DynamicImage> {
    let decoder = ImageReader::new(BufReader::new(File::open(path)?))
        .with_guessed_format()?
        .into_decoder()?;
    let mut decoder = decoder;
    let orientation = decoder.orientation()?;
    let mut image = DynamicImage::from_decoder(decoder)?;
    image.apply_orientation(orientation);
    Ok(image)
}

/// Convert with optional proportional resizing. The output dimensions are rounded up to even.
pub fn convert(
    image: &DynamicImage,
    width: Option<u32>,
    passes: usize,
) -> Result<(u32, u32, Vec<u8>)> {
    let result = convert_with_options(
        image,
        ConversionOptions {
            width,
            passes,
            ..ConversionOptions::default()
        },
    )?;
    Ok((result.width, result.height, result.pixels))
}

pub fn convert_with_options(
    image: &DynamicImage,
    options: ConversionOptions,
) -> Result<ConversionResult> {
    let ConversionOptions {
        width,
        passes,
        contrast,
        unsharp,
        diffusion,
        exchange,
        mixer,
        tone_curve,
        local_contrast,
    } = options;
    if contrast.is_some_and(|v| !(0.0..=3.0).contains(&v) || !v.is_finite()) {
        return Err("contrast must be between 0 and 3".into());
    }
    if unsharp.is_some_and(|v| !(0.0..=2.0).contains(&v) || !v.is_finite()) {
        return Err("unsharp must be between 0 and 2".into());
    }
    if let Some(weights) = mixer {
        if [weights.0, weights.1, weights.2]
            .iter()
            .any(|v| !v.is_finite() || !(-2.0..=2.0).contains(v))
        {
            return Err("mixer weights must be finite numbers between -2 and 2".into());
        }
    }
    if !local_contrast.is_finite() || !(0.0..=1.0).contains(&local_contrast) {
        return Err("local contrast must be between 0 and 1".into());
    }
    if passes > 10 {
        return Err("passes must be between 0 and 10".into());
    }
    if image.width() == 0 || image.height() == 0 {
        return Err("input image has no pixels".into());
    }
    let rgba = image.to_rgba8();
    let rgba = if let Some(w) = width {
        if w == 0 {
            return Err("width must be greater than zero".into());
        }
        let h = ((image.height() as f64 * w as f64 / image.width() as f64).round() as u32).max(1);
        imageops::resize(&rgba, w, h, imageops::FilterType::Lanczos3)
    } else {
        rgba
    };
    let out_w = rgba
        .width()
        .checked_add(rgba.width() % 2)
        .ok_or("image width is too large")?;
    let out_h = rgba
        .height()
        .checked_add(rgba.height() % 2)
        .ok_or("image height is too large")?;
    let len = (out_w as usize)
        .checked_mul(out_h as usize)
        .ok_or("image is too large")?;
    let valid_w = rgba.width() as usize;
    let valid_h = rgba.height() as usize;
    let stride = out_w as usize;
    let mut target = vec![1.0_f32; len];
    let mixer_weights = mixer.unwrap_or(STANDARD_MIXER);
    fill_mixed(&rgba, out_w, &mut target, mixer_weights);
    // Color adjustment front-end. The adjusted plane becomes the
    // reference for everything downstream (tonal check, candidate
    // search, scoring); the Rec.709 luminance is never reused.
    if tone_curve != ToneCurve::Normal {
        adjust::apply_tone_curve(&mut target, stride, valid_w, valid_h, tone_curve);
    }
    if local_contrast != 0.0 {
        adjust::apply_local_contrast(&mut target, stride, valid_w, valid_h, local_contrast);
    }
    let mut eligible = Vec::new();
    for y in 0..valid_h {
        for x in 0..valid_w {
            if rgba.get_pixel(x as u32, y as u32).0[3] >= 128 {
                eligible.push(y * stride + x);
            }
        }
    }
    // The RGBA plane is not read again (the candidate search works on the
    // luminance plane only); release it before the search's peak.
    drop(rgba);
    let tonal = has_tonal_range(&target, &eligible);
    let contrasts: Vec<f32> = match contrast {
        Some(v) => vec![v],
        None if tonal => vec![1.0, 1.04, 1.08, 1.12],
        None => vec![1.0],
    };
    let sharpness: Vec<f32> = match unsharp {
        Some(v) => vec![v],
        None if tonal => vec![0.0, 0.25, 0.5, 0.75, 1.0, 1.25, 1.5],
        None => vec![0.0],
    };
    let mut best: Option<(f64, f32, f32, Vec<u8>)> = None;
    for factor in contrasts {
        let mut contrasted = target.clone();
        preprocess(&mut contrasted, stride, valid_w, valid_h, factor, 0.0);
        if contrast.is_none() {
            let (rms, extra_sat) = stats(&target, &contrasted, &eligible);
            if rms > 6.0 / 255.0 || extra_sat > 0.01 {
                continue;
            }
        }
        for amount in &sharpness {
            let mut trial = contrasted.clone();
            preprocess(&mut trial, stride, valid_w, valid_h, 1.0, *amount);
            if unsharp.is_none() {
                let (rms, extra_sat) = stats(&contrasted, &trial, &eligible);
                if rms > 12.0 / 255.0 || extra_sat > 0.02 {
                    continue;
                }
            }
            // The candidate search always runs without exchange: scores
            // stay comparable across candidates, and the exchange stage
            // (which improves every candidate in the same direction) is
            // applied once, to the winner only, below.
            let result = optimizer::optimize(
                &trial,
                out_w as usize,
                out_h as usize,
                valid_w,
                valid_h,
                passes,
                diffusion,
                0,
            );
            let score = if contrast.is_none() || unsharp.is_none() {
                autotune::score(
                    &target,
                    &result.pixels,
                    stride,
                    out_h as usize,
                    valid_w,
                    valid_h,
                ) + 0.0001 * ((factor - 1.0).powi(2) + amount.powi(2)) as f64
            } else {
                0.0
            };
            if best
                .as_ref()
                .is_none_or(|(old, _, _, _)| score < *old - 1e-12)
            {
                best = Some((score, factor, *amount, result.pixels));
            }
        }
    }
    let (_, contrast, unsharp, pixels) = best.expect("unadjusted candidate must be available");
    let pixels = if exchange > 0 {
        // Rebuild the winning candidate's plane (preprocessing is
        // deterministic) and re-run its optimization with exchange.
        let mut contrasted = target.clone();
        preprocess(&mut contrasted, stride, valid_w, valid_h, contrast, 0.0);
        let mut trial = contrasted;
        preprocess(&mut trial, stride, valid_w, valid_h, 1.0, unsharp);
        optimizer::optimize(
            &trial,
            out_w as usize,
            out_h as usize,
            valid_w,
            valid_h,
            passes,
            diffusion,
            exchange,
        )
        .pixels
    } else {
        pixels
    };
    Ok(ConversionResult {
        width: out_w,
        height: out_h,
        pixels,
        contrast,
        unsharp,
        mixer: mixer_weights,
        tone_curve,
        local_contrast,
        exchange,
    })
}

/// Convert raw RGBA bytes (as delivered by a browser canvas or a WASM caller)
/// through the same pipeline as [`convert_with_options`]. For identical RGBA
/// input the result is identical to the `DynamicImage` entry point.
pub fn convert_rgba(
    rgba: &[u8],
    width: u32,
    height: u32,
    options: ConversionOptions,
) -> Result<ConversionResult> {
    if width == 0 || height == 0 {
        return Err("input image has no pixels".into());
    }
    let expected = (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(4))
        .ok_or("image is too large")?;
    if rgba.len() != expected {
        return Err("RGBA buffer length must equal width*height*4".into());
    }
    let image = RgbaImage::from_raw(width, height, rgba.to_vec())
        .ok_or("RGBA buffer length must equal width*height*4")?;
    convert_with_options(&DynamicImage::ImageRgba8(image), options)
}

fn stats(baseline: &[f32], trial: &[f32], eligible: &[usize]) -> (f32, f32) {
    if eligible.is_empty() {
        return (0.0, 0.0);
    }
    let mut sum_sq = 0.0;
    let mut base_sat = 0;
    let mut trial_sat = 0;
    for &i in eligible {
        let d = trial[i] - baseline[i];
        sum_sq += d * d;
        base_sat += (baseline[i] <= 1.0 / 255.0 || baseline[i] >= 254.0 / 255.0) as usize;
        trial_sat += (trial[i] <= 1.0 / 255.0 || trial[i] >= 254.0 / 255.0) as usize;
    }
    (
        (sum_sq / eligible.len() as f32).sqrt(),
        (trial_sat as f32 - base_sat as f32) / eligible.len() as f32,
    )
}

fn has_tonal_range(target: &[f32], eligible: &[usize]) -> bool {
    if eligible.len() < 16 {
        return false;
    }
    let mut values: Vec<f32> = eligible.iter().map(|&i| target[i]).collect();
    values.sort_by(f32::total_cmp);
    values[values.len() * 9 / 10] - values[values.len() / 10] >= 0.1
}

fn preprocess(
    target: &mut [f32],
    stride: usize,
    valid_w: usize,
    valid_h: usize,
    contrast: f32,
    unsharp: f32,
) {
    if contrast != 1.0 {
        for y in 0..valid_h {
            for x in 0..valid_w {
                let p = &mut target[y * stride + x];
                *p = ((*p - 0.5) * contrast + 0.5).clamp(0.0, 1.0);
            }
        }
    }
    if unsharp != 0.0 {
        let original = target.to_vec();
        for y in 0..valid_h {
            for x in 0..valid_w {
                let mut sum = 0.0;
                let mut norm = 0.0;
                for (ky, row) in UNSHARP_KERNEL.iter().enumerate() {
                    for (kx, &weight) in row.iter().enumerate() {
                        let nx = x as isize + kx as isize - 1;
                        let ny = y as isize + ky as isize - 1;
                        if nx >= 0 && ny >= 0 && nx < valid_w as isize && ny < valid_h as isize {
                            sum += original[ny as usize * stride + nx as usize] * weight;
                            norm += weight;
                        }
                    }
                }
                let i = y * stride + x;
                target[i] = (original[i] + unsharp * (original[i] - sum / norm)).clamp(0.0, 1.0);
            }
        }
    }
}

const UNSHARP_KERNEL: [[f32; 3]; 3] = [[1.0, 2.0, 1.0], [2.0, 4.0, 2.0], [1.0, 2.0, 1.0]];

/// Standard-weights fill, kept for the existing tests.
#[cfg(test)]
fn fill_luminance(rgba: &RgbaImage, stride: u32, target: &mut [f32]) {
    fill_mixed(rgba, stride, target, STANDARD_MIXER);
}

fn fill_mixed(rgba: &RgbaImage, stride: u32, target: &mut [f32], weights: (f32, f32, f32)) {
    let (wr, wg, wb) = weights;
    for y in 0..rgba.height() {
        for x in 0..rgba.width() {
            let p = rgba.get_pixel(x, y).0;
            let alpha = p[3] as f32 / 255.0;
            // Keep the operation shape identical to the original
            // Rec.709 expression so the standard weights reproduce the
            // previous results bit for bit. The clamp is a no-op for
            // the standard weights (their range is exactly [0, 1]) and
            // only bounds custom weights.
            let luminance =
                ((wr * p[0] as f32 + wg * p[1] as f32 + wb * p[2] as f32) / 255.0).clamp(0.0, 1.0);
            target[(y * stride + x) as usize] = luminance * alpha + 1.0 - alpha;
        }
    }
}

/// Optimize an already normalized, even-size gray image without count diffusion.
/// This compatibility helper keeps the previous f32 API.
pub fn optimize(target: &[f32], w: usize, h: usize, passes: usize) -> Vec<u8> {
    optimizer::optimize(target, w, h, w, h, passes, false, 0).pixels
}

/// Run the VM experiment optimizer on exact gray8 bytes, without preprocessing or resizing.
pub fn optimize_gray8_reference(raw: &[u8], w: usize, h: usize) -> Result<(Vec<u8>, Vec<usize>)> {
    optimize_gray8(raw, w, h, false)
}

/// Optimize exact gray8 bytes with optional cell-level count diffusion.
pub fn optimize_gray8(
    raw: &[u8],
    w: usize,
    h: usize,
    diffusion: bool,
) -> Result<(Vec<u8>, Vec<usize>)> {
    if w == 0 || h == 0 || w % 2 != 0 || h % 2 != 0 || w.checked_mul(h) != Some(raw.len()) {
        return Err("gray8 input needs positive even dimensions and width*height bytes".into());
    }
    let target: Vec<f64> = raw.iter().map(|&v| v as f64 / 255.0).collect();
    let result = optimizer::optimize_f64(&target, w, h, w, h, 10, diffusion, 0);
    Ok((result.pixels, result.changes))
}

/// Encode binary pixels (one 0/1 byte per pixel) as an actual 1-bit grayscale
/// PNG (PNG color type 0, bit depth 1) in memory. The bytes are identical to
/// what [`write_png`] writes for the same pixels.
pub fn encode_1bit_png(w: u32, h: u32, pixels: &[u8]) -> Result<Vec<u8>> {
    if pixels.len() != w as usize * h as usize || pixels.iter().any(|&v| v > 1) {
        return Err("invalid binary pixel buffer".into());
    }
    let row_bytes = (w as usize + 7) / 8;
    let mut packed = vec![0_u8; row_bytes * h as usize];
    for y in 0..h as usize {
        for x in 0..w as usize {
            packed[y * row_bytes + x / 8] |= pixels[y * w as usize + x] << (7 - x % 8);
        }
    }
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, w, h);
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::One);
        encoder.set_compression(png::Compression::High);
        let mut writer = encoder.write_header()?;
        writer.write_image_data(&packed)?;
        writer.finish()?;
    }
    Ok(out)
}

/// Write an actual 1-bit grayscale PNG (PNG color type 0, bit depth 1).
pub fn write_png(path: &Path, w: u32, h: u32, pixels: &[u8], overwrite: bool) -> Result<()> {
    let bytes = encode_1bit_png(w, h, pixels)?;
    let mut file = BufWriter::new(if overwrite {
        OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?
    } else {
        OpenOptions::new().write(true).create_new(true).open(path)?
    });
    file.write_all(&bytes)?;
    file.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_block_keeps_its_quantized_brightness() {
        let mut target = vec![0.0; 20];
        for block in 0..5 {
            for bit in 0..4 {
                let x = block * 2 + bit % 2;
                let y = bit / 2;
                target[y * 10 + x] = block as f32 / 4.0;
            }
        }
        for passes in [0, 1, 8] {
            let result = optimize(&target, 10, 2, passes);
            for block in 0..5 {
                let x = block * 2;
                let count = result[x] + result[x + 1] + result[10 + x] + result[11 + x];
                assert_eq!(count, block as u8);
            }
        }
    }

    #[test]
    fn png_header_is_grayscale_one_bit() {
        let path =
            std::env::temp_dir().join(format!("pic2gray4px-test-{}.png", std::process::id()));
        write_png(&path, 2, 2, &[0, 1, 1, 0], false).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        std::fs::remove_file(path).unwrap();
        assert_eq!(bytes[24], 1); // bit depth in IHDR
        assert_eq!(bytes[25], 0); // grayscale color type in IHDR
    }

    #[test]
    fn png_writer_refuses_existing_file_without_overwrite() {
        let path =
            std::env::temp_dir().join(format!("pic2gray4px-existing-{}.png", std::process::id()));
        std::fs::write(&path, b"important").unwrap();
        assert!(write_png(&path, 2, 2, &[0, 1, 1, 0], false).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"important");
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn transparent_pixels_become_white_and_odd_edges_are_padded() {
        let image = DynamicImage::ImageRgba8(RgbaImage::from_raw(1, 1, vec![0, 0, 0, 0]).unwrap());
        let (w, h, pixels) = convert(&image, None, 8).unwrap();
        assert_eq!((w, h), (2, 2));
        assert_eq!(pixels, vec![1; 4]);

        let image = DynamicImage::ImageRgba8(
            RgbaImage::from_raw(3, 1, vec![0, 0, 0, 255, 127, 127, 127, 255, 0, 0, 0, 255])
                .unwrap(),
        );
        let (w, h, pixels) = convert(&image, None, 8).unwrap();
        assert_eq!((w, h), (4, 2));
        assert_eq!(pixels[3], 1); // right padding
        assert!(pixels[4..].iter().all(|&p| p == 1)); // bottom padding
    }

    #[test]
    fn optional_preprocessing_has_identity_defaults_and_adjusts_tones() {
        let original = vec![0.2, 0.8, 0.2, 0.8];
        let mut identity = original.clone();
        preprocess(&mut identity, 2, 2, 2, 1.0, 0.0);
        assert_eq!(identity, original);

        let mut contrast = original.clone();
        preprocess(&mut contrast, 2, 2, 2, 2.0, 0.0);
        assert_eq!(contrast, vec![0.0, 1.0, 0.0, 1.0]);

        let mut sharpened = original;
        preprocess(&mut sharpened, 2, 2, 2, 1.0, 1.0);
        for (actual, expected) in sharpened.iter().zip([0.0, 1.0, 0.0, 1.0]) {
            assert!((actual - expected).abs() < 1e-6);
        }
    }

    #[test]
    fn auto_preprocessing_is_bounded_and_skips_flat_and_line_art() {
        let ramp = RgbaImage::from_fn(32, 16, |x, _| {
            let gray = (40 + x * 5) as u8;
            image::Rgba([gray, gray, gray, 255])
        });
        let ramp_image = DynamicImage::ImageRgba8(ramp.clone());
        let result = convert_with_options(&ramp_image, ConversionOptions::default()).unwrap();
        assert!((1.0..=1.12).contains(&result.contrast));
        assert!((0.0..=1.5).contains(&result.unsharp));
        let mut original = vec![1.0; 32 * 16];
        fill_luminance(&ramp, 32, &mut original);
        let eligible: Vec<usize> = (0..original.len()).collect();
        let mut contrasted = original.clone();
        preprocess(&mut contrasted, 32, 32, 16, result.contrast, 0.0);
        let (rms_contrast, sat_contrast) = stats(&original, &contrasted, &eligible);
        assert!(rms_contrast <= 6.0 / 255.0 + 1e-6 && sat_contrast <= 0.01);
        let mut sharpened = contrasted.clone();
        preprocess(&mut sharpened, 32, 32, 16, 1.0, result.unsharp);
        let (rms_sharp, sat_sharp) = stats(&contrasted, &sharpened, &eligible);
        assert!(rms_sharp <= 12.0 / 255.0 + 1e-6 && sat_sharp <= 0.02);

        for kind in 0..3 {
            let art = RgbaImage::from_fn(32, 16, |x, _| {
                let gray = match kind {
                    0 => 128,          // flat
                    1 if x == 16 => 0, // fine black line on white
                    1 => 255,
                    _ if x < 16 => 0, // saturated black and white
                    _ => 255,
                };
                image::Rgba([gray, gray, gray, 255])
            });
            let result =
                convert_with_options(&DynamicImage::ImageRgba8(art), ConversionOptions::default())
                    .unwrap();
            assert_eq!((result.contrast, result.unsharp), (1.0, 0.0));
            if kind == 1 {
                assert!(result.pixels.iter().any(|&v| v == 0));
            }
        }
    }

    #[test]
    fn auto_search_scores_no_worse_than_unadjusted_on_varied_tones() {
        for kind in 0..2 {
            let rgba = RgbaImage::from_fn(64, 48, |x, y| {
                let gray = if kind == 0 {
                    (20 + x * 3) as u8 // broad grayscale ramp
                } else {
                    (128.0 + 19.0 * (x as f32 / 5.0).sin() + 11.0 * (y as f32 / 7.0).cos()) as u8
                    // low contrast texture
                };
                image::Rgba([gray, gray, gray, 255])
            });
            let image = DynamicImage::ImageRgba8(rgba.clone());
            let auto = convert_with_options(&image, ConversionOptions::default()).unwrap();
            let plain = convert_with_options(
                &image,
                ConversionOptions {
                    contrast: Some(1.0),
                    unsharp: Some(0.0),
                    ..ConversionOptions::default()
                },
            )
            .unwrap();
            let mut original = vec![1.0; 64 * 48];
            fill_luminance(&rgba, 64, &mut original);
            let score_auto = autotune::score(&original, &auto.pixels, 64, 48, 64, 48);
            let score_plain = autotune::score(&original, &plain.pixels, 64, 48, 64, 48);
            assert!(score_auto <= score_plain + 1e-12, "kind {kind}");
        }
    }

    #[test]
    fn transparent_region_stays_white_with_auto_and_diffusion() {
        let image = RgbaImage::from_fn(8, 8, |x, _| {
            if x < 4 {
                image::Rgba([0, 0, 0, 0])
            } else {
                image::Rgba([128, 128, 128, 255])
            }
        });
        let result = convert_with_options(
            &DynamicImage::ImageRgba8(image),
            ConversionOptions::default(),
        )
        .unwrap();
        assert_eq!((result.contrast, result.unsharp), (1.0, 0.0));
        for y in 0..8 {
            assert!(result.pixels[y * 8..y * 8 + 4].iter().all(|&v| v == 1));
        }
    }

    #[test]
    fn cell_residual_diffusion_reduces_uniform_tone_bias() {
        let raw = vec![102; 100 * 20]; // 40% white, 500 cells
        let (old, _) = optimize_gray8(&raw, 100, 20, false).unwrap();
        let (diffused, _) = optimize_gray8(&raw, 100, 20, true).unwrap();
        let old_white: usize = old.iter().map(|&v| v as usize).sum();
        let new_white: usize = diffused.iter().map(|&v| v as usize).sum();
        let target = raw.len() * 102 / 255;
        assert_eq!(old_white, 1000);
        assert!((new_white as isize - target as isize).abs() <= 3);
        assert!(
            (new_white as isize - target as isize).abs()
                < (old_white as isize - target as isize).abs()
        );
    }

    #[test]
    fn rgba_entry_point_matches_dynamic_image_path() {
        // Gradient + dark block + fully and semi-transparent pixels, odd size.
        let image = RgbaImage::from_fn(25, 17, |x, y| {
            if x >= 20 && y < 4 {
                image::Rgba([10, 10, 10, 0])
            } else if x >= 20 {
                image::Rgba([200, 180, 160, 100])
            } else if (6..12).contains(&x) && (4..10).contains(&y) {
                image::Rgba([30, 30, 30, 255])
            } else {
                let gray = (30 + x * 7 + y * 3) as u8;
                image::Rgba([gray, gray, gray, 255])
            }
        });
        let raw = image.as_raw().clone();
        let dynamic = DynamicImage::ImageRgba8(image);
        for options in [
            ConversionOptions::default(),
            ConversionOptions {
                width: Some(16),
                contrast: Some(1.0),
                unsharp: Some(0.0),
                ..ConversionOptions::default()
            },
        ] {
            let via_rgba = convert_rgba(&raw, 25, 17, options).unwrap();
            let via_image = convert_with_options(&dynamic, options).unwrap();
            assert_eq!(via_rgba.width, via_image.width);
            assert_eq!(via_rgba.height, via_image.height);
            assert_eq!(via_rgba.pixels, via_image.pixels);
            assert_eq!(via_rgba.contrast, via_image.contrast);
            assert_eq!(via_rgba.unsharp, via_image.unsharp);
        }
    }

    #[test]
    fn rgba_entry_point_rejects_wrong_length() {
        assert!(convert_rgba(&[0; 10], 4, 4, ConversionOptions::default()).is_err());
        assert!(convert_rgba(&[], 0, 4, ConversionOptions::default()).is_err());
    }

    fn colorful_test_image() -> DynamicImage {
        DynamicImage::ImageRgba8(RgbaImage::from_fn(32, 24, |x, y| {
            if x < 10 {
                image::Rgba([200, (40 + y * 8) as u8, 30, 255]) // warm gradient
            } else if x < 20 {
                image::Rgba([40, (120 + x * 4) as u8, 50, 255]) // green gradient
            } else {
                let v = (60 + x * 5 + y * 4) as u8;
                image::Rgba([v, v, v, 255]) // gray ramp
            }
        }))
    }

    #[test]
    fn adjustment_defaults_report_standard_and_match_explicit_standard() {
        let image = colorful_test_image();
        let defaulted = convert_with_options(&image, ConversionOptions::default()).unwrap();
        assert_eq!(defaulted.mixer, STANDARD_MIXER);
        assert_eq!(defaulted.tone_curve, ToneCurve::Normal);
        assert_eq!(defaulted.local_contrast, 0.0);
        let explicit = convert_with_options(
            &image,
            ConversionOptions {
                mixer: Some(STANDARD_MIXER),
                tone_curve: ToneCurve::Normal,
                local_contrast: 0.0,
                ..ConversionOptions::default()
            },
        )
        .unwrap();
        assert_eq!(defaulted.pixels, explicit.pixels);
    }

    #[test]
    fn mixer_preserves_neutral_grays() {
        // R = G = B everywhere: every sum-to-1 mixer must agree with
        // the standard conversion exactly.
        let image = DynamicImage::ImageRgba8(RgbaImage::from_fn(32, 16, |x, y| {
            let v = (30 + x * 6 + y * 3) as u8;
            image::Rgba([v, v, v, 255])
        }));
        let fixed = ConversionOptions {
            contrast: Some(1.0),
            unsharp: Some(0.0),
            ..ConversionOptions::default()
        };
        let standard = convert_with_options(&image, fixed).unwrap();
        for name in ["warm", "green"] {
            let mixed = convert_with_options(
                &image,
                ConversionOptions {
                    mixer: mixer_preset(name),
                    ..fixed
                },
            )
            .unwrap();
            assert_eq!(mixed.pixels, standard.pixels, "{name}");
        }
    }

    #[test]
    fn mixer_output_is_clamped_to_unit_range() {
        let red = RgbaImage::from_raw(1, 1, vec![255, 0, 0, 255]).unwrap();
        let mut target = vec![0.0; 1];
        fill_mixed(&red, 1, &mut target, (2.0, -1.0, 0.0));
        assert_eq!(target[0], 1.0);
        fill_mixed(&red, 1, &mut target, (-1.0, 0.0, 0.0));
        assert_eq!(target[0], 0.0);
    }

    #[test]
    fn adjustment_options_are_validated() {
        let image = colorful_test_image();
        for options in [
            ConversionOptions {
                mixer: Some((2.5, 0.0, 0.0)),
                ..ConversionOptions::default()
            },
            ConversionOptions {
                mixer: Some((f32::NAN, 0.0, 0.0)),
                ..ConversionOptions::default()
            },
            ConversionOptions {
                local_contrast: 1.5,
                ..ConversionOptions::default()
            },
            ConversionOptions {
                local_contrast: -0.1,
                ..ConversionOptions::default()
            },
        ] {
            assert!(convert_with_options(&image, options).is_err());
        }
    }

    #[test]
    fn tone_curve_and_local_contrast_change_the_output() {
        let image = colorful_test_image();
        let fixed = ConversionOptions {
            contrast: Some(1.0),
            unsharp: Some(0.0),
            ..ConversionOptions::default()
        };
        let plain = convert_with_options(&image, fixed).unwrap();
        let lifted = convert_with_options(
            &image,
            ConversionOptions {
                tone_curve: ToneCurve::LiftBlacks,
                ..fixed
            },
        )
        .unwrap();
        assert_ne!(lifted.pixels, plain.pixels);
        assert_eq!(lifted.tone_curve, ToneCurve::LiftBlacks);
        let local = convert_with_options(
            &image,
            ConversionOptions {
                local_contrast: 0.8,
                ..fixed
            },
        )
        .unwrap();
        assert_ne!(local.pixels, plain.pixels);
        assert_eq!(local.local_contrast, 0.8);
    }

    #[test]
    fn exchange_preserves_white_total_and_changes_pixels() {
        // A smooth gradient: neighbouring cells have room to trade dots.
        let (w, h) = (96u32, 64u32);
        let mut rgba = Vec::with_capacity((w * h * 4) as usize);
        for _y in 0..h {
            for x in 0..w {
                let v = ((x * 255) / (w - 1)) as u8;
                rgba.extend_from_slice(&[v, v, v, 255]);
            }
        }
        let image = DynamicImage::ImageRgba8(RgbaImage::from_raw(w, h, rgba).unwrap());
        let base = ConversionOptions {
            width: Some(w),
            contrast: Some(1.0),
            unsharp: Some(0.0),
            ..ConversionOptions::default()
        };
        let plain = convert_with_options(&image, base).unwrap();
        let exchanged = convert_with_options(
            &image,
            ConversionOptions {
                exchange: 3,
                ..base
            },
        )
        .unwrap();
        assert_eq!(exchanged.exchange, 3);
        // Every exchange preserves its pair's total, hence the global one.
        let total = |p: &[u8]| p.iter().map(|&v| v as u64).sum::<u64>();
        assert_eq!(total(&plain.pixels), total(&exchanged.pixels));
        assert_ne!(plain.pixels, exchanged.pixels);
    }

    #[test]
    fn mode_presets_resolve() {
        let photo = mode_preset("photo").unwrap();
        assert!(photo.diffusion);
        assert_eq!(photo.exchange, MODE_EXCHANGE_ROUNDS);
        let illustration = mode_preset("illustration").unwrap();
        assert!(!illustration.diffusion);
        assert_eq!(illustration.exchange, MODE_EXCHANGE_ROUNDS);
        assert!(mode_preset("bogus").is_none());
    }

    #[test]
    fn encoded_png_matches_written_file() {
        let pixels: Vec<u8> = (0..10 * 6)
            .map(|i| ((i % 10 + i / 10) % 3 != 0) as u8)
            .collect();
        let encoded = encode_1bit_png(10, 6, &pixels).unwrap();
        assert_eq!(&encoded[..8], &[137, 80, 78, 71, 13, 10, 26, 10]);
        assert_eq!(encoded[24], 1); // bit depth in IHDR
        assert_eq!(encoded[25], 0); // grayscale color type in IHDR
        let path =
            std::env::temp_dir().join(format!("pic2gray4px-encode-{}.png", std::process::id()));
        write_png(&path, 10, 6, &pixels, false).unwrap();
        let written = std::fs::read(&path).unwrap();
        std::fs::remove_file(path).unwrap();
        assert_eq!(encoded, written);
    }
}
