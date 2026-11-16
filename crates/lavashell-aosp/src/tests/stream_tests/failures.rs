// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::platform::{Buffer, HardwareBufferFormat, HardwareBufferUsage, StreamEvent as Event};
use lavashell_renderer::{StreamRejection, UnsampledFormat};

const TOO_WIDE: PixelSize = PixelSize::new(SURFACE_SIZE.width + 1, SURFACE_SIZE.height);

fn reported(warnings: &[String], rejection: StreamRejection) -> usize {
    let report = rejection.to_string();
    warnings
        .iter()
        .filter(|warning| warning.starts_with(&report))
        .count()
}

#[test]
fn a_rejected_frame_is_returned_and_the_stream_shows_the_frame_before() -> Result<(), TestError> {
    let mut renderer = Display::new()?;
    let stream = renderer.create_stream(surface_sizes()?, StreamFormat::Private)?;
    let producer = renderer.producer(stream)?;
    producer.queue(Buffer::new(1, SURFACE_SIZE, skia::Color::RED));
    let shown = renderer.latched(stream)?;
    assert_eq!(renderer.render(shown)?, FrameOutcome::Shown);
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
            assert_eq!(renderer.latch(stream)?, Latch::Rejected(rejection));
        }
        Ok(())
    })?;
    assert_eq!(renderer.render(shown)?, FrameOutcome::Shown);
    assert_eq!(renderer.pixel(MIDDLE, MIDDLE)?, RED);
    assert_eq!(renderer.stream(stream)?.drawn_size(), Some(SURFACE_SIZE));
    let returned = (2..=9).map(|id| Event::Released { id, fenced: true });
    let events = returned.chain([Event::HandedBack(1)]).collect::<Vec<_>>();
    assert_eq!(producer.events(), events);
    assert_eq!(reported(&logged, StreamRejection::TooLarge), 1);
    assert_eq!(reported(&logged, ycbcr), 1);
    assert_eq!(reported(&logged, unusable), 3);
    assert_eq!(reported(&logged, StreamRejection::CropOutside), 1);
    Ok(())
}

#[test]
fn a_frame_that_fails_to_import_leaves_out_only_its_stream() -> Result<(), TestError> {
    let mut renderer = Display::new()?;
    let failing = renderer.create_stream(surface_sizes()?, StreamFormat::Rgba8888)?;
    let working = renderer.create_stream(surface_sizes()?, StreamFormat::Rgba8888)?;
    let failing_producer = renderer.producer(failing)?;
    let working_producer = renderer.producer(working)?;
    let mut unimportable = Buffer::new(1, SURFACE_SIZE, skia::Color::RED);
    unimportable.fails_import = true;
    failing_producer.queue(unimportable);
    working_producer.queue(Buffer::new(2, SURFACE_SIZE, skia::Color::BLUE));
    let failed = renderer.latched(failing)?;
    let imported = renderer.latched(working)?;
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
        let (frame, streams) = renderer.draw(&scene(&commands[..])?);
        outcome = Some(frame?);
        streams?;
        Ok(())
    })?;
    assert_eq!(outcome, Some(FrameOutcome::Shown));
    let (middle_of_left, middle_of_right) = (MIDDLE / 2, MIDDLE + (SIDE - MIDDLE) / 2);
    assert_eq!(renderer.pixel(middle_of_left, MIDDLE)?, [0; 4]);
    assert_eq!(renderer.pixel(middle_of_right, MIDDLE)?, BLUE);
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
    let mut renderer = Display::new()?;
    let stream = renderer.create_stream(surface_sizes()?, StreamFormat::Rgba8888)?;
    let producer = renderer.producer(stream)?;
    producer.queue(Buffer::new(1, SURFACE_SIZE, skia::Color::RED));
    let image = renderer.latched(stream)?;
    assert_eq!(renderer.render(image)?, FrameOutcome::Shown);
    let device = renderer.device();
    device.fails_release.set(true);
    device.busy.set(true);
    assert!(matches!(
        renderer.draw(&scene(whole_surface(image))?),
        (Ok(FrameOutcome::Shown), Err(Error::HandBack(_)))
    ));

    producer.clear_events();
    producer.queue(Buffer::new(2, SURFACE_SIZE, skia::Color::BLUE));
    assert!(matches!(renderer.latch(stream)?, Latch::Frame(_)));
    assert_eq!(producer.events(), []);
    renderer.device().busy.set(false);
    assert_eq!(renderer.latch(stream)?, Latch::Unchanged);
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
    let mut renderer = Display::new()?;
    let stream = renderer.create_stream(surface_sizes()?, StreamFormat::Rgba8888)?;
    let producer = renderer.producer(stream)?;
    producer.queue(Buffer::new(1, SURFACE_SIZE, skia::Color::RED));
    let image = renderer.latched(stream)?;
    let device = renderer.device();
    device.fails_release.set(true);
    device.fails_completion.set(true);
    assert!(matches!(
        renderer.render(image),
        Err(TestError::Adapter(Error::Several(_)))
    ));
    renderer.device().fails_completion.set(false);

    producer.clear_events();
    producer.queue(Buffer::new(2, SURFACE_SIZE, skia::Color::BLUE));
    assert!(matches!(renderer.latch(stream)?, Latch::Frame(_)));
    producer.queue(Buffer::new(3, SURFACE_SIZE, skia::Color::BLUE));
    assert!(matches!(renderer.latch(stream)?, Latch::Frame(_)));
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
fn a_device_with_a_ycbcr_conversion_samples_a_half_float_frame() -> Result<(), TestError> {
    let simulation = Simulation::start();
    simulation.gpu.sampler_ycbcr_conversion.set(true);
    let mut renderer = Display::on(simulation)?;
    let stream = renderer.create_stream(surface_sizes()?, StreamFormat::Private)?;
    let mut half_float = Buffer::new(1, SURFACE_SIZE, skia::Color::RED);
    half_float.format = HardwareBufferFormat::R16G16B16A16_FLOAT;
    renderer.producer(stream)?.queue(half_float);
    let image = renderer.latched(stream)?;
    assert_eq!(renderer.render(image)?, FrameOutcome::Shown);
    assert_eq!(renderer.pixel(MIDDLE, MIDDLE)?, RED);
    Ok(())
}

#[test]
fn a_render_on_a_lost_device_reports_every_step_that_failed() -> Result<(), TestError> {
    let mut renderer = Display::new()?;
    let stream = renderer.create_stream(surface_sizes()?, StreamFormat::Rgba8888)?;
    let producer = renderer.producer(stream)?;
    producer.queue(Buffer::new(1, SURFACE_SIZE, skia::Color::RED));
    let image = renderer.latched(stream)?;
    assert_eq!(renderer.render(image)?, FrameOutcome::Shown);
    renderer.device().lost.set(true);
    let (frame, streams) = renderer.draw(&scene(whole_surface(image))?);
    assert!(matches!(frame, Err(TestError::Renderer(_))));
    assert!(matches!(
        streams,
        Err(Error::Several(failures)) if matches!(
            failures.as_slice(),
            [
                Error::HandBack(_),
                Error::SubmitCompletion(_),
                Error::RetireImports(_)
            ]
        )
    ));
    Ok(())
}

#[test]
fn a_default_size_beyond_the_maximum_is_refused_where_the_sizes_are_made() {
    assert_eq!(
        BufferSizes::new(TOO_WIDE, SURFACE_SIZE),
        Err(BufferSizesError::DefaultBeyondMax {
            default: TOO_WIDE,
            max: SURFACE_SIZE
        })
    );
}

#[test]
fn a_size_beyond_the_signed_lengths_of_the_ndk_is_refused_where_the_sizes_are_made() {
    let beyond = PixelSize::new(u32::MAX, SURFACE_SIZE.height);
    assert_eq!(
        BufferSizes::new(SURFACE_SIZE, beyond),
        Err(BufferSizesError::BeyondNdk {
            default: SURFACE_SIZE,
            max: beyond
        })
    );
}

#[test]
fn a_reader_that_hands_out_no_window_fails_where_the_stream_is_made() -> Result<(), TestError> {
    let simulation = Simulation::start();
    simulation.media.fails_window.set(true);
    let failed = SurfaceStream::new(surface_sizes()?, StreamFormat::Rgba8888);
    assert!(matches!(failed, Err(Error::ReaderWindow(_))));
    Ok(())
}

#[test]
fn when_every_image_is_held_for_the_gpu_the_newest_frame_waits() -> Result<(), TestError> {
    let mut renderer = Display::new()?;
    let stream = renderer.create_stream(surface_sizes()?, StreamFormat::Rgba8888)?;
    let producer = renderer.producer(stream)?;
    renderer.device().busy.set(true);
    for id in 1..=3 {
        producer.queue(Buffer::new(id, SURFACE_SIZE, skia::Color::RED));
        let image = renderer.latched(stream)?;
        renderer.device().fails_release.set(true);
        assert!(matches!(
            renderer.render(image),
            Err(TestError::Adapter(Error::HandBack(_)))
        ));
    }
    producer.queue(Buffer::new(4, SURFACE_SIZE, skia::Color::RED));
    assert!(matches!(renderer.latch(stream)?, Latch::Frame(_)));
    producer.queue(Buffer::new(5, SURFACE_SIZE, skia::Color::BLUE));
    assert_eq!(renderer.latch(stream)?, Latch::Unchanged);
    renderer.device().busy.set(false);
    let newest = renderer.latched(stream)?;
    assert_eq!(renderer.stream(stream)?.drawn_size(), Some(newest.size));
    Ok(())
}

#[test]
fn a_freed_buffer_whose_id_cannot_be_read_fails_one_latch() -> Result<(), TestError> {
    let mut renderer = Display::new()?;
    let stream = renderer.create_stream(surface_sizes()?, StreamFormat::Rgba8888)?;
    let producer = renderer.producer(stream)?;
    let mut unidentifiable = Buffer::new(1, SURFACE_SIZE, skia::Color::RED);
    unidentifiable.unidentifiable = true;
    producer.remove(unidentifiable);
    producer.queue(Buffer::new(2, SURFACE_SIZE, skia::Color::RED));
    assert!(matches!(
        renderer.latch(stream),
        Err(Error::RemovedBufferId(_))
    ));
    assert!(matches!(renderer.latch(stream)?, Latch::Frame(_)));
    Ok(())
}
