use pic2gray4px::{convert_with_options, load_image, write_png, ConversionOptions, Result};
use std::env;
use std::path::{Path, PathBuf};

fn usage() -> &'static str {
    "Usage: pic2gray4px <input-image> [output.png] [--width PIXELS] [--contrast auto|off|C] [--unsharp auto|off|A] [--no-diffusion] [--passes N] [--force]\n\
     Converts a photo to a 1-bit grayscale PNG using 2x2 neighborhood gray.\n\
     Options:\n\
       --width PIXELS  Resize proportionally before conversion (e.g. 384 for T02)\n\
       --contrast MODE Contrast auto|off|0..3 (default: auto)\n\
       --unsharp MODE  Edge sharpening auto|off|0..2 (default: auto)\n\
       --no-diffusion  Use original independent cell white counts\n\
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
        } else if arg == "--no-diffusion" {
            options.diffusion = false;
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
        "{} ({}x{}, 1-bit grayscale PNG; contrast={:.2}{}; unsharp={:.2}{}; cell diffusion={})",
        output.display(),
        converted.width,
        converted.height,
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
        if options.diffusion { "on" } else { "off" }
    );
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("pic2gray4px: {error}");
        std::process::exit(1);
    }
}
