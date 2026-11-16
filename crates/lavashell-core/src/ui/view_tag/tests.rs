// SPDX-License-Identifier: GPL-3.0-only
use super::*;

#[derive(Debug, PartialEq, Eq)]
enum Control {
    Play,
    Pause,
}

#[test]
fn a_tag_matches_only_an_equal_value_of_its_own_type() {
    let play = NodeTag::new(Control::Play);
    assert!(play.is(&Control::Play));
    assert!(!play.is(&Control::Pause));
    assert!(!play.is(&"Play"));
}

#[test]
fn two_tags_are_equal_when_their_values_are() {
    assert_eq!(NodeTag::new(Control::Play), NodeTag::new(Control::Play));
    assert_ne!(NodeTag::new(Control::Play), NodeTag::new(Control::Pause));
    assert_ne!(NodeTag::new(0_u32), NodeTag::new(0_u64));
}

#[test]
fn a_tag_returns_its_value_only_as_its_own_type() {
    let pause = NodeTag::new(Control::Pause);
    assert_eq!(pause.get::<Control>(), Some(&Control::Pause));
    assert_eq!(pause.get::<&str>(), None);
}

#[test]
fn a_tag_shows_the_value_it_was_made_from() {
    assert_eq!(format!("{:?}", NodeTag::new(Control::Pause)), "Pause");
}
