//! Set PIC2GRAY4PX_REFERENCE_DIR to the extracted, user-supplied VM reference ZIP.

use pic2gray4px::{optimize_gray8, optimize_gray8_reference};
use std::fs;

#[test]
fn vm_reference_full_and_crop() {
    let Ok(dir) = std::env::var("PIC2GRAY4PX_REFERENCE_DIR") else {
        return;
    };
    for (name, expected_changes) in [
        ("full", vec![658, 241, 64, 22, 4, 3, 1, 0]),
        ("crop", vec![1173, 472, 113, 49, 13, 1, 0]),
    ] {
        let input = fs::read(format!("{dir}/{name}_input_gray8.raw")).unwrap();
        let expected = fs::read(format!("{dir}/{name}_expected_binary8.raw")).unwrap();
        let (actual, changes) = optimize_gray8_reference(&input, 250, 122).unwrap();
        let mismatches = actual
            .iter()
            .zip(&expected)
            .filter(|(a, b)| **a * 255 != **b)
            .count();
        println!("{name}: mismatches={mismatches}, changes={changes:?}");
        assert_eq!(changes, expected_changes);
        assert_eq!(mismatches, 0);
    }
}

#[test]
fn cell_diffusion_reference() {
    let Ok(dir) = std::env::var("PIC2GRAY4PX_CELLFS_DIR") else {
        return;
    };
    for name in ["full_original", "crop_original", "crop_auto"] {
        let input = image::open(format!("{dir}/{name}_gray.png"))
            .unwrap()
            .to_luma8();
        let expected = image::open(format!("{dir}/{name}_cellfs4px.png"))
            .unwrap()
            .to_luma8();
        let (actual, changes) = optimize_gray8(input.as_raw(), 250, 122, true).unwrap();
        let mismatches = actual
            .iter()
            .zip(expected.as_raw())
            .filter(|(a, b)| **a * 255 != **b)
            .count();
        println!("{name}: mismatches={mismatches}, changes={changes:?}");
        assert_eq!(mismatches, 0);
    }
}
