//! Thin wasm-bindgen wrapper over the pic2gray4px library.
//! No logic lives here: arguments are repacked into the library's own types
//! and results are exposed through getters.

use pic2gray4px::ConversionOptions;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct ConvertResult {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    contrast_used: f32,
    unsharp_used: f32,
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
}

/// Convert raw RGBA bytes. `target_width: None` keeps the original width,
/// `contrast` / `unsharp: None` select the library's automatic search
/// (the UI maps "off" to 1.0 / 0.0 before calling, as the CLI does).
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
    let result = pic2gray4px::convert_rgba(
        rgba,
        width,
        height,
        ConversionOptions {
            width: target_width,
            passes: passes as usize,
            contrast,
            unsharp,
            diffusion,
        },
    )
    .map_err(|e| JsError::new(&e.to_string()))?;
    Ok(ConvertResult {
        width: result.width,
        height: result.height,
        pixels: result.pixels,
        contrast_used: result.contrast,
        unsharp_used: result.unsharp,
    })
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
