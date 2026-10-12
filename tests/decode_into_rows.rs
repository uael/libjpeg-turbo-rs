//! A decode into rows the caller gives one by one writes the rows of the decode into a
//! buffer, whatever the gaps between them and the buffers that hold them.

use libjpeg_turbo_rs::{compress, Decoder, JpegError, PixelFormat, ScalingFactor, Subsampling};

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

/// What a decode into rows `pitch` bytes apart, in buffers of `per_buffer` rows each, writes, as
/// rows packed; the bytes past each row must stay 0xEE.
fn into_rows(
    decoder: &Decoder,
    height: usize,
    row: usize,
    pitch: usize,
    per_buffer: usize,
) -> Vec<u8> {
    let mut memory = vec![0xEE; pitch * height];
    let mut rows: Vec<&mut [u8]> = memory
        .chunks_mut(pitch * per_buffer)
        .flat_map(|buffer| buffer.chunks_mut(pitch))
        .collect();
    let written = decoder
        .decode_image_into_rows(&mut rows)
        .expect("the decode into rows");
    assert_eq!(written.bytes_written, row * height);
    assert!(memory
        .chunks(pitch)
        .all(|line| line[row..].iter().all(|&byte| byte == 0xEE)));
    memory
        .chunks(pitch)
        .flat_map(|line| line[..row].to_vec())
        .collect()
}

#[test]
fn a_decode_into_rows_writes_the_rows_of_the_decode() {
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
            for scale in [1, 2] {
                let mut decoder = Decoder::new(&jpeg).expect("the decoder");
                decoder.set_output_format(format);
                decoder.set_scale(ScalingFactor::new(1, scale));
                let expected = decoder.decode_image().expect("the decode");
                let row = expected.width * format.bytes_per_pixel();
                for (pitch, per_buffer) in [(row, expected.height), (row + 13, 7), (row + 256, 1)] {
                    assert_eq!(
                        into_rows(&decoder, expected.height, row, pitch, per_buffer),
                        expected.data,
                        "{subsampling:?} {format:?} at 1/{scale}, rows {pitch} bytes apart, \
                         {per_buffer} a buffer"
                    );
                }
            }
        }
    }
}

#[test]
fn a_decode_into_too_few_or_too_short_rows_is_an_error() {
    let (width, height) = (16, 16);
    let jpeg = compress(
        &samples(width, height),
        width,
        height,
        PixelFormat::Rgb,
        90,
        Subsampling::S420,
    )
    .expect("the JPEG");
    let mut decoder = Decoder::new(&jpeg).expect("the decoder");
    decoder.set_output_format(PixelFormat::Rgba);
    let row = width * 4;
    for (rows, length) in [(height - 1, row), (height, row - 1)] {
        let mut memory = vec![0; rows * length];
        let mut lines: Vec<&mut [u8]> = memory.chunks_mut(length).collect();
        assert!(matches!(
            decoder.decode_image_into_rows(&mut lines),
            Err(JpegError::BufferTooSmall { .. })
        ));
    }
}

struct Counting;

thread_local! {
    static ALLOCATED: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}

fn record(size: usize) {
    // An allocation after this thread's locals went counts nowhere.
    let _ =
        ALLOCATED.try_with(|allocated| allocated.set(allocated.get().map(|bytes| bytes + size)));
}

// SAFETY: each call goes to the system's allocator with the same arguments.
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

/// The bytes `work` allocates on this thread.
fn allocated(work: impl FnOnce()) -> usize {
    ALLOCATED.with(|allocated| allocated.set(Some(0)));
    work();
    ALLOCATED
        .with(|allocated| allocated.replace(None))
        .unwrap_or(0)
}

/// The YCbCr paths write each row where the caller gave it: the decode allocates its planes and
/// its row scratch, and nothing of the output's size.
#[test]
fn a_decode_into_rows_of_ycbcr_allocates_no_output() {
    let (width, height) = (512, 512);
    let output = width * height * 4;
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
        let mut decoder = Decoder::new(&jpeg).expect("the decoder");
        decoder.set_output_format(PixelFormat::Rgba);
        let mut memory = vec![0; (width * 4 + 256) * height];
        let mut rows: Vec<&mut [u8]> = memory.chunks_mut(width * 4 + 256).collect();
        let bytes = allocated(|| {
            decoder
                .decode_image_into_rows(&mut rows)
                .expect("the decode into rows");
        });
        assert!(
            bytes < output,
            "{subsampling:?}: {bytes} bytes allocated, against {output} of output"
        );
    }
}
