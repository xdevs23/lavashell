// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::ui::ContentSizes;

/// Measures a text as one line 20 dp high and 10 dp wide for each character when that width fits
/// into the given width. Otherwise the text wraps to two lines 40 dp high at the given width, but
/// at least 30 dp wide, the width assumed for the longest word.
struct TwoLines;

impl TextMeasurer for TwoLines {
    fn measure(&mut self, text: &str, _: &TextStyle<Dp>, max_width: Dp) -> Size {
        let natural = repeated(10.dp(), text.chars().count());
        if natural <= max_width {
            Size::new(natural, 20.dp())
        } else {
            Size::new(max_width.max(30.dp()), 40.dp())
        }
    }
}

#[test]
fn a_node_reports_its_size_without_a_bound_and_at_its_smallest_and_keeps_its_layout() -> TestResult
{
    let reported = State::new(None);
    let sink = reported.clone();
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let sink = sink.clone();
        let report = Modifier::new().on_content_sizes(move |sizes| sink.set(Some(sizes)));
        ui.layout(Axis::Vertical, report, |ui| {
            ui.text("one two three", Modifier::new().tag("text"));
        });
    });
    advance_with(
        &mut scene,
        &mut view,
        Viewport::new(WINDOW_SIZE, Density::ONE),
        &mut TwoLines,
    )?;
    let sizes = ContentSizes {
        preferred: Size::new(130.dp(), 20.dp()),
        minimum: Size::new(30.dp(), 40.dp()),
    };
    assert_eq!(reported.get(), Some(sizes));
    assert_eq!(
        view.bounds_of("text").map(Rect::size),
        Some(Size::new(130.dp(), 20.dp()))
    );
    Ok(())
}
