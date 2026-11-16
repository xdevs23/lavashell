// SPDX-License-Identifier: GPL-3.0-only
use super::write_png;
use crate::frame::{CapturedFrame, FrameLayout, PixelFormat};
use std::fs::File;
use std::io::BufReader;

const PADDING: [u8; 4] = [0xEE; 4];

#[derive(Debug, thiserror::Error)]
enum Failure {
    #[error(transparent)]
    Session(#[from] crate::error::SessionError),
    #[error(transparent)]
    File(#[from] std::io::Error),
    #[error(transparent)]
    Decode(#[from] png::DecodingError),
    #[error("the decoder does not know the size of the decoded image")]
    UnknownSize,
}

fn pixel_rows(rows: [[[u8; 4]; 2]; 2]) -> Vec<u8> {
    rows.iter()
        .flat_map(|row| row.iter().flatten().chain(PADDING.iter()).copied())
        .collect()
}

fn frame(format: PixelFormat, inverted: bool) -> CapturedFrame {
    CapturedFrame {
        layout: FrameLayout {
            format,
            width: 2,
            height: 2,
            stride: 12,
        },
        inverted,
    }
}

fn written_png(pixels: &[u8], frame: CapturedFrame) -> Result<(png::ColorType, Vec<u8>), Failure> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("frame.png");
    write_png(pixels, frame, &path)?;
    let mut reader = png::Decoder::new(BufReader::new(File::open(&path)?)).read_info()?;
    let mut decoded = vec![0; reader.output_buffer_size().ok_or(Failure::UnknownSize)?];
    let info = reader.next_frame(&mut decoded)?;
    decoded.truncate(info.buffer_size());
    Ok((info.color_type, decoded))
}

#[test]
fn an_xrgb_frame_becomes_rgb_rows_without_the_stride_padding() -> Result<(), Failure> {
    let pixels = pixel_rows([
        [[1, 2, 3, 0xFF], [4, 5, 6, 0x00]],
        [[7, 8, 9, 0x10], [10, 11, 12, 0x20]],
    ]);
    assert_eq!(
        written_png(&pixels, frame(PixelFormat::Xrgb8888, false))?,
        (
            png::ColorType::Rgb,
            vec![3, 2, 1, 6, 5, 4, 9, 8, 7, 12, 11, 10]
        )
    );
    Ok(())
}

#[test]
fn an_argb_frame_keeps_its_alpha() -> Result<(), Failure> {
    let pixels = pixel_rows([
        [[1, 2, 3, 0xFF], [4, 5, 6, 0x00]],
        [[7, 8, 9, 0x10], [10, 11, 12, 0x20]],
    ]);
    assert_eq!(
        written_png(&pixels, frame(PixelFormat::Argb8888, false))?,
        (
            png::ColorType::Rgba,
            vec![
                3, 2, 1, 0xFF, 6, 5, 4, 0x00, 9, 8, 7, 0x10, 12, 11, 10, 0x20
            ]
        )
    );
    Ok(())
}

#[test]
fn an_inverted_frame_is_written_bottom_row_first() -> Result<(), Failure> {
    let pixels = pixel_rows([
        [[1, 2, 3, 0], [4, 5, 6, 0]],
        [[7, 8, 9, 0], [10, 11, 12, 0]],
    ]);
    assert_eq!(
        written_png(&pixels, frame(PixelFormat::Xrgb8888, true))?,
        (
            png::ColorType::Rgb,
            vec![9, 8, 7, 12, 11, 10, 3, 2, 1, 6, 5, 4]
        )
    );
    Ok(())
}
