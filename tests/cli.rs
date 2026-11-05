use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("pic2gray4px-cli-{}-{nonce}", std::process::id()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn run(args: &[&Path]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_pic2gray4px"));
    command.args(args);
    command.output().unwrap()
}

fn assert_error(output: Output, message: &str) {
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(message),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn default_name_safe_overwrite_and_deterministic_output() {
    let fixture = Fixture::new();
    let input = fixture.path("photo.pgm");
    let output = fixture.path("photo-4px.png");
    fs::write(&input, b"P5\n3 1\n255\n\x00\x80\xff").unwrap();

    assert!(run(&[&input]).status.success());
    let first = fs::read(&output).unwrap();
    assert_eq!(&first[..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(&first[16..24], &[0, 0, 0, 4, 0, 0, 0, 2]); // odd input padded to 4x2
    assert_eq!(&first[24..26], &[1, 0]); // 1-bit grayscale

    fs::write(&output, b"old output").unwrap();
    assert_error(run(&[&input]), "use --force to overwrite");
    assert_eq!(fs::read(&output).unwrap(), b"old output");
    assert!(run(&[&input, Path::new("--force")]).status.success());
    assert_eq!(fs::read(&output).unwrap(), first);
}

#[test]
fn rejects_bad_arguments_and_same_file_without_overwriting_input() {
    let fixture = Fixture::new();
    let input = fixture.path("photo.pgm");
    let original = b"P5\n2 2\n255\n\x00\x40\x80\xff";
    fs::write(&input, original).unwrap();

    assert_error(run(&[]), "missing input image");
    assert_error(
        run(&[&input, Path::new("--width"), Path::new("0")]),
        "--width must be greater than zero",
    );
    assert_error(run(&[&input, Path::new("--unknown")]), "unknown option");
    assert_error(
        run(&[&input, Path::new("--contrast"), Path::new("3.1")]),
        "--contrast must be auto, off, or between 0 and 3",
    );
    assert_error(
        run(&[&input, Path::new("--unsharp"), Path::new("-0.1")]),
        "--unsharp must be auto, off, or between 0 and 2",
    );
    assert_error(
        run(&[&input, Path::new("--passes"), Path::new("11")]),
        "--passes must be between 0 and 10",
    );
    assert_error(
        run(&[&input, &input]),
        "input and output must be different files",
    );
    assert_error(
        run(&[&input, &input, Path::new("--force")]),
        "input and output must be different files",
    );
    assert_eq!(fs::read(input).unwrap(), original);
}

#[test]
fn preprocessing_flags_are_optional_and_accepted_in_range() {
    let fixture = Fixture::new();
    let input = fixture.path("photo.pgm");
    let plain = fixture.path("plain.png");
    let adjusted = fixture.path("adjusted.png");
    fs::write(&input, b"P5\n2 2\n255\n\x66\x66\x66\x66").unwrap();

    assert!(run(&[&input, &plain]).status.success());
    let adjusted_run = run(&[
        &input,
        &adjusted,
        Path::new("--contrast"),
        Path::new("3"),
        Path::new("--unsharp"),
        Path::new("1.5"),
    ]);
    assert!(adjusted_run.status.success());
    assert!(String::from_utf8_lossy(&adjusted_run.stdout).contains("contrast=3.00"));
    assert!(String::from_utf8_lossy(&adjusted_run.stdout).contains("unsharp=1.50"));
    assert_ne!(fs::read(plain).unwrap(), fs::read(adjusted).unwrap());
}

#[test]
fn off_switches_and_original_count_mode_are_reported() {
    let fixture = Fixture::new();
    let input = fixture.path("photo.pgm");
    let output = fixture.path("original-mode.png");
    fs::write(&input, b"P5\n4 2\n255\n\x66\x66\x66\x66\x66\x66\x66\x66").unwrap();
    let result = run(&[
        &input,
        &output,
        Path::new("--contrast"),
        Path::new("off"),
        Path::new("--unsharp"),
        Path::new("off"),
        Path::new("--no-diffusion"),
    ]);
    assert!(result.status.success());
    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(stdout.contains("contrast=1.00"));
    assert!(stdout.contains("unsharp=0.00"));
    assert!(stdout.contains("cell diffusion=off"));
}
