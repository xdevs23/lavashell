// SPDX-License-Identifier: GPL-3.0-only
use super::probe::{WINDOW, Wrapping};
use super::*;

/// Measures like [`Wrapping`]. Its `min_width` returns the width of the longest word at 10 dp for
/// each character, the smallest width at which the text draws correctly.
struct Words;

impl TextMeasurer for Words {
    fn measure(&mut self, text: &str, style: &TextStyle<Dp>, max_width: Dp) -> Size {
        Wrapping.measure(text, style, max_width)
    }

    fn min_width(&mut self, text: &str, _: &TextStyle<Dp>) -> Dp {
        let longest = (text.split(' ').map(|word| word.chars().count()).max()).unwrap_or_default();
        repeated(10.dp(), longest)
    }
}

fn probe(compose: impl FnMut(&mut Ui) + 'static, proposal: Proposal) -> Result<Size, TestError> {
    let mut view = View::new(crate::platform::fixture::facts(), compose);
    (view.show_frame(&mut SceneTree::default(), WINDOW, &mut Words)).without_failures()?;
    Ok(view.size_that_fits(proposal, &mut Words)?)
}

#[test]
fn text_is_as_wide_as_its_smallest_width() -> TestResult {
    let title = |ui: &mut Ui| {
        ui.text("Display brightness", Modifier::new());
    };
    assert_eq!(
        probe(
            title,
            Proposal {
                width: Some(0.dp()),
                height: Some(0.dp()),
            },
        )?,
        Size::new(100.dp(), 40.dp())
    );
    Ok(())
}

#[test]
fn a_row_adds_the_smallest_widths_of_its_children() -> TestResult {
    let row = |ui: &mut Ui| {
        ui.layout(Axis::Horizontal, Modifier::new().gap(5.dp()), |ui| {
            ui.text("Display brightness", Modifier::new());
            ui.spacer(150.dp(), 30.dp(), Modifier::new());
        });
    };
    assert_eq!(
        probe(
            row,
            Proposal {
                width: Some(0.dp()),
                height: Some(0.dp()),
            },
        )?,
        Size::new(255.dp(), 40.dp())
    );
    Ok(())
}

#[test]
fn a_column_is_as_wide_as_its_widest_child_at_its_smallest_width() -> TestResult {
    let column = |ui: &mut Ui| {
        ui.layout(Axis::Vertical, Modifier::new(), |ui| {
            ui.text("Display brightness", Modifier::new());
            ui.text("Volume", Modifier::new());
            ui.spacer(80.dp(), 10.dp(), Modifier::new());
        });
    };
    assert_eq!(
        probe(
            column,
            Proposal {
                width: Some(0.dp()),
                height: Some(0.dp()),
            },
        )?,
        Size::new(100.dp(), 70.dp())
    );
    Ok(())
}

#[test]
fn padding_adds_to_the_smallest_size_of_the_content() -> TestResult {
    let padded = |ui: &mut Ui| {
        ui.text("Display brightness", Modifier::new().padding(10.dp()));
    };
    assert_eq!(
        probe(
            padded,
            Proposal {
                width: Some(0.dp()),
                height: Some(0.dp()),
            },
        )?,
        Size::new(120.dp(), 60.dp())
    );
    Ok(())
}

#[test]
fn a_fixed_child_keeps_its_size() -> TestResult {
    let fixed = |ui: &mut Ui| {
        ui.layout(Axis::Vertical, Modifier::new(), |ui| {
            ui.text("Display brightness", Modifier::new().width(300.dp()));
            ui.spacer(40.dp(), 30.dp(), Modifier::new());
        });
    };
    assert_eq!(
        probe(
            fixed,
            Proposal {
                width: Some(0.dp()),
                height: Some(0.dp()),
            },
        )?,
        Size::new(300.dp(), 50.dp())
    );
    Ok(())
}

#[test]
fn a_filling_child_has_its_smallest_size() -> TestResult {
    let filling = |ui: &mut Ui| {
        ui.layout(Axis::Vertical, Modifier::new(), |ui| {
            ui.text(
                "Display brightness",
                Modifier::new().fill_max_width().fill_max_height(),
            );
            ui.spacer(50.dp(), 20.dp(), Modifier::new());
        });
    };
    assert_eq!(
        probe(
            filling,
            Proposal {
                width: Some(0.dp()),
                height: Some(0.dp()),
            },
        )?,
        Size::new(100.dp(), 60.dp())
    );
    Ok(())
}

fn weighted_row(ui: &mut Ui) {
    ui.layout(Axis::Horizontal, Modifier::new(), |ui| {
        ui.text("Brightness", Modifier::new());
        ui.spacer(40.dp(), 10.dp(), Modifier::new().weight(1.0).tag("slider"));
        ui.text("100 %", Modifier::new().weight_up_to(3.0).tag("value"));
    });
}

#[test]
fn weighted_children_take_the_length_at_which_each_has_its_smallest_size() -> TestResult {
    assert_eq!(
        probe(
            weighted_row,
            Proposal {
                width: Some(0.dp()),
                height: Some(0.dp()),
            },
        )?,
        Size::new(260.dp(), 20.dp())
    );
    let mut view = View::new(crate::platform::fixture::facts(), weighted_row);
    let viewport = Viewport::new(Size::new(260.dp(), 20.dp()), Density::ONE);
    (view.show_frame(&mut SceneTree::default(), viewport, &mut Words)).without_failures()?;
    assert_eq!(
        view.tagged_bounds("slider")?,
        Rect::new(100.dp(), 0.dp(), 40.dp(), 10.dp())
    );
    assert_eq!(
        view.tagged_bounds("value")?,
        Rect::new(140.dp(), 0.dp(), 50.dp(), 20.dp())
    );
    Ok(())
}

#[test]
fn nested_containers_have_the_smallest_size_of_their_content() -> TestResult {
    let card = |ui: &mut Ui| {
        ui.layout(Axis::Vertical, Modifier::new().padding(10.dp()), |ui| {
            ui.layout(Axis::Horizontal, Modifier::new().fill_max_width(), |ui| {
                ui.text("Display brightness", Modifier::new());
                ui.spacer(Dp::ZERO, Dp::ZERO, Modifier::new().weight(1.0));
                ui.spacer(20.dp(), 20.dp(), Modifier::new());
            });
            ui.spacer(Dp::ZERO, Dp::ZERO, Modifier::new().weight(1.0));
            ui.text("Subtitle", Modifier::new());
        });
    };
    assert_eq!(
        probe(
            card,
            Proposal {
                width: Some(0.dp()),
                height: Some(0.dp()),
            },
        )?,
        Size::new(140.dp(), 80.dp())
    );
    Ok(())
}

fn up_to_shares(ui: &mut Ui) {
    ui.layout(Axis::Horizontal, Modifier::new(), |ui| {
        ui.text("abcd e", Modifier::new().weight_up_to(1.0).tag("text"));
        ui.spacer(Dp::ZERO, Dp::ZERO, Modifier::new().weight_up_to(1.0));
    });
}

#[test]
fn a_stack_is_as_long_as_its_weighted_children_need_to_get_their_size_from_their_share()
-> TestResult {
    let wide = Proposal {
        width: Some(1000.dp()),
        height: None,
    };
    let mut view = View::new(crate::platform::fixture::facts(), up_to_shares);
    assert_eq!(
        view.size_that_fits(wide, &mut Words),
        Ok(Size::new(120.dp(), 20.dp()))
    );
    let viewport = Viewport::new(Size::new(120.dp(), 20.dp()), Density::ONE);
    (view.show_frame(&mut SceneTree::default(), viewport, &mut Words)).without_failures()?;
    assert_eq!(
        view.tagged_bounds("text")?,
        Rect::new(0.dp(), 0.dp(), 60.dp(), 20.dp())
    );
    Ok(())
}

#[test]
fn a_text_area_wraps_at_its_smallest_width() -> TestResult {
    let area = |ui: &mut Ui| {
        ui.text_area(
            "one two three four five six",
            TextFieldOptions::default(),
            Modifier::new(),
            |_| {},
        );
    };
    assert_eq!(
        probe(
            area,
            Proposal {
                width: Some(0.dp()),
                height: Some(0.dp()),
            },
        )?,
        Size::new(50.dp(), 120.dp())
    );
    Ok(())
}

#[test]
fn text_on_one_line_is_as_wide_as_an_ellipsis() {
    let style = TextStyle {
        max_lines: Some(1),
        ..TextStyle::new("sans-serif", 14.sp(), Color::WHITE)
    }
    .resolve_unscaled();
    assert_eq!(Wrapping.min_width("Display brightness", &style), 10.dp());
    assert_eq!(Wrapping.min_width("", &style), Dp::ZERO);
}

/// Measures like [`Words`] and counts the calls of `measure` in its field.
struct Counting(usize);

impl TextMeasurer for Counting {
    fn measure(&mut self, text: &str, style: &TextStyle<Dp>, max_width: Dp) -> Size {
        self.0 += 1;
        Words.measure(text, style, max_width)
    }

    fn min_width(&mut self, text: &str, style: &TextStyle<Dp>) -> Dp {
        Words.min_width(text, style)
    }
}

fn nested(ui: &mut Ui, depth: usize) {
    let axis = if depth.is_multiple_of(2) {
        Axis::Horizontal
    } else {
        Axis::Vertical
    };
    ui.layout(axis, Modifier::new().weight(1.0), |ui| {
        ui.text("Display brightness", Modifier::new());
        if depth > 0 {
            nested(ui, depth - 1);
        }
    });
}

fn measurements(depth: usize) -> Result<usize, TestError> {
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        nested(ui, depth);
    });
    (view.show_frame(&mut SceneTree::default(), WINDOW, &mut Words)).without_failures()?;
    let mut counting = Counting(0);
    let _probed_for_the_count = view.size_that_fits(
        Proposal {
            width: Some(0.dp()),
            height: Some(0.dp()),
        },
        &mut counting,
    );
    Ok(counting.0)
}

#[test]
fn a_probe_measures_the_texts_of_nested_weighted_stacks_at_most_their_count_squared_times()
-> TestResult {
    for depth in 0..=12 {
        let texts = depth + 1;
        assert!(measurements(depth)? <= texts * texts, "{depth}");
    }
    Ok(())
}

#[test]
fn a_probe_wraps_text_at_a_fixed_width_below_the_smallest_width_of_the_text() -> TestResult {
    assert_eq!(
        probe(
            narrow,
            Proposal {
                width: Some(0.dp()),
                height: Some(0.dp()),
            },
        )?,
        Size::new(50.dp(), 80.dp())
    );
    Ok(())
}

fn narrow(ui: &mut Ui) {
    ui.layout(Axis::Vertical, Modifier::new(), |ui| {
        ui.text(
            "Display brightness",
            Modifier::new().tag("title").width(50.dp()),
        );
    });
}

#[test]
fn a_frame_wraps_text_at_its_width_below_the_smallest_width_of_the_text() -> TestResult {
    let mut view = View::new(crate::platform::fixture::facts(), narrow);
    (view.show_frame(&mut SceneTree::default(), WINDOW, &mut Words)).without_failures()?;
    assert_eq!(
        view.tagged_bounds("title")?,
        Rect::new(0.dp(), 0.dp(), 50.dp(), 80.dp())
    );
    Ok(())
}

#[test]
fn a_weighted_row_inside_a_maximum_width_keeps_the_frame_result() -> TestResult {
    let bounded = |ui: &mut Ui| {
        ui.layout(Axis::Vertical, Modifier::new(), |ui| {
            ui.layout(
                Axis::Horizontal,
                Modifier::new().tag("row").max_width(100.dp()),
                |ui| {
                    ui.text("Display brightness", Modifier::new().weight(1.0));
                    ui.text("Display brightness", Modifier::new().weight(1.0));
                },
            );
        });
    };
    let mut view = View::new(crate::platform::fixture::facts(), bounded);
    (view.show_frame(&mut SceneTree::default(), WINDOW, &mut Words)).without_failures()?;
    assert_eq!(
        view.tagged_bounds("row")?.size(),
        Size::new(100.dp(), 80.dp())
    );
    let probed = probe(
        bounded,
        Proposal {
            width: Some(0.dp()),
            height: Some(0.dp()),
        },
    )?;
    assert_eq!(probed, Size::new(100.dp(), 80.dp()));
    Ok(())
}

#[test]
fn a_share_of_a_length_measured_back_from_a_child_is_at_least_its_size() -> TestResult {
    for width in 1..=120_u8 {
        let width = Dp::new(f64::from(width));
        let row = move |ui: &mut Ui| {
            ui.layout(Axis::Horizontal, Modifier::new(), |ui| {
                ui.spacer(width, 10.dp(), Modifier::new().weight(3.0).tag("measured"));
                ui.spacer(0.dp(), 10.dp(), Modifier::new().weight(4.0));
            });
        };
        let probed = probe(
            row,
            Proposal {
                width: Some(0.dp()),
                height: Some(0.dp()),
            },
        )?;
        let mut view = View::new(crate::platform::fixture::facts(), row);
        let viewport = Viewport::new(probed, Density::ONE);
        (view.show_frame(&mut SceneTree::default(), viewport, &mut Words)).without_failures()?;
        let measured = view.tagged_bounds("measured")?.width;
        assert!(measured >= width, "{width:?} became {measured:?}");
    }
    Ok(())
}
