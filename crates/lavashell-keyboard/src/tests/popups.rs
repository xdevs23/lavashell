// SPDX-License-Identifier: GPL-3.0-only
use super::dock::docked;
use super::*;
use crate::popup::{ENTRY_GAP, ENTRY_WIDTH, STRIP_PADDING};
use lavashell_core::numbers::count_f64;

/// The further letters under the a of `nine_under_a`.
const NINE: &str = "àâæäáãåāą";

/// A layout with nine further letters under the a at the start of its top row.
fn nine_under_a() -> Result<&'static Layout, TestError> {
    let layout = Layout::new(LayoutSpec {
        id: "xx-nine",
        language: language!("xx"),
        name: "Nine",
        arrangement: "AZERTY",
        rows: ["azertyuiop", "qsdfghjklm", "wxcvbn'"],
        further_letters: &[('a', NINE)],
        currency: "€",
    });
    Ok(Box::leak(Box::new(layout?)))
}

/// The width of a strip of `letters` letters in one row, each `letter` wide.
fn one_row(letters: f64, letter: Dp) -> Dp {
    STRIP_PADDING * 2.0 + letter * letters + ENTRY_GAP * (letters - 1.0)
}

/// A host offering `layouts`, with the panel open at `placement`.
fn host_at(placement: Placement, layouts: Vec<&'static Layout>) -> Result<Host, TestError> {
    let (settings, facts) = offering(layouts);
    let mut host = Host::on(settings, facts)?;
    host.placement.set(placement);
    host.start(TextHints::default(), "")?;
    Ok(host)
}

/// Presses the key named `name` until it opens a popup.
fn long_press(host: &mut Host, name: &str) -> Result<(), TestError> {
    let at = key(&host.view, name)?;
    host.view.input(InputEvent::TouchDown(finger(1, at)));
    host.at(host.time + LONG_PRESS)
}

fn bounds(host: &Host, role: &Role, name: &str) -> Result<Rect, TestError> {
    Ok(node(&host.view, role, name)?.bounds)
}

/// The bounds of the letters of `NINE` in the open strip, in the order of the letters.
fn letters(host: &Host) -> Result<Vec<Rect>, TestError> {
    (NINE.chars())
        .map(|letter| bounds(host, &Role::MenuItem, &letter.to_string()))
        .collect()
}

#[test]
fn a_strip_that_fits_the_panel_shows_its_letters_in_one_row() -> Result<(), TestError> {
    let mut host = host_at(Placement::FullWidth, vec![nine_under_a()?])?;
    long_press(&mut host, "a")?;
    let panel = bounds(&host, &Role::Group, "Keyboard")?;
    assert!(one_row(count_f64(NINE.chars().count())?, ENTRY_WIDTH) <= panel.width);
    let strip = bounds(&host, &Role::Menu, "More letters")?;
    assert!(strip.width <= panel.width, "{strip:?}");
    let letters = letters(&host)?;
    let (first, last) = (letters[0], letters[letters.len() - 1]);
    for letter in &letters {
        assert!(near_length(letter.y, first.y), "one row: {letters:?}");
        assert!(near_length(letter.width, ENTRY_WIDTH), "{letters:?}");
    }
    assert!(near_length(first.x - strip.x, STRIP_PADDING), "{strip:?}");
    let after_last = strip.x + strip.width - last.x - last.width;
    assert!(near_length(after_last, STRIP_PADDING), "{strip:?}");
    Ok(())
}

#[test]
fn a_strip_wider_than_a_dock_wraps_into_two_rows_from_the_top_of_the_screen()
-> Result<(), TestError> {
    let mut host = host_at(docked(Side::Right, 640.dp())?, vec![nine_under_a()?])?;
    long_press(&mut host, "a")?;
    let panel = bounds(&host, &Role::Group, "Keyboard")?;
    assert!(one_row(count_f64(NINE.chars().count())?, ENTRY_WIDTH) > panel.width);
    let strip = bounds(&host, &Role::Menu, "More letters")?;
    assert!(
        near_length(strip.y, Dp::ZERO) && strip.width <= panel.width,
        "{strip:?}"
    );
    assert!(strip.x + strip.width <= SCREEN.width);
    let letters = letters(&host)?;
    for letter in &letters {
        assert!(near_length(letter.width, ENTRY_WIDTH), "{letters:?}");
    }
    let (lower, upper) = letters.split_at(NINE.chars().count().div_ceil(2));
    for (lower, upper) in lower.iter().zip(upper) {
        assert!(upper.y < lower.y, "the first letters lie in the lower row");
        assert!(near_length(upper.x, lower.x), "{letters:?}");
    }
    let (first, last) = (lower[0], lower[lower.len() - 1]);
    assert!(near_length(first.x - strip.x, STRIP_PADDING), "{strip:?}");
    let after_last = strip.x + strip.width - last.x - last.width;
    assert!(near_length(after_last, STRIP_PADDING), "{strip:?}");
    Ok(())
}

#[test]
fn letters_narrow_where_two_rows_are_wider_than_the_dock() -> Result<(), TestError> {
    let mut host = host_at(docked(Side::Left, 400.dp())?, vec![nine_under_a()?])?;
    long_press(&mut host, "a")?;
    let panel = bounds(&host, &Role::Group, "Keyboard")?;
    let columns = count_f64(NINE.chars().count().div_ceil(2))?;
    assert!(one_row(columns, ENTRY_WIDTH) > panel.width);
    let strip = bounds(&host, &Role::Menu, "More letters")?;
    assert!(near_length(strip.width, panel.width), "{strip:?}");
    let letter = bounds(&host, &Role::MenuItem, "à")?;
    assert!(
        near_length(one_row(columns, letter.width), panel.width),
        "{letter:?}"
    );
    Ok(())
}

#[test]
fn a_finger_slid_to_the_upper_row_types_the_letter_there() -> Result<(), TestError> {
    let mut host = host_at(docked(Side::Right, 640.dp())?, vec![nine_under_a()?])?;
    long_press(&mut host, "a")?;
    let upper = bounds(&host, &Role::MenuItem, "ã")?.center();
    host.view.input(InputEvent::TouchMove(finger(1, upper)));
    host.view.input(InputEvent::TouchUp(finger(1, upper)));
    assert_eq!(host.sent(), [Host::commit("ã")]);
    Ok(())
}

#[test]
fn the_list_of_layouts_starts_on_the_screen_above_a_dock() -> Result<(), TestError> {
    let mut host = host_at(docked(Side::Left, 640.dp())?, LAYOUTS.to_vec())?;
    long_press(&mut host, "Switch layout")?;
    let list = bounds(&host, &Role::Menu, "Keyboard layouts")?;
    assert!(near_length(list.y, Dp::ZERO), "{list:?}");
    for layout in LAYOUTS {
        let entry = bounds(&host, &Role::MenuItem, layout.name())?;
        assert!(
            entry.y + entry.height <= list.y + list.height,
            "{entry:?} inside {list:?}"
        );
    }
    Ok(())
}

#[test]
fn a_finger_sliding_in_a_popup_beside_another_finger_types_its_letter_and_passes_no_transform()
-> Result<(), TestError> {
    let mut host = host_at(Placement::FullWidth, vec![nine_under_a()?])?;
    host.pass_transforms();
    let panel = node(&host.view, &Role::Group, "Keyboard")?.bounds;
    let resting = Point::new(panel.x + 2.dp(), panel.y + panel.height / 2.0);
    host.view.input(InputEvent::TouchDown(finger(2, resting)));
    long_press(&mut host, "a")?;
    let third = bounds(&host, &Role::MenuItem, "æ")?.center();
    host.view.input(InputEvent::TouchMove(finger(1, third)));
    host.view.input(InputEvent::TouchUp(finger(1, third)));
    assert!(host.transforms.borrow().is_empty());
    assert_eq!(host.sent(), [Host::commit("æ")]);
    Ok(())
}
