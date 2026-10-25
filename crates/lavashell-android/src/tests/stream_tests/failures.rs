// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::platform::{
    Buffer, GPU, HardwareBufferFormat, HardwareBufferUsage, MEDIA, StreamEvent as Event,
};
use crate::surface_stream::{BufferSizes, StreamFormat, SurfaceStream};

/// A buffer one pixel wider than the largest size drawn.
const TOO_WIDE: PixelSize = PixelSize::new(SURFACE_SIZE.width + 1, SURFACE_SIZE.height);
use lavashell_renderer::{StreamRejection, UnsampledFormat};

fn reported(warnings: &[String], rejection: StreamRejection) -> usize {
    let report = rejection.to_string();
    warnings
        .iter()
        .filter(|warning| warning.starts_with(&report))
        .count()
}

#[test]
fn a_rejected_frame_is_returned_and_the_stream_shows_the_frame_before() -> Result<(), TestError> {
    let (mut renderer, surface) = attached()?;
    let mut stream = SurfaceStream::new(SURFACE_SIZES, StreamFormat::Private)?;
    let producer = producer(&stream)?;
    producer.queue(Buffer::new(1, SURFACE_SIZE, skia::Color::RED));
    let shown = latched(&mut renderer, &mut stream)?;
    assert_eq!(render(&mut renderer, shown)?, FrameOutcome::Shown);
    producer.clear_events();

    let too_large = Buffer::new(2, TOO_WIDE, skia::Color::BLUE);
    let buffer = |id| Buffer::new(id, SURFACE_SIZE, skia::Color::BLUE);
    let mut half_float = buffer(3);
    half_float.format = HardwareBufferFormat::R16G16B16A16_FLOAT;
    let mut layered = buffer(4);
    layered.layers = 2;
    let mut unsampled = buffer(5);
    unsampled.usage = HardwareBufferUsage::CPU_READ_OFTEN;
    let mut protected = buffer(6);
    protected.usage =
        HardwareBufferUsage::GPU_SAMPLED_IMAGE | HardwareBufferUsage::PROTECTED_CONTENT;
    let mut mipmapped = buffer(7);
    mipmapped.usage =
        HardwareBufferUsage::GPU_SAMPLED_IMAGE | HardwareBufferUsage::GPU_MIPMAP_COMPLETE;
    let mut undescribable = buffer(8);
    undescribable.undescribable = true;
    let mut outside = buffer(9);
    // The crop starts at the right edge of the buffer, outside the pixels of the buffer.
    outside.crop = Some((SIDE, 0, SIDE + MIDDLE, SIDE));
    let ycbcr = StreamRejection::BufferFormat(UnsampledFormat::NeedsYcbcrConversion);
    let unusable = StreamRejection::UnusableBuffer;
    let rejected = [
        (too_large, StreamRejection::TooLarge),
        (half_float, ycbcr),
        (layered, unusable),
        (unsampled, unusable),
        (protected, unusable),
        (mipmapped, unusable),
        (undescribable, unusable),
        (outside, StreamRejection::CropOutside),
    ];
    let logged = warnings(|| {
        for (buffer, rejection) in rejected {
            producer.queue(buffer);
            assert_eq!(renderer.latch(&mut stream)?, Latch::Rejected(rejection));
        }
        Ok(())
    })?;
    assert_eq!(render(&mut renderer, shown)?, FrameOutcome::Shown);
    assert_eq!(surface.pixel(MIDDLE, MIDDLE)?, RED);
    assert_eq!(stream.drawn_size(), Some(SURFACE_SIZE));
    let returned = (2..=9).map(|id| Event::Released { id, fenced: true });
    let events = returned.chain([Event::HandedBack(1)]).collect::<Vec<_>>();
    assert_eq!(producer.events(), events);
    assert_eq!(reported(&logged, StreamRejection::TooLarge), 1);
    assert_eq!(reported(&logged, ycbcr), 1);
    // A repeated warning is logged only at counts that are powers of two. The five unusable buffers log it at the
    // first, the second and the fourth buffer.
    assert_eq!(reported(&logged, unusable), 3);
    assert_eq!(reported(&logged, StreamRejection::CropOutside), 1);
    Ok(())
}

#[test]
fn a_frame_that_fails_to_import_leaves_out_only_its_stream() -> Result<(), TestError> {
    let (mut renderer, surface) = attached()?;
    let mut failing = SurfaceStream::new(SURFACE_SIZES, StreamFormat::Rgba8888)?;
    let mut working = SurfaceStream::new(SURFACE_SIZES, StreamFormat::Rgba8888)?;
    let (failing_producer, working_producer) = (producer(&failing)?, producer(&working)?);
    let mut unimportable = Buffer::new(1, SURFACE_SIZE, skia::Color::RED);
    unimportable.fails_import = true;
    failing_producer.queue(unimportable);
    working_producer.queue(Buffer::new(2, SURFACE_SIZE, skia::Color::BLUE));
    let failed = latched(&mut renderer, &mut failing)?;
    let imported = latched(&mut renderer, &mut working)?;
    let left = Rect::new(0.dp(), 0.dp(), MIDDLE.dp(), SIDE.dp());
    let right = Rect::new(MIDDLE.dp(), 0.dp(), (SIDE - MIDDLE).dp(), SIDE.dp());
    let commands =
        [(failed, left), (imported, right)].map(|(image, half)| DrawCommand::StreamImage {
            image,
            destination: half,
            frame: Rect::from_size(half.size()),
        });
    let mut outcome = None;
    let logged = warnings(|| {
        let tree = scene(&commands[..])?;
        outcome = Some(renderer.render(
            tree.at(Duration::ZERO),
            Density::ONE,
            skia::Color::TRANSPARENT,
        )?);
        Ok(())
    })?;
    assert_eq!(outcome, Some(FrameOutcome::Shown));
    let (middle_of_left, middle_of_right) = (MIDDLE / 2, MIDDLE + (SIDE - MIDDLE) / 2);
    assert_eq!(surface.pixel(middle_of_left, MIDDLE)?, [0; 4]);
    assert_eq!(surface.pixel(middle_of_right, MIDDLE)?, BLUE);
    assert_eq!(
        failing_producer.events(),
        [Event::Released {
            id: 1,
            fenced: true
        }]
    );
    assert_eq!(reported(&logged, StreamRejection::ImportFailed), 1);
    Ok(())
}

#[test]
fn a_frame_drawn_without_a_release_fence_returns_once_the_gpu_finished() -> Result<(), TestError> {
    let (mut renderer, _surface) = attached()?;
    let mut stream = SurfaceStream::new(SURFACE_SIZES, StreamFormat::Rgba8888)?;
    let producer = producer(&stream)?;
    producer.queue(Buffer::new(1, SURFACE_SIZE, skia::Color::RED));
    let image = latched(&mut renderer, &mut stream)?;
    assert_eq!(render(&mut renderer, image)?, FrameOutcome::Shown);
    GPU.with(|gpu| {
        gpu.fails_release.set(true);
        gpu.busy.set(true);
    });
    assert!(matches!(
        render(&mut renderer, image),
        Err(TestError::Adapter(Error::StreamsOfDrawnFrame {
            outcome: FrameOutcome::Shown,
            ..
        }))
    ));

    producer.clear_events();
    producer.queue(Buffer::new(2, SURFACE_SIZE, skia::Color::BLUE));
    assert!(matches!(renderer.latch(&mut stream)?, Latch::Frame(_)));
    assert_eq!(producer.events(), []);
    GPU.with(|gpu| gpu.busy.set(false));
    assert_eq!(renderer.latch(&mut stream)?, Latch::Unchanged);
    assert_eq!(
        producer.events(),
        [Event::Released {
            id: 1,
            fenced: false
        }]
    );
    Ok(())
}

#[test]
fn a_frame_drawn_when_no_fence_can_be_submitted_returns_at_teardown() -> Result<(), TestError> {
    let (mut renderer, _surface) = attached()?;
    let mut stream = SurfaceStream::new(SURFACE_SIZES, StreamFormat::Rgba8888)?;
    let producer = producer(&stream)?;
    producer.queue(Buffer::new(1, SURFACE_SIZE, skia::Color::RED));
    let image = latched(&mut renderer, &mut stream)?;
    GPU.with(|gpu| {
        gpu.fails_release.set(true);
        gpu.fails_completion.set(true);
    });
    assert!(matches!(
        render(&mut renderer, image),
        Err(TestError::Adapter(_))
    ));
    GPU.with(|gpu| gpu.fails_completion.set(false));

    producer.clear_events();
    producer.queue(Buffer::new(2, SURFACE_SIZE, skia::Color::BLUE));
    assert!(matches!(renderer.latch(&mut stream)?, Latch::Frame(_)));
    producer.queue(Buffer::new(3, SURFACE_SIZE, skia::Color::BLUE));
    assert!(matches!(renderer.latch(&mut stream)?, Latch::Frame(_)));
    let released = Event::Released {
        id: 1,
        fenced: false,
    };
    assert!(!producer.events().contains(&released));
    drop(renderer);
    assert!(producer.events().contains(&released));
    Ok(())
}

#[test]
fn a_render_on_a_lost_device_reports_every_step_that_failed() -> Result<(), TestError> {
    let (mut renderer, _surface) = attached()?;
    let mut stream = SurfaceStream::new(SURFACE_SIZES, StreamFormat::Rgba8888)?;
    let producer = producer(&stream)?;
    producer.queue(Buffer::new(1, SURFACE_SIZE, skia::Color::RED));
    let image = latched(&mut renderer, &mut stream)?;
    assert_eq!(render(&mut renderer, image)?, FrameOutcome::Shown);
    GPU.with(|gpu| gpu.lost.set(true));
    let failures = match render(&mut renderer, image) {
        Err(TestError::Adapter(Error::Several(failures))) => failures,
        Err(other) => return Err(other),
        Ok(outcome) => return Err(TestError::RenderSucceeded(outcome)),
    };
    assert!(matches!(
        failures.as_slice(),
        [
            Error::Draw(_),
            Error::HandBack(_),
            Error::SubmitCompletion(_),
            Error::RetireImports(_)
        ]
    ));
    Ok(())
}

#[test]
fn a_default_size_beyond_the_maximum_is_refused_where_the_stream_is_made() {
    let sizes = BufferSizes {
        default: TOO_WIDE,
        max: SURFACE_SIZE,
    };
    let refused = SurfaceStream::new(sizes, StreamFormat::Rgba8888);
    assert!(matches!(refused, Err(Error::DefaultBeyondMax(_))));
}

#[test]
fn a_reader_that_hands_out_no_window_fails_where_the_stream_is_made() {
    MEDIA.with(|media| media.fails_window.set(true));
    let failed = SurfaceStream::new(SURFACE_SIZES, StreamFormat::Rgba8888);
    assert!(matches!(failed, Err(Error::ReaderWindow(_))));
}

#[test]
fn when_every_image_is_held_for_the_gpu_the_newest_frame_waits() -> Result<(), TestError> {
    let (mut renderer, _surface) = attached()?;
    let mut stream = SurfaceStream::new(SURFACE_SIZES, StreamFormat::Rgba8888)?;
    let producer = producer(&stream)?;
    GPU.with(|gpu| gpu.busy.set(true));
    for id in 1..=3 {
        producer.queue(Buffer::new(id, SURFACE_SIZE, skia::Color::RED));
        let image = latched(&mut renderer, &mut stream)?;
        GPU.with(|gpu| gpu.fails_release.set(true));
        assert!(matches!(
            render(&mut renderer, image),
            Err(TestError::Adapter(_))
        ));
    }
    producer.queue(Buffer::new(4, SURFACE_SIZE, skia::Color::RED));
    assert!(matches!(renderer.latch(&mut stream)?, Latch::Frame(_)));
    producer.queue(Buffer::new(5, SURFACE_SIZE, skia::Color::BLUE));
    assert_eq!(renderer.latch(&mut stream)?, Latch::Unchanged);
    GPU.with(|gpu| gpu.busy.set(false));
    let newest = latched(&mut renderer, &mut stream)?;
    assert_eq!(stream.drawn_size(), Some(newest.size));
    Ok(())
}

#[test]
fn a_freed_buffer_whose_id_cannot_be_read_fails_one_latch() -> Result<(), TestError> {
    let (mut renderer, _surface) = attached()?;
    let mut stream = SurfaceStream::new(SURFACE_SIZES, StreamFormat::Rgba8888)?;
    let producer = producer(&stream)?;
    let mut unidentifiable = Buffer::new(1, SURFACE_SIZE, skia::Color::RED);
    unidentifiable.unidentifiable = true;
    producer.remove(unidentifiable);
    producer.queue(Buffer::new(2, SURFACE_SIZE, skia::Color::RED));
    assert!(matches!(
        renderer.latch(&mut stream),
        Err(Error::RemovedBufferId(_))
    ));
    assert!(matches!(renderer.latch(&mut stream)?, Latch::Frame(_)));
    Ok(())
}
