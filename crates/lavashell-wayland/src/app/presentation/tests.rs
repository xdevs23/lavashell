// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use wp_presentation_feedback::Kind;

fn flags_of(kind: Kind) -> [bool; 4] {
    let flags = presentation_flags(kind);
    [
        flags.contains(PresentationFlag::Vsync),
        flags.contains(PresentationFlag::HardwareClock),
        flags.contains(PresentationFlag::HardwareCompletion),
        flags.contains(PresentationFlag::ZeroCopy),
    ]
}

#[test]
fn each_feedback_kind_sets_only_its_own_flag() {
    assert_eq!(flags_of(Kind::Vsync), [true, false, false, false]);
    assert_eq!(flags_of(Kind::HwClock), [false, true, false, false]);
    assert_eq!(flags_of(Kind::HwCompletion), [false, false, true, false]);
    assert_eq!(flags_of(Kind::ZeroCopy), [false, false, false, true]);
    assert_eq!(flags_of(Kind::empty()), [false; 4]);
    assert_eq!(flags_of(Kind::all()), [true; 4]);
}
