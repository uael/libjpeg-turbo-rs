//! A transform whose file goes to a function gives the bytes of the transform's own file.

use libjpeg_turbo_rs::{
    compress, transform_jpeg_to, transform_jpeg_with_options, MarkerCopyMode, PixelFormat,
    Subsampling, TransformOp, TransformOptions,
};

#[test]
fn a_transform_to_a_function_gives_the_file_of_the_transform() {
    let (width, height) = (77, 41);
    let samples: Vec<u8> = (0..width * height * 3)
        .map(|i| (i * 13 % 251) as u8)
        .collect();
    let jpeg = compress(
        &samples,
        width,
        height,
        PixelFormat::Rgb,
        85,
        Subsampling::S420,
    )
    .expect("the JPEG");
    let exif =
        std::fs::read("tests/fixtures/exif_orientation/orient_6_16x8.jpg").expect("the fixture");
    for source in [jpeg, exif] {
        for (op, optimize, restart_interval, progressive, copy_markers) in [
            (TransformOp::None, false, 0, false, MarkerCopyMode::All),
            (TransformOp::None, true, 1, false, MarkerCopyMode::All),
            (TransformOp::HFlip, true, 0, false, MarkerCopyMode::None),
            (TransformOp::Rot90, false, 2, false, MarkerCopyMode::IccOnly),
            (TransformOp::None, false, 0, true, MarkerCopyMode::All),
        ] {
            let options = TransformOptions {
                op,
                optimize,
                restart_interval,
                progressive,
                copy_markers,
                ..Default::default()
            };
            let label = format!("{op:?} optimize {optimize} restart {restart_interval} progressive {progressive} {copy_markers:?}");
            let whole = transform_jpeg_with_options(&source, &options).expect(&label);
            let mut parts: Vec<Vec<u8>> = Vec::new();
            transform_jpeg_to(&source, &options, &mut |part| parts.push(part.to_vec()))
                .expect(&label);
            assert_eq!(parts.concat(), whole, "{label}");
            if !progressive {
                // A sequential file goes as its header, its entropy-coded data and EOI.
                assert_eq!(parts.len(), 3, "{label}");
                assert_eq!(parts[2], [0xFF, 0xD9], "{label}");
            }
        }
    }
}

#[test]
fn a_transform_of_no_output_gives_nothing() {
    let samples: Vec<u8> = vec![128; 16 * 16 * 3];
    let jpeg =
        compress(&samples, 16, 16, PixelFormat::Rgb, 85, Subsampling::S444).expect("the JPEG");
    let mut given = 0;
    transform_jpeg_to(
        &jpeg,
        &TransformOptions {
            no_output: true,
            ..Default::default()
        },
        &mut |_| given += 1,
    )
    .expect("the transform");
    assert_eq!(given, 0);
}
