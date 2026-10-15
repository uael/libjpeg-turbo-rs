//! Borrowed entropy pieces keep the configured slice decoder's output and errors.

mod helpers;

use libjpeg_turbo_rs::decode::marker::MarkerReader;
use libjpeg_turbo_rs::{Decoder, Image, ScalingFactor};

const PHOTO: &[u8] = include_bytes!("fixtures/cjpeg_33x31_420.jpg");

fn start(bytes: &[u8]) -> usize {
    MarkerReader::new(bytes)
        .read_markers()
        .expect("the JPEG header")
        .entropy_data_offset
}

fn same(actual: &Image, expected: &Image) {
    assert_eq!(
        (actual.width, actual.height),
        (expected.width, expected.height)
    );
    assert_eq!(actual.pixel_format, expected.pixel_format);
    same_bytes(&actual.data, &expected.data);
    assert_eq!(actual.icc_profile(), expected.icc_profile());
    assert_eq!(actual.exif_data(), expected.exif_data());
    assert_eq!(actual.warnings, expected.warnings);
}

fn c_pixels(bytes: &[u8]) {
    let Some(djpeg) = helpers::djpeg_path() else {
        eprintln!("SKIP: djpeg not found; contiguous equivalence still runs");
        return;
    };
    let expected = helpers::decode_with_c_djpeg(&djpeg, bytes, "segments");
    let image = Decoder::new(bytes).unwrap().decode_image().unwrap();
    assert_eq!((image.width, image.height), (expected.0, expected.1));
    assert_eq!(image.data, expected.2);
}

#[test]
fn segmented_rows_keep_configured_pixels_and_metadata() {
    c_pixels(PHOTO);
    for bytes in [
        PHOTO,
        include_bytes!("fixtures/cjpeg_33x31_422.jpg").as_slice(),
        include_bytes!("fixtures/cjpeg_16x15_even_odd_444.jpg").as_slice(),
        include_bytes!("fixtures/gray_8x8.jpg").as_slice(),
        include_bytes!("fixtures/cmyk_scanner/scanner_64x64.jpg").as_slice(),
        include_bytes!("fixtures/photo_640x480_420_rst.jpg").as_slice(),
    ] {
        let at = start(bytes);
        for chunk in [1, 2, 7, 31, 4096] {
            let pieces: Vec<&[u8]> = bytes[at..]
                .chunks(chunk)
                .flat_map(|part| [&[][..], part])
                .collect();
            for scale in [1, 2, 4, 8] {
                for crop in [false, true] {
                    let configure = |decoder| configured(decoder, scale, crop);
                    let expected = configure(Decoder::new(bytes).unwrap())
                        .decode_image()
                        .unwrap();
                    let decoder = configure(Decoder::new_segmented(&bytes[..at], &pieces).unwrap());
                    let mut actual: Vec<u8> = Vec::new();
                    let info = decoder
                        .decode_image_rows(|row| actual.extend_from_slice(row))
                        .unwrap();
                    assert_eq!((info.width, info.height), (expected.width, expected.height));
                    same_bytes(&actual, &expected.data);
                    assert_eq!(info.icc_profile.as_deref(), expected.icc_profile());
                    assert_eq!(info.exif_data.as_deref(), expected.exif_data());
                    assert_eq!(info.warnings, expected.warnings);
                    let mut memory = vec![0xEE; expected.data.len() + expected.height * 13];
                    let row = expected.width * expected.pixel_format.bytes_per_pixel();
                    let mut rows: Vec<&mut [u8]> = memory.chunks_mut(row + 13).collect();
                    decoder.decode_image_into_rows(&mut rows).unwrap();
                    for (line, reference) in rows.iter().zip(expected.data.chunks(row)) {
                        assert_eq!(&line[..row], reference);
                        assert!(line[row..].iter().all(|&b| b == 0xEE));
                    }
                }
                let mut expected = Decoder::new(bytes).unwrap();
                expected.set_scale(ScalingFactor::new(1, scale));
                let expected = expected.decode_raw().unwrap();
                let mut actual = Decoder::new_segmented(&bytes[..at], &pieces).unwrap();
                actual.set_scale(ScalingFactor::new(1, scale));
                let actual = actual.decode_raw().unwrap();
                assert_eq!(actual.planes, expected.planes);
                assert_eq!(actual.plane_widths, expected.plane_widths);
                assert_eq!(actual.plane_heights, expected.plane_heights);
            }
        }
    }
}

#[test]
fn every_entropy_boundary_and_short_input_keeps_slice_results() {
    c_pixels(PHOTO);
    let at = start(PHOTO);
    for split in at..=PHOTO.len() {
        let pieces = [&PHOTO[at..split], &[][..], &PHOTO[split..]];
        let expected = Decoder::new(PHOTO).unwrap().decode_image().unwrap();
        let actual = Decoder::new_segmented(&PHOTO[..at], &pieces)
            .unwrap()
            .decode_image()
            .unwrap();
        same(&actual, &expected);
        let pieces = [&PHOTO[at..split]];
        for lenient in [false, true] {
            let configure = |decoder| lenient_decoder(decoder, lenient);
            let expected = configure(Decoder::new(&PHOTO[..split]).unwrap()).decode_image();
            let actual =
                configure(Decoder::new_segmented(&PHOTO[..at], &pieces).unwrap()).decode_image();
            match (actual, expected) {
                (Ok(actual), Ok(expected)) => same(&actual, &expected),
                (Err(actual), Err(expected)) => {
                    assert_eq!(format!("{actual:?}"), format!("{expected:?}"))
                }
                (actual, expected) => {
                    panic!("split={split}: actual={actual:?}, expected={expected:?}")
                }
            }
        }
    }
}

fn configured(mut decoder: Decoder<'_>, scale: u32, crop: bool) -> Decoder<'_> {
    decoder.set_scale(ScalingFactor::new(1, scale));
    if crop {
        decoder.set_crop_region(0, 1, 9, 7);
    }
    decoder
}

fn lenient_decoder(mut decoder: Decoder<'_>, lenient: bool) -> Decoder<'_> {
    decoder.set_lenient(lenient);
    decoder
}

#[test]
fn segmented_metadata_and_corrupt_restart_errors_keep_slice_policy() {
    use libjpeg_turbo_rs::{compress_with_metadata, Encoder, PixelFormat, Subsampling};
    let pixels = Decoder::new(PHOTO).unwrap().decode_image().unwrap().data;
    let icc = vec![0x42; 70_000];
    let exif = [0x49, 0x49, 0x2A, 0, 8, 0, 0, 0, 0, 0];
    let jpeg = compress_with_metadata(
        &pixels,
        33,
        31,
        PixelFormat::Rgb,
        90,
        Subsampling::S420,
        Some(&icc),
        Some(&exif),
    )
    .unwrap();
    c_pixels(&jpeg);
    let at = start(&jpeg);
    let pieces: Vec<&[u8]> = jpeg[at..].chunks(1).collect();
    let expected = Decoder::new(&jpeg).unwrap().decode_image().unwrap();
    let actual = Decoder::new_segmented(&jpeg[..at], &pieces)
        .unwrap()
        .decode_image()
        .unwrap();
    same(&actual, &expected);
    assert_eq!(actual.icc_profile(), Some(icc.as_slice()));
    assert_eq!(actual.exif_data(), Some(exif.as_slice()));

    let jpeg = Encoder::new(&pixels, 33, 31, PixelFormat::Rgb)
        .restart_blocks(1)
        .encode()
        .unwrap();
    c_pixels(&jpeg);
    let at = start(&jpeg);
    let restart = jpeg[at..]
        .windows(2)
        .position(|w| w == [0xFF, 0xD0])
        .unwrap()
        + at;
    let mut wrong = jpeg.clone();
    wrong[restart + 1] = 0xD7;
    let mut fill = jpeg.clone();
    fill.splice(restart..restart, std::iter::repeat_n(0xFF, 10_000));
    let mut corrupt = jpeg.clone();
    corrupt[at] = 0xFF;
    corrupt[at + 1] = 0xC0;
    for bytes in [
        jpeg.as_slice(),
        &wrong,
        &fill,
        &corrupt,
        &jpeg[..jpeg.len() - 1],
        &jpeg[..jpeg.len() - 2],
    ] {
        let pieces: Vec<&[u8]> = bytes[at..]
            .chunks(1)
            .flat_map(|part| [part, &[][..]])
            .collect();
        for lenient in [false, true] {
            for stop in [false, true] {
                let configure = |decoder| error_policy(decoder, lenient, stop);
                let expected = configure(Decoder::new(bytes).unwrap()).decode_image();
                let actual = configure(Decoder::new_segmented(&bytes[..at], &pieces).unwrap())
                    .decode_image();
                match (actual, expected) {
                    (Ok(actual), Ok(expected)) => same(&actual, &expected),
                    (Err(actual), Err(expected)) => {
                        assert_eq!(format!("{actual:?}"), format!("{expected:?}"))
                    }
                    (actual, expected) => {
                        panic!("policy lenient={lenient} stop={stop}: {actual:?} vs {expected:?}")
                    }
                }
            }
        }
    }
}

fn error_policy(mut decoder: Decoder<'_>, lenient: bool, stop: bool) -> Decoder<'_> {
    decoder.set_lenient(lenient);
    decoder.set_stop_on_warning(stop);
    decoder
}

struct Counting;
thread_local! {
    static ALLOCATED: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}
fn record(size: usize) {
    let _ =
        ALLOCATED.try_with(|allocated| allocated.set(allocated.get().map(|bytes| bytes + size)));
}
// SAFETY: each call passes the same arguments to the system allocator.
unsafe impl std::alloc::GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
        record(layout.size());
        unsafe { std::alloc::System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: std::alloc::Layout) {
        unsafe { std::alloc::System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: std::alloc::Layout, size: usize) -> *mut u8 {
        record(size);
        unsafe { std::alloc::System.realloc(ptr, layout, size) }
    }
}
#[global_allocator]
static COUNTING: Counting = Counting;
fn allocated<T>(work: impl FnOnce() -> T) -> (T, usize) {
    ALLOCATED.with(|allocated| allocated.set(Some(0)));
    let result = work();
    let bytes = ALLOCATED.with(|allocated| allocated.replace(None)).unwrap();
    (result, bytes)
}

#[test]
fn segmented_decode_allocates_no_compressed_payload_copy() {
    c_pixels(PHOTO);
    let at = start(PHOTO);
    let mut jpeg = PHOTO.to_vec();
    // Trailing bytes stay accepted as in the slice decoder. They make a hidden
    // input concatenation visible without changing the tiny decoded output.
    jpeg.resize(8 * 1024 * 1024, 0);
    let pieces: Vec<&[u8]> = jpeg[at..].chunks(4096).collect();
    let (expected, before) = allocated(|| Decoder::new(&jpeg).unwrap().decode_image().unwrap());
    let (actual, after) = allocated(|| {
        Decoder::new_segmented(&jpeg[..at], &pieces)
            .unwrap()
            .decode_image()
            .unwrap()
    });
    same(&actual, &expected);
    assert_eq!(
        after, before,
        "borrowed pieces add no input or output staging allocation"
    );
    assert!(after < jpeg.len() / 4);
    eprintln!(
        "slice and segmented each requested {after} Rust allocation bytes; input {} bytes",
        jpeg.len()
    );
}

#[test]
fn segmented_input_keeps_limits_and_requires_a_single_scan_header() {
    use libjpeg_turbo_rs::DecodeLimits;
    c_pixels(PHOTO);
    let at = start(PHOTO);
    let pieces = [&PHOTO[at..]];
    assert!(Decoder::new_segmented(&PHOTO[..at + 1], &pieces).is_err());
    let limits = DecodeLimits {
        max_scans: 0,
        ..DecodeLimits::default()
    };
    let before = Decoder::new_with_limits(&PHOTO[..at], limits)
        .err()
        .unwrap();
    let after = Decoder::new_segmented_with_limits(&PHOTO[..at], &pieces, limits)
        .err()
        .unwrap();
    assert_eq!(format!("{after:?}"), format!("{before:?}"));
    let limits = DecodeLimits {
        max_pixels: 1,
        ..DecodeLimits::default()
    };
    let before = Decoder::new_with_limits(PHOTO, limits)
        .unwrap()
        .decode_image()
        .unwrap_err();
    let after = Decoder::new_segmented_with_limits(&PHOTO[..at], &pieces, limits)
        .unwrap()
        .decode_image()
        .unwrap_err();
    assert_eq!(format!("{after:?}"), format!("{before:?}"));
    let progressive = include_bytes!("fixtures/photo_320x240_420_prog.jpg");
    let at = start(progressive);
    assert!(Decoder::new_segmented(&progressive[..at], &[&progressive[at..]]).is_err());
}

fn same_bytes(actual: &[u8], expected: &[u8]) {
    assert_eq!(actual.len(), expected.len());
    let mismatch = actual
        .iter()
        .zip(expected)
        .enumerate()
        .find_map(|(at, (&a, &b))| (a != b).then_some((at, a, b)));
    assert_eq!(mismatch, None);
}
