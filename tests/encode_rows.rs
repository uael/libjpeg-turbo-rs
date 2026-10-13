//! Rows written one at a time encode as the whole picture does: `RowEncoder` and
//! `ScanlineEncoder` give the bytes of `compress_with_params`, and of C cjpeg.

mod helpers;

use libjpeg_turbo_rs::encode::pipeline::{compress_with_params, CompressParams, RowEncoder};
use libjpeg_turbo_rs::{DctMethod, JpegError, PixelFormat, ScanlineEncoder, Subsampling};

/// Rows of `width` x `height` pixels of `bpp` bytes: a gradient with a noise of a few levels, so
/// every block codes AC coefficients.
fn picture(width: usize, height: usize, bpp: usize) -> Vec<u8> {
    let mut state: u32 = 0x9E37_79B9;
    let mut pixels: Vec<u8> = Vec::with_capacity(width * height * bpp);
    for y in 0..height {
        for x in 0..width * bpp {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            pixels.push(((x * 3 + y * 5) as u32 % 256 + (state & 15)) as u8);
        }
    }
    pixels
}

#[test]
fn rows_written_one_by_one_encode_as_the_whole_picture() {
    for (width, height) in [(1, 1), (17, 33), (100, 67), (333, 21)] {
        for format in [
            PixelFormat::Rgb,
            PixelFormat::Rgba,
            PixelFormat::Bgr,
            PixelFormat::Bgra,
        ] {
            let bpp: usize = format.bytes_per_pixel();
            let pixels: Vec<u8> = picture(width, height, bpp);
            for subsampling in [
                Subsampling::S444,
                Subsampling::S422,
                Subsampling::S420,
                Subsampling::S440,
                Subsampling::S411,
            ] {
                for restart in [0, 3] {
                    let label: String =
                        format!("{width}x{height} {format:?} {subsampling:?} restart {restart}");
                    let params =
                        CompressParams::new(&pixels, width, height, format, 85, subsampling)
                            .restart_interval(restart);
                    let whole: Vec<u8> = compress_with_params(&params).expect(&label);
                    let mut rows = RowEncoder::new(&params)
                        .expect(&label)
                        .expect("a row encode of samples of three components");
                    for row in pixels.chunks_exact(width * bpp) {
                        rows.write_row(row).expect(&label);
                    }
                    assert_eq!(rows.finish().expect(&label), whole, "{label}");
                }
            }
        }
    }
}

#[test]
fn a_scanline_encode_codes_rows_as_the_whole_picture() {
    let (width, height) = (61, 45);
    for format in [PixelFormat::Rgba, PixelFormat::Grayscale] {
        let bpp: usize = format.bytes_per_pixel();
        let pixels: Vec<u8> = picture(width, height, bpp);
        let whole: Vec<u8> = compress_with_params(
            &CompressParams::new(&pixels, width, height, format, 90, Subsampling::S422)
                .dct_method(DctMethod::IsLow),
        )
        .expect("the whole picture");
        let mut encoder = ScanlineEncoder::new(width, height, format);
        encoder.set_quality(90);
        encoder.set_subsampling(Subsampling::S422);
        for (y, row) in pixels.chunks_exact(width * bpp).enumerate() {
            assert_eq!(encoder.next_scanline(), y);
            encoder.write_scanline(row).expect("a row");
        }
        assert_eq!(encoder.finish().expect("the file"), whole, "{format:?}");
    }
}

#[test]
fn a_row_encode_refuses_rows_past_the_picture_and_a_file_short_of_rows() {
    let (width, height) = (16, 2);
    let pixels: Vec<u8> = picture(width, height, 4);
    let params = CompressParams::new(
        &pixels,
        width,
        height,
        PixelFormat::Rgba,
        90,
        Subsampling::S420,
    );
    let mut rows = RowEncoder::new(&params).unwrap().unwrap();
    assert!(matches!(
        rows.write_row(&pixels[..width * 4 - 1]),
        Err(JpegError::BufferTooSmall { .. })
    ));
    rows.write_row(&pixels[..width * 4]).unwrap();
    let short = RowEncoder::new(&params).unwrap().unwrap();
    assert!(matches!(short.finish(), Err(JpegError::Unsupported(_))));
    rows.write_row(&pixels[width * 4..]).unwrap();
    assert!(matches!(
        rows.write_row(&pixels[width * 4..]),
        Err(JpegError::Unsupported(_))
    ));
    assert!(rows.finish().is_ok());
}

#[test]
fn a_row_encode_takes_no_format_it_cannot_code_a_row_at_a_time() {
    let pixels: Vec<u8> = picture(8, 8, 4);
    for (format, optimize) in [
        (PixelFormat::Grayscale, false),
        (PixelFormat::Cmyk, false),
        (PixelFormat::Rgba, true),
    ] {
        let params = CompressParams::new(&pixels, 8, 8, format, 90, Subsampling::S444)
            .optimize_huffman(optimize);
        assert!(RowEncoder::new(&params).unwrap().is_none(), "{format:?}");
    }
}

#[test]
fn c_xval_a_row_encode_is_the_file_of_cjpeg() {
    let cjpeg = require_c_tool!("cjpeg");
    let (width, height) = (75, 51);
    let pixels: Vec<u8> = picture(width, height, 3);
    for (subsampling, sample) in [
        (Subsampling::S444, "1x1"),
        (Subsampling::S422, "2x1"),
        (Subsampling::S420, "2x2"),
    ] {
        let label: String = format!("rows_{sample}");
        let params = CompressParams::new(&pixels, width, height, PixelFormat::Rgb, 90, subsampling);
        let mut rows = RowEncoder::new(&params).unwrap().unwrap();
        for row in pixels.chunks_exact(width * 3) {
            rows.write_row(row).unwrap();
        }
        let ppm: Vec<u8> = helpers::build_ppm(&pixels, width, height);
        let c_jpeg: Vec<u8> = helpers::encode_with_c_cjpeg(
            &cjpeg,
            &ppm,
            &["-quality", "90", "-sample", sample, "-baseline"],
            &label,
        );
        assert_eq!(rows.finish().unwrap(), c_jpeg, "{label}");
    }
}
