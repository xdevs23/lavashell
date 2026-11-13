// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use lavashell::core::frame::FrameTime;

fn presented(surface: u64, milliseconds: u64) -> RecordedFrame {
    RecordedFrame {
        surface: SurfaceId(surface),
        time: FrameTime {
            presentation: Duration::from_millis(milliseconds),
            ..FrameTime::default()
        },
        png: Vec::new(),
    }
}

fn labels(labeled: Vec<(&'static str, RecordedFrame)>) -> Vec<(&'static str, u64, u128)> {
    (labeled.into_iter())
        .map(|(label, frame)| (label, frame.surface.0, frame.time.presentation.as_millis()))
        .collect()
}

#[test]
fn each_frame_shows_the_newest_step_started_at_or_before_its_presentation() {
    let started = [
        (Duration::from_millis(10), "a"),
        (Duration::from_millis(20), "c"),
    ];
    let frames = vec![
        presented(1, 5),
        presented(1, 12),
        presented(1, 20),
        presented(1, 30),
    ];
    assert_eq!(
        labels(label(frames, &started)),
        [("a", 1, 12), ("c", 1, 20), ("c", 1, 30)]
    );
}

#[test]
fn a_step_without_a_presented_frame_is_shown_by_the_newest_frame_of_each_surface() {
    let started = [
        (Duration::from_millis(10), "a"),
        (Duration::from_millis(20), "b"),
        (Duration::from_millis(20), "c"),
    ];
    let frames = vec![
        presented(1, 12),
        presented(2, 14),
        presented(1, 15),
        presented(1, 25),
    ];
    assert_eq!(
        labels(label(frames, &started)),
        [
            ("a", 1, 12),
            ("a", 2, 14),
            ("a", 1, 15),
            ("b", 1, 15),
            ("b", 2, 14),
            ("c", 1, 25)
        ]
    );
}
