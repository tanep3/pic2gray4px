//! Thin wasm-bindgen wrapper over the pic2gray4px library.
//! No logic lives here: arguments are repacked into the library's own types
//! and results are exposed through getters.

use pic2gray4px::{ConversionOptions, ToneCurve};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct ConvertResult {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    contrast_used: f32,
    unsharp_used: f32,
    mixer_used: Vec<f32>,
    tone_curve_used: String,
    local_contrast_used: f32,
}

#[wasm_bindgen]
impl ConvertResult {
    #[wasm_bindgen(getter)]
    pub fn width(&self) -> u32 {
        self.width
    }

    #[wasm_bindgen(getter)]
    pub fn height(&self) -> u32 {
        self.height
    }

    #[wasm_bindgen(getter)]
    pub fn pixels(&self) -> Vec<u8> {
        self.pixels.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn contrast_used(&self) -> f32 {
        self.contrast_used
    }

    #[wasm_bindgen(getter)]
    pub fn unsharp_used(&self) -> f32 {
        self.unsharp_used
    }

    /// Mixer weights actually applied, as [R, G, B].
    #[wasm_bindgen(getter)]
    pub fn mixer_used(&self) -> Vec<f32> {
        self.mixer_used.clone()
    }

    /// Tone curve actually applied, by its core name.
    #[wasm_bindgen(getter)]
    pub fn tone_curve_used(&self) -> String {
        self.tone_curve_used.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn local_contrast_used(&self) -> f32 {
        self.local_contrast_used
    }
}

fn run(
    rgba: &[u8],
    width: u32,
    height: u32,
    options: ConversionOptions,
) -> std::result::Result<ConvertResult, JsError> {
    let result = pic2gray4px::convert_rgba(rgba, width, height, options)
        .map_err(|e| JsError::new(&e.to_string()))?;
    Ok(ConvertResult {
        width: result.width,
        height: result.height,
        pixels: result.pixels,
        contrast_used: result.contrast,
        unsharp_used: result.unsharp,
        mixer_used: vec![result.mixer.0, result.mixer.1, result.mixer.2],
        tone_curve_used: result.tone_curve.name().to_string(),
        local_contrast_used: result.local_contrast,
    })
}

/// Convert raw RGBA bytes. `target_width: None` keeps the original width,
/// `contrast` / `unsharp: None` select the library's automatic search
/// (the UI maps "off" to 1.0 / 0.0 before calling, as the CLI does).
/// Color adjustment stays at the library defaults (standard mixer,
/// normal tone curve, no local contrast), so results are identical to
/// the pre-adjustment releases.
#[wasm_bindgen]
pub fn convert_rgba(
    rgba: &[u8],
    width: u32,
    height: u32,
    target_width: Option<u32>,
    contrast: Option<f32>,
    unsharp: Option<f32>,
    passes: u32,
    diffusion: bool,
) -> std::result::Result<ConvertResult, JsError> {
    run(
        rgba,
        width,
        height,
        ConversionOptions {
            width: target_width,
            passes: passes as usize,
            contrast,
            unsharp,
            diffusion,
            ..ConversionOptions::default()
        },
    )
}

/// Convert raw RGBA bytes with color adjustment. Presets are named and
/// resolved by the library (the single source of truth for the preset
/// table); `None` for a name or amount selects the library default for
/// that stage. `mixer_preset_name` and `mixer_weights` are mutually
/// exclusive. See `docs/adjustment-design.md`.
#[wasm_bindgen]
#[allow(clippy::too_many_arguments)]
pub fn convert_rgba_adjusted(
    rgba: &[u8],
    width: u32,
    height: u32,
    target_width: Option<u32>,
    contrast: Option<f32>,
    unsharp: Option<f32>,
    passes: u32,
    diffusion: bool,
    mixer_preset_name: Option<String>,
    mixer_weights: Option<Vec<f32>>,
    tone_curve_name: Option<String>,
    local_contrast: Option<f32>,
) -> std::result::Result<ConvertResult, JsError> {
    if mixer_preset_name.is_some() && mixer_weights.is_some() {
        return Err(JsError::new(
            "mixer_preset_name and mixer_weights cannot be combined",
        ));
    }
    let mixer = match (mixer_preset_name, mixer_weights) {
        (Some(name), _) => Some(
            pic2gray4px::mixer_preset(&name)
                .ok_or_else(|| JsError::new(&format!("unknown mixer preset: {name}")))?,
        ),
        (_, Some(weights)) => {
            if weights.len() != 3 {
                return Err(JsError::new("mixer_weights must have exactly 3 values"));
            }
            Some((weights[0], weights[1], weights[2]))
        }
        (None, None) => None,
    };
    let tone_curve = match tone_curve_name {
        Some(name) => ToneCurve::from_name(&name)
            .ok_or_else(|| JsError::new(&format!("unknown tone curve: {name}")))?,
        None => ToneCurve::Normal,
    };
    run(
        rgba,
        width,
        height,
        ConversionOptions {
            width: target_width,
            passes: passes as usize,
            contrast,
            unsharp,
            diffusion,
            mixer,
            tone_curve,
            local_contrast: local_contrast.unwrap_or(0.0),
        },
    )
}

/// Encode binary pixels (one 0/1 byte per pixel) as a 1-bit grayscale PNG.
#[wasm_bindgen]
pub fn encode_1bit_png(
    width: u32,
    height: u32,
    pixels: &[u8],
) -> std::result::Result<Vec<u8>, JsError> {
    pic2gray4px::encode_1bit_png(width, height, pixels)
        .map_err(|e| JsError::new(&e.to_string()))
}
