// SPDX-License-Identifier: GPL-3.0-only
use super::probe::{WINDOW, Wrapping};
use super::*;

/// Measures like [`Wrapping`], but cuts the height to `max_lines` lines of 20 dp when the text
/// style sets `max_lines`.
struct Capped;

impl TextMeasurer for Capped {
    fn measure(&mut self, text: &str, style: &TextStyle<Dp>, max_width: Dp) -> Size {
        let wrapped = Wrapping.measure(text, style, max_width);
        let allowed = (style.max_lines).map(|lines| repeated(20.dp(), lines));
        Size::new(
            wrapped.width,
            allowed.map_or(wrapped.height, |height| wrapped.height.min(height)),
        )
    }
}

#[test]
fn a_single_line_input_measures_one_line_where_a_text_area_wraps() -> TestResult {
    const VALUE: &str = "a value wider than the field";
    let height = |compose: fn(&mut Ui)| -> Result<Dp, TestError> {
        let mut view = View::new(crate::platform::fixture::facts(), compose);
        view.provide(&VIEWPORT, WINDOW);
        let narrow = Proposal {
            width: Some(50.dp()),
            height: None,
        };
        Ok(view.size_that_fits(narrow, &mut Capped)?.height)
    };
    let input = |ui: &mut Ui| {
        ui.text_input(VALUE, TextFieldOptions::default(), Modifier::new(), |_| {});
    };
    let area = |ui: &mut Ui| {
        ui.text_area(VALUE, TextFieldOptions::default(), Modifier::new(), |_| {});
    };
    assert_eq!(height(input)?, 20.dp());
    assert_eq!(height(area)?, 120.dp());
    Ok(())
}

#[test]
fn a_text_style_leaves_a_single_line_input_on_one_line() -> TestResult {
    let mut view = View::new(crate::platform::fixture::facts(), |ui| {
        let style = Modifier::new().text_style(TextStyle::new("sans-serif", 14.sp(), Color::WHITE));
        ui.text_input(
            "a value wider than the field",
            TextFieldOptions::default(),
            style,
            |_| {},
        );
    });
    view.provide(&VIEWPORT, WINDOW);
    let narrow = Proposal {
        width: Some(50.dp()),
        height: None,
    };
    assert_eq!(view.size_that_fits(narrow, &mut Capped)?.height, 20.dp());
    Ok(())
}
