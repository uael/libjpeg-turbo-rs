//! A decode that gives each row to a function gives the rows of the decode, top first, and the
//! rows of a vertical crop alone.

use libjpeg_turbo_rs::{compress, Decoder, PixelFormat, ScalingFactor, Subsampling};

/// RGB samples of `width` x `height` texels that differ from texel to texel.
fn samples(width: usize, height: usize) -> Vec<u8> {
    let mut samples = Vec::with_capacity(width * height * 3);
    for y in 0..height {
        for x in 0..width {
            samples.extend_from_slice(&[(x * 7) as u8, (y * 5) as u8, (x * y) as u8]);
        }
    }
    samples
}

#[test]
fn a_decode_of_rows_gives_the_rows_of_the_decode() {
    let (width, height) = (61, 45);
    for subsampling in [
        Subsampling::S444,
        Subsampling::S422,
        Subsampling::S420,
        Subsampling::S440,
        Subsampling::S411,
    ] {
        let jpeg = compress(
            &samples(width, height),
            width,
            height,
            PixelFormat::Rgb,
            90,
            subsampling,
        )
        .expect("the JPEG");
        for format in [PixelFormat::Rgba, PixelFormat::Rgb, PixelFormat::Grayscale] {
            for scale in [1, 2, 8] {
                for crop in [None, Some((7, 20)), Some((0, 1)), Some((40, 30))] {
                    let decoder = || {
                        let mut decoder = Decoder::new(&jpeg).expect("the decoder");
                        decoder.set_output_format(format);
                        decoder.set_scale(ScalingFactor::new(1, scale));
                        if let Some((y, height)) = crop {
                            decoder.set_crop_y(y, height);
                        }
                        decoder
                    };
                    let expected = decoder().decode_image().expect("the decode");
                    let row = expected.width * format.bytes_per_pixel();
                    let mut rows: Vec<Vec<u8>> = Vec::new();
                    let info = decoder()
                        .decode_image_rows(|line| rows.push(line.to_vec()))
                        .expect("the decode of rows");
                    let label = format!("{subsampling:?} {format:?} at 1/{scale}, crop {crop:?}");
                    assert!(rows.iter().all(|line| line.len() == row), "{label}");
                    assert_eq!(rows.concat(), expected.data, "{label}");
                    assert_eq!(
                        (info.width, info.height, info.bytes_written),
                        (expected.width, expected.height, expected.data.len()),
                        "{label}"
                    );
                }
            }
        }
    }
}

#[test]
fn a_progressive_decode_of_rows_gives_the_rows_of_the_decode() {
    let jpeg = std::fs::read("tests/fixtures/checker_640x480_420_prog.jpg").expect("the fixture");
    let expected = Decoder::new(&jpeg)
        .expect("the decoder")
        .decode_image()
        .expect("the decode");
    let mut rows: Vec<u8> = Vec::new();
    Decoder::new(&jpeg)
        .expect("the decoder")
        .decode_image_rows(|line| rows.extend_from_slice(line))
        .expect("the decode of rows");
    assert_eq!(rows, expected.data);
}
