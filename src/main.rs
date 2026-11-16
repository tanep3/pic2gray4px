use pic2gray4px::{
    convert_with_options, load_image, mixer_preset, mode_preset, write_png, ConversionOptions,
    Result, ToneCurve,
};
use std::env;
use std::path::{Path, PathBuf};

fn usage() -> &'static str {
    "Usage: pic2gray4px <input-image> [output.png] [--width PIXELS] [--contrast auto|off|C] [--unsharp auto|off|A] [--mixer NAME] [--tone-curve NAME] [--local-contrast A] [--no-diffusion] [--exchange N] [--mode NAME] [--passes N] [--force]\n\
     Converts a photo to a 1-bit grayscale PNG using 2x2 neighborhood gray.\n\
     Options:\n\
       --width PIXELS  Resize proportionally before conversion (e.g. 384 for T02)\n\
       --contrast MODE Contrast auto|off|0..3 (default: auto)\n\
       --unsharp MODE  Edge sharpening auto|off|0..2 (default: auto)\n\
       --mixer NAME    Color-to-gray mixer preset: standard|warm|green (default: standard)\n\
       --mixer-weights R,G,B  Custom mixer weights, each -2..2 (cannot be combined with --mixer)\n\
       --tone-curve NAME  Tone curve: normal|contrast|lift-blacks (default: normal)\n\
       --local-contrast A  Local contrast amount 0..1 (default: 0)\n\
       --no-diffusion  Use original independent cell white counts\n\
       --exchange N    Inter-cell white-dot exchange rounds 0..10 (default: 0)\n\
       --mode NAME     Mode preset: photo (diffusion + exchange) | illustration (exchange only)\n\
       --passes N      Maximum optimization sweeps 0..10 (default: 10)\n\
       --force         Overwrite an existing output file\n\
       -h, --help      Show this help"
}

fn default_output(input: &Path) -> PathBuf {
    let stem = input.file_stem().unwrap_or_default().to_string_lossy();
    input.with_file_name(format!("{stem}-4px.png"))
}

fn run() -> Result<()> {
    let mut args = env::args_os().skip(1);
    let mut input: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut options = ConversionOptions::default();
    let mut mixer_source: Option<&'static str> = None;
    let mut force = false;
    while let Some(arg) = args.next() {
        if arg == "-h" || arg == "--help" {
            println!("{}", usage());
            return Ok(());
        } else if arg == "--width" {
            let value = args.next().ok_or("--width needs a positive integer")?;
            let parsed: u32 = value.to_string_lossy().parse()?;
            if parsed == 0 {
                return Err("--width must be greater than zero".into());
            }
            options.width = Some(parsed);
        } else if arg == "--passes" {
            let value = args.next().ok_or("--passes needs an integer")?;
            options.passes = value.to_string_lossy().parse()?;
            if options.passes > 10 {
                return Err("--passes must be between 0 and 10".into());
            }
        } else if arg == "--contrast" {
            let value = args.next().ok_or("--contrast needs a number")?;
            options.contrast = match value.to_string_lossy().as_ref() {
                "auto" => None,
                "off" => Some(1.0),
                text => {
                    let parsed: f32 = text.parse()?;
                    if !parsed.is_finite() || !(0.0..=3.0).contains(&parsed) {
                        return Err("--contrast must be auto, off, or between 0 and 3".into());
                    }
                    Some(parsed)
                }
            };
        } else if arg == "--unsharp" {
            let value = args.next().ok_or("--unsharp needs a number")?;
            options.unsharp = match value.to_string_lossy().as_ref() {
                "auto" => None,
                "off" => Some(0.0),
                text => {
                    let parsed: f32 = text.parse()?;
                    if !parsed.is_finite() || !(0.0..=2.0).contains(&parsed) {
                        return Err("--unsharp must be auto, off, or between 0 and 2".into());
                    }
                    Some(parsed)
                }
            };
        } else if arg == "--mixer" {
            let value = args.next().ok_or("--mixer needs a preset name")?;
            let name = value.to_string_lossy();
            options.mixer = Some(mixer_preset(&name).ok_or_else(|| {
                format!("unknown mixer preset: {name} (standard, warm, green)")
            })?);
            if mixer_source == Some("--mixer-weights") {
                return Err("--mixer and --mixer-weights cannot be combined".into());
            }
            mixer_source = Some("--mixer");
        } else if arg == "--mixer-weights" {
            let value = args.next().ok_or("--mixer-weights needs \"R,G,B\"")?;
            let text = value.to_string_lossy();
            let parts: Vec<&str> = text.split(',').collect();
            if parts.len() != 3 {
                return Err("--mixer-weights needs three numbers \"R,G,B\"".into());
            }
            let mut weights = [0.0_f32; 3];
            for (slot, part) in weights.iter_mut().zip(&parts) {
                *slot = part.trim().parse()?;
                if !slot.is_finite() || !(-2.0..=2.0).contains(slot) {
                    return Err("--mixer-weights values must be between -2 and 2".into());
                }
            }
            options.mixer = Some((weights[0], weights[1], weights[2]));
            if mixer_source == Some("--mixer") {
                return Err("--mixer and --mixer-weights cannot be combined".into());
            }
            mixer_source = Some("--mixer-weights");
        } else if arg == "--tone-curve" {
            let value = args.next().ok_or("--tone-curve needs a name")?;
            let name = value.to_string_lossy();
            options.tone_curve = ToneCurve::from_name(&name).ok_or_else(|| {
                format!("unknown tone curve: {name} (normal, contrast, lift-blacks)")
            })?;
        } else if arg == "--local-contrast" {
            let value = args.next().ok_or("--local-contrast needs a number")?;
            let parsed: f32 = value.to_string_lossy().parse()?;
            if !parsed.is_finite() || !(0.0..=1.0).contains(&parsed) {
                return Err("--local-contrast must be between 0 and 1".into());
            }
            options.local_contrast = parsed;
        } else if arg == "--no-diffusion" {
            options.diffusion = false;
        } else if arg == "--exchange" {
            let value = args.next().ok_or("--exchange needs an integer")?;
            options.exchange = value.to_string_lossy().parse()?;
            if options.exchange > 10 {
                return Err("--exchange must be between 0 and 10".into());
            }
        } else if arg == "--mode" {
            let value = args.next().ok_or("--mode needs a name")?;
            let name = value.to_string_lossy();
            let preset = mode_preset(&name).ok_or_else(|| {
                format!("unknown mode: {name} (photo, illustration)")
            })?;
            options.diffusion = preset.diffusion;
            options.exchange = preset.exchange;
        } else if arg == "--force" {
            force = true;
        } else if arg.to_string_lossy().starts_with('-') {
            return Err(format!("unknown option: {}", arg.to_string_lossy()).into());
        } else if input.is_none() {
            input = Some(arg.into());
        } else if output.is_none() {
            output = Some(arg.into());
        } else {
            return Err("too many file arguments".into());
        }
    }
    let input = input.ok_or_else(|| format!("missing input image\n{}", usage()))?;
    let output = output.unwrap_or_else(|| default_output(&input));
    if input == output
        || (input.exists() && output.exists() && input.canonicalize()? == output.canonicalize()?)
    {
        return Err("input and output must be different files".into());
    }
    if output.exists() && !force {
        return Err(format!(
            "output already exists: {} (use --force to overwrite)",
            output.display()
        )
        .into());
    }
    let image = load_image(&input)?;
    let converted = convert_with_options(&image, options)?;
    write_png(
        &output,
        converted.width,
        converted.height,
        &converted.pixels,
        force,
    )?;
    println!(
        "{} ({}x{}, 1-bit grayscale PNG; mixer=({:.2}, {:.2}, {:.2}); tone-curve={}; local-contrast={:.2}; contrast={:.2}{}; unsharp={:.2}{}; cell diffusion={}; exchange={})",
        output.display(),
        converted.width,
        converted.height,
        converted.mixer.0,
        converted.mixer.1,
        converted.mixer.2,
        converted.tone_curve.name(),
        converted.local_contrast,
        converted.contrast,
        if options.contrast.is_none() {
            " auto"
        } else {
            ""
        },
        converted.unsharp,
        if options.unsharp.is_none() {
            " auto"
        } else {
            ""
        },
        if options.diffusion { "on" } else { "off" },
        converted.exchange
    );
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("pic2gray4px: {error}");
        std::process::exit(1);
    }
}
