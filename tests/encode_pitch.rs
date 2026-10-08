//! A row pitch: rows with a gap encode as the same rows packed.

use libjpeg_turbo_rs::encode::pipeline::{compress_with_params, CompressParams};
use libjpeg_turbo_rs::{JpegError, PixelFormat, Subsampling};

/// RGBA rows of `width` x `height` texels, `gap` bytes of 0xEE after each row but the last.
fn rows(width: usize, height: usize, gap: usize) -> Vec<u8> {
    let mut pixels = Vec::new();
    for y in 0..height {
        for x in 0..width {
            pixels.extend_from_slice(&[(x * 7) as u8, (y * 5) as u8, (x * y) as u8, 0xFF]);
        }
        if y + 1 < height {
            pixels.extend(std::iter::repeat_n(0xEE, gap));
        }
    }
    pixels
}

#[test]
fn rows_with_a_gap_encode_as_the_rows_packed() {
    let (width, height) = (37, 23);
    for (format, subsampling) in [
        (PixelFormat::Rgba, Subsampling::S420),
        (PixelFormat::Rgba, Subsampling::S444),
        (PixelFormat::Bgra, Subsampling::S422),
    ] {
        let packed = compress_with_params(&CompressParams::new(
            &rows(width, height, 0),
            width,
            height,
            format,
            90,
            subsampling,
        ))
        .expect("the packed rows");
        let gapped = compress_with_params(
            &CompressParams::new(
                &rows(width, height, 12),
                width,
                height,
                format,
                90,
                subsampling,
            )
            .pitch(width * 4 + 12),
        )
        .expect("the rows with a gap");
        assert_eq!(gapped, packed, "{format:?} {subsampling:?}");
    }
}

#[test]
fn a_path_that_reads_packed_rows_refuses_a_gap() {
    let (width, height) = (8, 8);
    let gray = vec![128u8; (width + 3) * height];
    let refused = compress_with_params(
        &CompressParams::new(
            &gray,
            width,
            height,
            PixelFormat::Grayscale,
            90,
            Subsampling::S444,
        )
        .pitch(width + 3),
    );
    assert!(
        matches!(refused, Err(JpegError::Unsupported(_))),
        "{refused:?}"
    );
}

#[test]
fn a_pitch_shorter_than_a_row_is_refused() {
    let (width, height) = (8, 8);
    let refused = compress_with_params(
        &CompressParams::new(
            &rows(width, height, 0),
            width,
            height,
            PixelFormat::Rgba,
            90,
            Subsampling::S420,
        )
        .pitch(width * 4 - 1),
    );
    assert!(
        matches!(refused, Err(JpegError::Unsupported(_))),
        "{refused:?}"
    );
}
