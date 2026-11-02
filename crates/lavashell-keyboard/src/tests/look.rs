// SPDX-License-Identifier: GPL-3.0-only
use super::*;

fn styled(style: KeyboardStyle) -> Result<Host, TestError> {
    let mut host = Host::styled(style)?;
    host.start(TextHints::default(), "")?;
    Ok(host)
}

/// The color of the label of the key `q`.
fn ink(scene: &Scene) -> Option<Color> {
    (leaves(scene).into_iter())
        .flat_map(|(_, commands)| commands)
        .find_map(|command| match command {
            DrawCommand::Text { text, style, .. } if text == "q" => Some(style.color),
            _ => None,
        })
}

#[test]
fn reduced_transparency_turns_the_glass_opaque() -> Result<(), TestError> {
    let mut host = styled(KeyboardStyle::night())?;
    assert!(
        !backdrops(&host.scene).is_empty(),
        "the glass blurs what is behind it"
    );
    host.view.provide(&REDUCED_TRANSPARENCY, true);
    host.frame()?;
    host.settle()?;
    assert!(backdrops(&host.scene).is_empty());
    let rows = painted(&host.scene, &|command| match command {
        DrawCommand::Fill {
            shape: Shape::Union(union),
            brush: Brush::Solid(color),
        } if union.members().len() == 10 && color.alpha() >= 1.0 => Some(union.bounds()),
        _ => None,
    });
    assert_eq!(rows.len(), 1, "the keys of the top row are opaque");
    Ok(())
}

#[test]
fn without_a_style_the_keyboard_follows_the_color_scheme_of_the_platform() -> Result<(), TestError>
{
    let night = ink(&styled(KeyboardStyle::night())?.scene);
    let day = ink(&styled(KeyboardStyle::day())?.scene);
    assert!(
        night.is_some() && night != day,
        "the styles paint different ink"
    );
    let mut host = Host::on(settings(), lavashell_test_fixtures::facts())?;
    host.start(TextHints::default(), "")?;
    assert_eq!(ink(&host.scene), night);
    host.platform(|facts| facts.color_scheme = ColorScheme::Light);
    host.frame()?;
    assert_eq!(ink(&host.scene), day);
    Ok(())
}

#[test]
fn styles_made_from_the_same_palette_are_equal() {
    assert_eq!(KeyboardStyle::night(), KeyboardStyle::night());
    assert_ne!(KeyboardStyle::night(), KeyboardStyle::day());
    let accent = |style: KeyboardStyle| style.accent(Color::WHITE, Color::TRANSPARENT);
    assert_eq!(accent(KeyboardStyle::day()), accent(KeyboardStyle::day()));
    assert_ne!(accent(KeyboardStyle::day()), KeyboardStyle::day());
}

#[test]
fn a_style_makes_its_looks_again_only_for_another_height_of_the_keys() {
    let style = KeyboardStyle::night();
    let first = style.looks(76.dp());
    assert!(Rc::ptr_eq(&first, &style.clone().looks(76.dp())));
    assert!(!Rc::ptr_eq(&first, &style.looks(90.dp())));
}
