// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::platform::{Buffer, StreamEvent as Event};
use crate::surface_stream::{BufferSizes, StreamFormat, SurfaceStream};
use std::sync::{Mutex, PoisonError};

/// Buffers smaller than the simulated surface in both lengths, with a default size that is smaller still.
const SMALLER: BufferSizes = BufferSizes {
    default: PixelSize::new(SURFACE_SIZE.width / 2, SURFACE_SIZE.height / 4),
    max: PixelSize::new(SURFACE_SIZE.width / 2, SURFACE_SIZE.height / 2),
};

/// A counter and a wake that adds one to it each time the wake runs.
fn counting_wake() -> (Arc<Mutex<u32>>, impl Fn() + Send + Sync + 'static) {
    let count = Arc::new(Mutex::new(0));
    let counter = count.clone();
    let wake = move || *counter.lock().unwrap_or_else(PoisonError::into_inner) += 1;
    (count, wake)
}

fn runs(count: &Mutex<u32>) -> u32 {
    *count.lock().unwrap_or_else(PoisonError::into_inner)
}

#[test]
fn a_resized_stream_shows_the_old_surface_until_the_new_one_delivers() -> Result<(), TestError> {
    let (mut renderer, surface) = attached()?;
    let mut stream = SurfaceStream::new(SURFACE_SIZES, StreamFormat::Rgba8888)?;
    let earlier = producer(&stream)?;
    let earlier_window = stream.native_window().clone();
    earlier.queue(Buffer::new(1, SURFACE_SIZE, skia::Color::RED));
    assert!(matches!(renderer.latch(&mut stream)?, Latch::Frame(_)));
    stream.resize(SMALLER)?;
    let resized = producer(&stream)?;
    assert_eq!(
        (earlier.default_size(), resized.default_size()),
        (SURFACE_SIZE, SMALLER.default)
    );

    earlier.queue(Buffer::new(2, SURFACE_SIZE, skia::Color::RED));
    let shown = latched(&mut renderer, &mut stream)?;
    assert_eq!(
        (shown.size, stream.drawn_size()),
        (SURFACE_SIZE, Some(SURFACE_SIZE))
    );
    assert_eq!(render(&mut renderer, shown)?, FrameOutcome::Shown);
    assert_eq!(surface.pixel(MIDDLE, MIDDLE)?, RED);

    resized.queue(Buffer::new(10, SMALLER.default, skia::Color::BLUE));
    stream.release_window(&earlier_window);
    drop(earlier_window);
    let delivered = latched(&mut renderer, &mut stream)?;
    assert_eq!(
        (delivered.size, stream.drawn_size()),
        (SMALLER.default, Some(SMALLER.default))
    );
    assert_eq!(render(&mut renderer, delivered)?, FrameOutcome::Shown);
    assert_eq!(surface.pixel(MIDDLE, MIDDLE)?, BLUE);
    assert_eq!(
        earlier.events(),
        [
            Event::Released {
                id: 1,
                fenced: true
            },
            Event::Imported(2),
            Event::Acquired(2),
            Event::HandedBack(2),
            Event::Released {
                id: 2,
                fenced: true
            },
            Event::ReaderDeleted,
            Event::ImportDropped(2),
        ]
    );
    Ok(())
}

#[test]
fn a_removed_buffer_or_a_dropped_stream_drops_its_imports_and_its_frame() -> Result<(), TestError> {
    let (mut renderer, surface) = attached()?;
    let mut stream = SurfaceStream::new(SURFACE_SIZES, StreamFormat::Rgba8888)?;
    let producer = producer(&stream)?;
    for id in [1, 2] {
        producer.queue(Buffer::new(id, SURFACE_SIZE, skia::Color::RED));
        let image = latched(&mut renderer, &mut stream)?;
        assert_eq!(render(&mut renderer, image)?, FrameOutcome::Shown);
    }
    producer.clear_events();
    producer.queue(Buffer::new(1, SURFACE_SIZE, skia::Color::RED));
    let image = latched(&mut renderer, &mut stream)?;
    assert_eq!(render(&mut renderer, image)?, FrameOutcome::Shown);
    let reused = [
        Event::Released {
            id: 2,
            fenced: true,
        },
        Event::Acquired(1),
        Event::HandedBack(1),
    ];
    assert_eq!(producer.events(), reused);
    producer.clear_events();
    producer.remove(Buffer::new(2, SURFACE_SIZE, skia::Color::BLUE));
    assert_eq!(renderer.latch(&mut stream)?, Latch::Unchanged);
    assert_eq!(producer.events(), [Event::ImportDropped(2)]);

    producer.clear_events();
    drop(stream);
    let unbound = StreamImage {
        stream: StreamId::next(),
        generation: 1,
        size: SURFACE_SIZE,
    };
    assert_eq!(render(&mut renderer, unbound)?, FrameOutcome::Shown);
    assert_eq!(surface.pixel(MIDDLE, MIDDLE)?, [0; 4]);
    assert_eq!(
        producer.events(),
        [
            Event::ReaderDeleted,
            Event::Released {
                id: 1,
                fenced: false
            },
            Event::ImportDropped(1),
        ]
    );
    Ok(())
}

#[test]
fn every_surface_of_a_stream_wakes_the_host_and_a_crop_sets_drawn_size() -> Result<(), TestError> {
    let (mut renderer, _surface) = attached()?;
    let mut stream = SurfaceStream::new(SURFACE_SIZES, StreamFormat::Rgba8888)?;
    let (woken, wake) = counting_wake();
    stream.on_frame(wake);
    let earlier = producer(&stream)?;
    stream.resize(SURFACE_SIZES)?;
    let resized = producer(&stream)?;
    let mut cropped = Buffer::new(1, SURFACE_SIZE, skia::Color::RED);
    let (left, top, right) = (SIDE / 8, SIDE / 4, SIDE * 3 / 4);
    // The crop reaches past the bottom of the buffer. The latch cuts the crop at the bottom edge of the buffer.
    cropped.crop = Some((left, top, right, SIDE + top));
    earlier.queue(Buffer::new(2, SURFACE_SIZE, skia::Color::RED));
    resized.queue(cropped);
    assert_eq!(runs(&woken), 2);
    let image = latched(&mut renderer, &mut stream)?;
    let drawn = PixelSize::new((right - left).unsigned_abs(), (SIDE - top).unsigned_abs());
    assert_eq!((image.size, stream.drawn_size()), (drawn, Some(drawn)));
    Ok(())
}

#[test]
fn a_second_wake_replaces_the_first_on_every_surface_of_a_stream() -> Result<(), TestError> {
    let mut stream = SurfaceStream::new(SURFACE_SIZES, StreamFormat::Rgba8888)?;
    let (first, first_wake) = counting_wake();
    stream.on_frame(first_wake);
    let earlier = producer(&stream)?;
    stream.resize(SURFACE_SIZES)?;
    let resized = producer(&stream)?;
    let (second, second_wake) = counting_wake();
    stream.on_frame(second_wake);
    earlier.queue(Buffer::new(1, SURFACE_SIZE, skia::Color::RED));
    resized.queue(Buffer::new(2, SURFACE_SIZE, skia::Color::RED));
    assert_eq!((runs(&first), runs(&second)), (0, 2));
    Ok(())
}
