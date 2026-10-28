// SPDX-License-Identifier: GPL-3.0-only
use super::*;

const WINDOW: Viewport = Viewport::new(Size::new(Dp::new(400.0), Dp::new(400.0)), Density::ONE);

fn framed(compose: impl FnMut(&mut Ui) + 'static) -> Result<View, TestError> {
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), compose);
    advance_with(&mut scene, &mut view, WINDOW, &mut Metrics)?;
    Ok(view)
}

fn row(value: Size) -> impl FnMut(&mut Ui) + 'static {
    move |ui: &mut Ui| {
        ui.layout(Axis::Horizontal, Modifier::new(), |ui| {
            ui.spacer(0.dp(), 10.dp(), Modifier::new().weight(3.0).tag("slider"));
            ui.spacer(
                value.width,
                value.height,
                Modifier::new().weight_up_to(1.0).tag("value"),
            );
        });
    }
}

#[test]
fn a_child_weighted_up_to_its_share_keeps_its_content_size_and_leaves_the_rest_empty() -> TestResult
{
    let view = framed(row(Size::new(40.dp(), 10.dp())))?;
    assert_eq!(
        view.tagged_bounds("slider")?,
        Rect::new(0.dp(), 0.dp(), 300.dp(), 10.dp())
    );
    assert_eq!(
        view.tagged_bounds("value")?,
        Rect::new(300.dp(), 0.dp(), 40.dp(), 10.dp())
    );
    Ok(())
}

#[test]
fn a_child_weighted_up_to_its_share_is_no_larger_than_its_share() -> TestResult {
    let view = framed(row(Size::new(150.dp(), 10.dp())))?;
    assert_eq!(
        view.tagged_bounds("value")?,
        Rect::new(300.dp(), 0.dp(), 100.dp(), 10.dp())
    );
    Ok(())
}

#[test]
fn a_column_gives_a_child_weighted_up_to_its_share_at_most_that_share_of_its_height() -> TestResult
{
    let view = framed(|ui| {
        ui.layout(Axis::Vertical, Modifier::new(), |ui| {
            ui.spacer(10.dp(), 0.dp(), Modifier::new().weight(3.0).tag("slider"));
            ui.spacer(
                10.dp(),
                40.dp(),
                Modifier::new().weight_up_to(1.0).tag("value"),
            );
        });
    })?;
    assert_eq!(
        view.tagged_bounds("value")?,
        Rect::new(0.dp(), 300.dp(), 10.dp(), 40.dp())
    );
    Ok(())
}

#[test]
fn a_probe_measures_a_child_weighted_up_to_its_share_at_its_content_size() -> TestResult {
    let mut view = framed(row(Size::new(40.dp(), 10.dp())))?;
    let open = Proposal {
        width: None,
        height: None,
    };
    assert_eq!(
        view.size_that_fits(open, &mut Metrics),
        Ok(Size::new(40.dp(), 10.dp()))
    );
    let bounded = Proposal {
        width: Some(400.dp()),
        height: None,
    };
    assert_eq!(
        view.size_that_fits(bounded, &mut Metrics),
        Ok(Size::new(400.dp(), 10.dp()))
    );
    Ok(())
}

#[test]
fn a_weight_of_zero_leaves_the_child_unweighted() -> TestResult {
    let view = framed(|ui| {
        ui.layout(Axis::Horizontal, Modifier::new(), |ui| {
            ui.spacer(
                20.dp(),
                10.dp(),
                Modifier::new().weight_up_to(0.0).tag("zero"),
            );
        });
    })?;
    assert_eq!(view.tagged_bounds("zero")?.width, 20.dp());
    Ok(())
}

#[test]
fn a_column_with_a_filling_child_and_a_fixed_sibling_fits_its_height() -> TestResult {
    let view = framed(|ui| {
        ui.layout(Axis::Vertical, Modifier::new().tag("column"), |ui| {
            ui.spacer(
                10.dp(),
                10.dp(),
                Modifier::new().fill_max_height().tag("filling"),
            );
            ui.spacer(10.dp(), 100.dp(), Modifier::new().tag("fixed"));
        });
    })?;
    assert_eq!(
        view.tagged_bounds("filling")?,
        Rect::new(0.dp(), 0.dp(), 10.dp(), 300.dp())
    );
    assert_eq!(
        view.tagged_bounds("fixed")?,
        Rect::new(0.dp(), 300.dp(), 10.dp(), 100.dp())
    );
    assert_eq!(view.tagged_bounds("column")?.height, 400.dp());
    Ok(())
}

#[test]
fn two_filling_children_share_the_free_length() -> TestResult {
    let view = framed(|ui| {
        ui.layout(Axis::Vertical, Modifier::new(), |ui| {
            ui.spacer(
                10.dp(),
                10.dp(),
                Modifier::new().fill_max_height().tag("first"),
            );
            ui.spacer(10.dp(), 100.dp(), Modifier::new());
            ui.spacer(
                10.dp(),
                10.dp(),
                Modifier::new().fill_max_height().tag("second"),
            );
        });
    })?;
    assert_eq!(
        view.tagged_bounds("first")?,
        Rect::new(0.dp(), 0.dp(), 10.dp(), 150.dp())
    );
    assert_eq!(
        view.tagged_bounds("second")?,
        Rect::new(0.dp(), 250.dp(), 10.dp(), 150.dp())
    );
    Ok(())
}

#[test]
fn a_filling_child_shares_the_length_with_weighted_siblings_in_proportion_to_the_weight_factors()
-> TestResult {
    let view = framed(|ui| {
        ui.layout(Axis::Horizontal, Modifier::new(), |ui| {
            ui.spacer(
                10.dp(),
                10.dp(),
                Modifier::new().fill_max_width().tag("filling"),
            );
            ui.spacer(0.dp(), 10.dp(), Modifier::new().weight(3.0).tag("weighted"));
        });
    })?;
    assert_eq!(view.tagged_bounds("filling")?.width, 100.dp());
    assert_eq!(view.tagged_bounds("weighted")?.width, 300.dp());
    Ok(())
}

#[test]
fn an_explicit_weight_wins_over_the_fill() -> TestResult {
    let view = framed(|ui| {
        ui.layout(Axis::Horizontal, Modifier::new(), |ui| {
            ui.spacer(
                10.dp(),
                10.dp(),
                Modifier::new().fill_max_width().weight(3.0).tag("filling"),
            );
            ui.spacer(0.dp(), 10.dp(), Modifier::new().weight(1.0).tag("weighted"));
        });
    })?;
    assert_eq!(view.tagged_bounds("filling")?.width, 300.dp());
    assert_eq!(view.tagged_bounds("weighted")?.width, 100.dp());
    Ok(())
}

#[test]
fn a_bound_set_before_the_fill_limits_a_filling_child() -> TestResult {
    let view = framed(|ui| {
        ui.layout(Axis::Vertical, Modifier::new(), |ui| {
            ui.spacer(
                10.dp(),
                10.dp(),
                Modifier::new()
                    .max_height(50.dp())
                    .fill_max_height()
                    .tag("filling"),
            );
            ui.spacer(10.dp(), 100.dp(), Modifier::new().tag("fixed"));
        });
    })?;
    assert_eq!(view.tagged_bounds("filling")?.height, 50.dp());
    assert_eq!(view.tagged_bounds("fixed")?.y, 50.dp());
    Ok(())
}

#[test]
fn a_child_that_fills_across_the_stack_takes_the_whole_cross_length() -> TestResult {
    let view = framed(|ui| {
        ui.layout(Axis::Vertical, Modifier::new(), |ui| {
            ui.spacer(
                10.dp(),
                10.dp(),
                Modifier::new().fill_max_width().tag("filling"),
            );
            ui.spacer(50.dp(), 100.dp(), Modifier::new().tag("fixed"));
        });
    })?;
    assert_eq!(
        view.tagged_bounds("filling")?,
        Rect::new(0.dp(), 0.dp(), 400.dp(), 10.dp())
    );
    assert_eq!(view.tagged_bounds("fixed")?.y, 10.dp());
    Ok(())
}

#[test]
fn a_filling_child_has_the_minimum_length_on_the_scroll_axis_where_a_weighted_child_has_the_assigned_share()
-> TestResult {
    let view = framed(|ui| {
        ui.scroll(Axis::Vertical, Modifier::new(), |ui| {
            ui.spacer(
                10.dp(),
                10.dp(),
                Modifier::new().fill_max_height().tag("filling"),
            );
            ui.spacer(10.dp(), 0.dp(), Modifier::new().weight(1.0).tag("weighted"));
        });
    })?;
    assert_eq!(view.tagged_bounds("filling")?.height, 10.dp());
    assert_eq!(
        view.tagged_bounds("weighted")?,
        Rect::new(0.dp(), 10.dp(), 10.dp(), 390.dp())
    );
    Ok(())
}

#[test]
fn a_keyed_filling_child_takes_the_length_left_by_the_other_children() -> TestResult {
    let view = framed(|ui| {
        ui.layout(Axis::Vertical, Modifier::new(), |ui| {
            ui.key("filling", Modifier::new(), |ui| {
                ui.spacer(
                    10.dp(),
                    10.dp(),
                    Modifier::new().fill_max_height().tag("filling"),
                );
            });
            ui.spacer(10.dp(), 100.dp(), Modifier::new().tag("fixed"));
        });
    })?;
    assert_eq!(view.tagged_bounds("filling")?.height, 300.dp());
    assert_eq!(view.tagged_bounds("fixed")?.y, 300.dp());
    Ok(())
}

#[test]
fn a_shared_filling_child_takes_the_length_left_by_the_other_children() -> TestResult {
    let view = framed(|ui| {
        ui.layout(Axis::Vertical, Modifier::new(), |ui| {
            ui.shared("filling", Modifier::new(), |ui| {
                ui.spacer(
                    10.dp(),
                    10.dp(),
                    Modifier::new().fill_max_height().tag("filling"),
                );
            });
            ui.spacer(10.dp(), 100.dp(), Modifier::new().tag("fixed"));
        });
    })?;
    assert_eq!(view.tagged_bounds("filling")?.height, 300.dp());
    assert_eq!(view.tagged_bounds("fixed")?.y, 300.dp());
    Ok(())
}

#[test]
fn a_filling_child_inside_nested_keys_takes_the_length_left_by_the_other_children() -> TestResult {
    let view = framed(|ui| {
        ui.layout(Axis::Vertical, Modifier::new(), |ui| {
            ui.key("outer", Modifier::new(), |ui| {
                ui.key("inner", Modifier::new(), |ui| {
                    ui.spacer(
                        10.dp(),
                        10.dp(),
                        Modifier::new().fill_max_height().tag("filling"),
                    );
                });
            });
            ui.spacer(10.dp(), 100.dp(), Modifier::new().tag("fixed"));
        });
    })?;
    assert_eq!(view.tagged_bounds("filling")?.height, 300.dp());
    assert_eq!(view.tagged_bounds("fixed")?.y, 300.dp());
    Ok(())
}

#[test]
fn an_overlay_with_a_filling_child_takes_the_length_left_by_the_other_children() -> TestResult {
    let view = framed(|ui| {
        ui.layout(Axis::Vertical, Modifier::new(), |ui| {
            ui.layout(Arrangement::Overlay, Modifier::new().tag("overlay"), |ui| {
                ui.spacer(
                    10.dp(),
                    10.dp(),
                    Modifier::new().fill_max_height().tag("filling"),
                );
                ui.spacer(10.dp(), 50.dp(), Modifier::new());
            });
            ui.spacer(10.dp(), 100.dp(), Modifier::new().tag("fixed"));
        });
    })?;
    assert_eq!(view.tagged_bounds("filling")?.height, 300.dp());
    assert_eq!(view.tagged_bounds("fixed")?.y, 300.dp());
    assert_eq!(view.tagged_bounds("overlay")?.height, 300.dp());
    Ok(())
}

#[test]
fn a_height_set_before_the_fill_replaces_the_fill() -> TestResult {
    let view = framed(|ui| {
        ui.layout(Axis::Vertical, Modifier::new(), |ui| {
            ui.spacer(
                10.dp(),
                10.dp(),
                Modifier::new()
                    .height(50.dp())
                    .fill_max_height()
                    .tag("sized"),
            );
            ui.spacer(10.dp(), 0.dp(), Modifier::new().weight(1.0).tag("weighted"));
        });
    })?;
    assert_eq!(view.tagged_bounds("sized")?.height, 50.dp());
    assert_eq!(
        view.tagged_bounds("weighted")?,
        Rect::new(0.dp(), 50.dp(), 10.dp(), 350.dp())
    );
    Ok(())
}

#[test]
fn a_key_with_a_height_of_its_own_keeps_it_around_a_filling_child() -> TestResult {
    let view = framed(|ui| {
        ui.layout(Axis::Vertical, Modifier::new(), |ui| {
            ui.key(
                "header",
                Modifier::new().height(56.dp()).tag("header"),
                |ui| {
                    ui.spacer(10.dp(), 10.dp(), Modifier::new().fill_max_height());
                },
            );
            ui.spacer(10.dp(), 0.dp(), Modifier::new().weight(1.0).tag("weighted"));
        });
    })?;
    assert_eq!(view.tagged_bounds("header")?.height, 56.dp());
    assert_eq!(
        view.tagged_bounds("weighted")?,
        Rect::new(0.dp(), 56.dp(), 10.dp(), 344.dp())
    );
    Ok(())
}

#[test]
fn a_bound_on_a_key_limits_a_filling_child_in_it() -> TestResult {
    let view = framed(|ui| {
        ui.layout(Axis::Vertical, Modifier::new(), |ui| {
            ui.key(
                "filling",
                Modifier::new().max_height(50.dp()).tag("filling"),
                |ui| {
                    ui.spacer(10.dp(), 10.dp(), Modifier::new().fill_max_height());
                },
            );
            ui.spacer(10.dp(), 100.dp(), Modifier::new().tag("fixed"));
        });
    })?;
    assert_eq!(view.tagged_bounds("filling")?.height, 50.dp());
    assert_eq!(view.tagged_bounds("fixed")?.y, 50.dp());
    Ok(())
}

#[test]
fn a_filling_child_out_of_flow_leaves_its_key_at_the_content_size() -> TestResult {
    let view = framed(|ui| {
        ui.layout(Axis::Vertical, Modifier::new(), |ui| {
            ui.key("card", Modifier::new().tag("card"), |ui| {
                ui.spacer(10.dp(), 50.dp(), Modifier::new());
                ui.spacer(
                    10.dp(),
                    10.dp(),
                    Modifier::new().fill_max_height().out_of_flow(),
                );
            });
            ui.spacer(10.dp(), 100.dp(), Modifier::new().tag("fixed"));
        });
    })?;
    assert_eq!(view.tagged_bounds("card")?.height, 50.dp());
    assert_eq!(view.tagged_bounds("fixed")?.y, 50.dp());
    Ok(())
}

#[test]
fn a_keyed_child_that_fills_across_a_row_takes_no_share_of_its_width() -> TestResult {
    let view = framed(|ui| {
        ui.layout(Axis::Horizontal, Modifier::new(), |ui| {
            ui.key("tall", Modifier::new(), |ui| {
                ui.spacer(10.dp(), 10.dp(), Modifier::new().fill_max_height());
            });
            ui.spacer(0.dp(), 10.dp(), Modifier::new().weight(1.0).tag("weighted"));
        });
    })?;
    assert_eq!(
        view.tagged_bounds("weighted")?,
        Rect::new(10.dp(), 0.dp(), 390.dp(), 10.dp())
    );
    Ok(())
}

#[test]
fn a_keyed_filling_child_has_the_minimum_length_on_the_scroll_axis() -> TestResult {
    let view = framed(|ui| {
        ui.scroll(Axis::Vertical, Modifier::new(), |ui| {
            ui.key("filling", Modifier::new(), |ui| {
                ui.spacer(
                    10.dp(),
                    10.dp(),
                    Modifier::new().fill_max_height().tag("filling"),
                );
            });
            ui.spacer(10.dp(), 100.dp(), Modifier::new().tag("fixed"));
        });
    })?;
    assert_eq!(view.tagged_bounds("filling")?.height, 10.dp());
    assert_eq!(view.tagged_bounds("fixed")?.y, 10.dp());
    Ok(())
}

#[test]
fn a_weight_given_after_an_invalid_one_replaces_it() -> TestResult {
    let view = framed(|ui| {
        ui.layout(Axis::Horizontal, Modifier::new(), |ui| {
            let weighted = Modifier::new().weight(-1.0).weight(1.0).tag("weighted");
            ui.spacer(0.dp(), 10.dp(), weighted);
        });
    })?;
    assert_eq!(view.tagged_bounds("weighted")?.width, WINDOW.size.width);
    Ok(())
}
