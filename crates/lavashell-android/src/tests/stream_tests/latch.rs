// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::platform::{Buffer, GPU, StreamEvent as Event, Writing};

#[test]
fn a_frame_is_imported_when_a_render_draws_it_and_returned_after_it() -> Result<(), TestError> {
    let mut renderer = Display::new()?;
    let stream = renderer.create_stream(SURFACE_SIZES, StreamFormat::Rgba8888)?;
    let producer = renderer.producer(stream)?;
    producer.queue(Buffer::new(1, SURFACE_SIZE, skia::Color::RED));
    let first = renderer.latched(stream)?;
    assert_eq!(
        (first.size, renderer.stream(stream)?.drawn_size()),
        (SURFACE_SIZE, Some(SURFACE_SIZE))
    );
    assert_eq!(producer.events(), []);
    for _ in 0..2 {
        assert_eq!(renderer.render(first)?, FrameOutcome::Shown);
        assert_eq!(renderer.pixel(MIDDLE, MIDDLE)?, RED);
    }
    let read = [
        Event::Imported(1),
        Event::Acquired(1),
        Event::HandedBack(1),
        Event::HandedBack(1),
    ];
    assert_eq!(producer.events(), read);

    producer.clear_events();
    producer.queue(Buffer::new(2, SURFACE_SIZE, skia::Color::BLUE));
    let second = renderer.latched(stream)?;
    assert_eq!(second.generation, first.generation + 1);
    assert_eq!(
        producer.events(),
        [Event::Released {
            id: 1,
            fenced: true
        }]
    );
    assert_eq!(renderer.render(second)?, FrameOutcome::Shown);
    assert_eq!(renderer.pixel(MIDDLE, MIDDLE)?, BLUE);
    let logged = warnings(|| {
        assert_eq!(renderer.render(first)?, FrameOutcome::Shown);
        Ok(())
    })?;
    assert_eq!(renderer.pixel(MIDDLE, MIDDLE)?, [0; 4]);
    assert_eq!(logged.len(), 1);
    Ok(())
}

#[test]
fn a_frame_waits_in_the_stream_until_its_producer_has_written_it() -> Result<(), TestError> {
    let mut renderer = Display::new()?;
    let stream = renderer.create_stream(SURFACE_SIZES, StreamFormat::Rgba8888)?;
    let producer = renderer.producer(stream)?;
    producer.queue(Buffer::new(1, SURFACE_SIZE, skia::Color::RED));
    let shown = renderer.latched(stream)?;
    assert_eq!(renderer.render(shown)?, FrameOutcome::Shown);
    producer.clear_events();

    for id in [2, 3] {
        let mut drawing = Buffer::new(id, SURFACE_SIZE, skia::Color::BLUE);
        drawing.writing = Writing::Pending;
        producer.queue(drawing);
        assert_eq!(renderer.latch(stream)?, Latch::Unchanged);
        assert!(renderer.source(stream)?.waits_for_producer());
        assert!(renderer.source(stream)?.producer_fence().is_some());
        assert_eq!(renderer.render(shown)?, FrameOutcome::Shown);
        assert_eq!(renderer.pixel(MIDDLE, MIDDLE)?, RED);
    }
    producer.finish_writing(3)?;
    let written = renderer.latched(stream)?;
    assert!(!renderer.source(stream)?.waits_for_producer());
    assert!(renderer.source(stream)?.producer_fence().is_none());
    assert_eq!(written.generation, shown.generation + 1);
    assert_eq!(renderer.render(written)?, FrameOutcome::Shown);
    assert_eq!(renderer.pixel(MIDDLE, MIDDLE)?, BLUE);
    assert_eq!(
        producer.events(),
        [
            Event::HandedBack(1),
            Event::Released {
                id: 2,
                fenced: true
            },
            Event::HandedBack(1),
            Event::Released {
                id: 1,
                fenced: true
            },
            Event::Imported(3),
            Event::Acquired(3),
            Event::HandedBack(3),
        ]
    );
    Ok(())
}

#[test]
fn a_frame_imported_by_a_pending_render_is_drawn_by_the_next_render() -> Result<(), TestError> {
    let mut renderer = Display::new()?;
    let stream = renderer.create_stream(SURFACE_SIZES, StreamFormat::Rgba8888)?;
    let producer = renderer.producer(stream)?;
    producer.queue(Buffer::new(1, SURFACE_SIZE, skia::Color::RED));
    let image = renderer.latched(stream)?;
    GPU.with(|gpu| gpu.pending.set(true));
    assert_eq!(renderer.render(image)?, FrameOutcome::Pending);
    let imported = [Event::Imported(1), Event::Acquired(1), Event::HandedBack(1)];
    assert_eq!(producer.events(), imported);

    producer.clear_events();
    assert_eq!(renderer.render(image)?, FrameOutcome::Shown);
    assert_eq!(renderer.pixel(MIDDLE, MIDDLE)?, RED);
    assert_eq!(producer.events(), [Event::HandedBack(1)]);
    Ok(())
}

#[test]
fn an_undrawn_stream_returns_its_frames_with_their_own_fences() -> Result<(), TestError> {
    let mut renderer = Display::new()?;
    let stream = renderer.create_stream(SURFACE_SIZES, StreamFormat::Private)?;
    let producer = renderer.producer(stream)?;
    producer.queue(Buffer::new(1, SURFACE_SIZE, skia::Color::RED));
    assert!(matches!(renderer.latch(stream)?, Latch::Frame(_)));
    producer.queue(Buffer::new(2, SURFACE_SIZE, skia::Color::RED));
    producer.queue(Buffer::new(3, SURFACE_SIZE, skia::Color::RED));
    let newest = renderer.latched(stream)?;
    assert_eq!(renderer.latch(stream)?, Latch::Unchanged);
    let mut unfenced = Buffer::new(4, SURFACE_SIZE, skia::Color::RED);
    unfenced.writing = Writing::Unfenced;
    producer.queue(unfenced);
    assert!(renderer.latched(stream)?.generation > newest.generation);
    producer.queue(Buffer::new(5, SURFACE_SIZE, skia::Color::RED));
    assert!(matches!(renderer.latch(stream)?, Latch::Frame(_)));
    assert_eq!(
        producer.events(),
        [
            Event::Released {
                id: 2,
                fenced: true
            },
            Event::Released {
                id: 1,
                fenced: true
            },
            Event::Released {
                id: 3,
                fenced: true
            },
            Event::Released {
                id: 4,
                fenced: false
            },
        ]
    );
    assert_eq!(producer.broken_fences()?, []);
    Ok(())
}

#[test]
fn a_frame_latched_over_an_older_frame_returns_with_a_fence_of_its_own() -> Result<(), TestError> {
    let mut renderer = Display::new()?;
    let stream = renderer.create_stream(SURFACE_SIZES, StreamFormat::Rgba8888)?;
    let producer = renderer.producer(stream)?;
    producer.queue(Buffer::new(1, SURFACE_SIZE, skia::Color::RED));
    let first = renderer.latched(stream)?;
    assert_eq!(renderer.render(first)?, FrameOutcome::Shown);
    producer.queue(Buffer::new(2, SURFACE_SIZE, skia::Color::GREEN));
    producer.queue(Buffer::new(3, SURFACE_SIZE, skia::Color::BLUE));
    let third = renderer.latched(stream)?;
    assert_eq!(renderer.render(third)?, FrameOutcome::Shown);
    producer.queue(Buffer::new(4, SURFACE_SIZE, skia::Color::RED));
    assert!(matches!(renderer.latch(stream)?, Latch::Frame(_)));
    assert_eq!(producer.broken_fences()?, []);
    Ok(())
}
