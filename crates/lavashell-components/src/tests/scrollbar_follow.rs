// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use lavashell_core::frame::FrameTime;
use lavashell_core::scene::tree::Change;
use lavashell_core::style::{BorderStyle, INTERACTION_STYLE, InteractionStyle};
use std::time::Duration;

const SECTION: Viewport = Viewport::new(Size::new(Dp::new(120.0), Dp::new(120.0)), Density::ONE);

/// A list 120 dp high with twenty rows of 40 dp, and a scrollbar whose thumb follows the list along 120 dp.
fn section(recycler: &Recycler<usize>, style: InteractionStyle) -> Result<View, TestError> {
    let list = recycler.clone();
    let spring = Spring::new(Duration::from_millis(170), 1.0)?;
    Ok(View::new(lavashell_test_platform::facts(), move |ui| {
        let list = list.clone();
        let style = style.clone();
        ui.provide(&INTERACTION_STYLE, style, |ui| {
            section_content(ui, list, spring);
        });
    }))
}

fn section_content(ui: &mut Ui, list: Recycler<usize>, spring: Spring) {
    ui.scoped("section", Modifier::new(), move |ui| {
        if let Err(error) = list.scroll.smooth(ui.motion(), spring) {
            ui.fail(error);
        }
        ui.row(Modifier::new(), |ui| {
            list.compose(
                ui,
                RecyclerItems {
                    count: 20,
                    key: |i| i,
                    view_type: |_| 0,
                },
                RecyclerLayout::list(constant!(PositiveDp::try_new(Dp::new(40.0)))),
                Modifier::new().size(100.dp(), 120.dp()),
                |ui, item| {
                    ui.spacer(
                        item.size.width,
                        item.size.height,
                        Modifier::new().background(Color::WHITE),
                    );
                },
            );
            ui.scrollbar(
                list.scroll.clone(),
                Modifier::new().size(10.dp(), 120.dp()),
                |ui, bar| {
                    ui.spacer(
                        10.dp(),
                        bar.thumb_length(120.dp()),
                        bar.follow(120.dp()).background(Color::WHITE),
                    );
                },
            );
        });
    });
}

fn frame_at(view: &mut View, ms: u64) -> Result<SceneUpdate, FrameFailure> {
    let time = Duration::from_millis(ms);
    let timing = FrameTime {
        now: time,
        presentation: time,
        ..FrameTime::default()
    };
    (view.frame_at(SECTION, &mut Metrics, timing)).without_failures()
}

/// The jumps move the scroll in steps of 2 dp up to 18 dp. Up to 40 dp, rows 0 to 3 are in view, so
/// the window of the list does not change.
fn jumped_on_every_frame_composes_and_paints_nothing(
    style: InteractionStyle,
) -> Result<(), TestError> {
    let recycler = Recycler::<usize>::new();
    let mut view = section(&recycler, style)?;
    let mut tree = SceneTree::default();
    tree.show(frame_at(&mut view, 0)?)?;
    recycler.scroll.jump_to(2.dp());
    tree.show(frame_at(&mut view, 10)?)?;
    recycler.scroll.jump_to(4.dp());
    tree.show(frame_at(&mut view, 20)?)?;
    let before = view.stats();
    for step in 3..10_u16 {
        recycler.scroll.jump_to(2.dp() * f64::from(step));
        let update = frame_at(&mut view, 10 * (u64::from(step) + 1))?;
        assert_eq!(update.changes.len(), 2, "step {step}");
        assert!(
            (update.changes.iter()).all(|change| matches!(change, Change::SetSlot { .. })),
            "step {step}"
        );
        tree.show(update)?;
    }
    let after = view.stats();
    assert_eq!(after.component_compositions, before.component_compositions);
    assert_eq!(after.root_compositions, before.root_compositions);
    assert_eq!(after.painted_components, before.painted_components);
    Ok(())
}

#[test]
fn a_scrollbar_beside_a_list_jumped_on_every_frame_composes_and_paints_nothing()
-> Result<(), TestError> {
    jumped_on_every_frame_composes_and_paints_nothing(InteractionStyle::default())
}

/// A focus border paints only around a focused node. The thumb is never focused.
#[test]
fn a_thumb_under_a_focus_style_follows_the_list_without_painting() -> Result<(), TestError> {
    jumped_on_every_frame_composes_and_paints_nothing(InteractionStyle {
        focus: Some(BorderStyle {
            color: Color::rgb(0x20_20_20),
            width: constant!(NonNegativeDp::try_new(Dp::new(2.0))),
            radius: 4.dp(),
        }),
        ..InteractionStyle::default()
    })
}
