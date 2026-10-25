// SPDX-License-Identifier: GPL-3.0-only
use super::probe::{WINDOW, Wrapping};
use super::*;
use crate::scroll::ScrollState;
use std::cell::Cell;
use std::rc::Rc;

struct Traced {
    view: View,
    scene: Scene,
    scroll: ScrollState,
    measured: Rc<Cell<u32>>,
    clicks: Rc<Cell<u32>>,
    stripe: State<Dp>,
}

impl Traced {
    fn new() -> Result<Self, TestError> {
        let scroll = ScrollState::new(Axis::Vertical);
        let measured = Rc::new(Cell::new(0));
        let clicks = Rc::new(Cell::new(0));
        let stripe = State::new(10.dp());
        let (list, counted, clicked) = (scroll.clone(), measured.clone(), clicks.clone());
        let painted = stripe.clone();
        let view = View::new(crate::platform::fixture::facts(), move |ui| {
            let list = list.clone();
            let (counted, clicked, painted) = (counted.clone(), clicked.clone(), painted.clone());
            ui.layout(Axis::Vertical, Modifier::new(), |ui| {
                ui.scoped("list", Modifier::new(), move |ui| {
                    ui.scroll(list.clone(), Modifier::new().height(100.dp()), |ui| {
                        for _ in 0..10 {
                            ui.spacer(80.dp(), 30.dp(), Modifier::new().on_click(|| {}));
                        }
                    });
                });
                ui.spacer(
                    60.dp(),
                    20.dp(),
                    Modifier::new()
                        .tag("button")
                        .on_measure(move |_| counted.set(counted.get() + 1))
                        .on_click(move || clicked.set(clicked.get() + 1))
                        .background(Color::WHITE),
                );
                ui.canvas(
                    Size::new(40.dp(), 10.dp()),
                    Modifier::new(),
                    move |drawing| {
                        let rect = Rect::new(0.dp(), 0.dp(), painted.get(), 10.dp());
                        drawing.fill(rect, Color::WHITE);
                    },
                );
            });
        });
        let mut traced = Self {
            view,
            scene: Scene::default(),
            scroll,
            measured,
            clicks,
            stripe,
        };
        traced.frame()?;
        traced.view.input(InputEvent::Scroll {
            at: Point::new(10.dp(), 10.dp()),
            delta: Point::new(Dp::ZERO, 150.dp()),
        });
        traced.frame()?;
        Ok(traced)
    }

    fn frame(&mut self) -> TestResult {
        self.scene.show(self.view.frame(WINDOW, &mut Wrapping)?)?;
        Ok(())
    }

    fn click_button(&mut self) -> TestResult {
        let button = self.view.tagged_bounds("button")?;
        let at = Point::new(button.x + 5.dp(), button.y + 5.dp());
        self.view.input(InputEvent::Press(at));
        self.view.input(InputEvent::Release(at));
        Ok(())
    }

    fn assert_same(&self, other: &Self) {
        let counts = |stats: ViewStats| {
            (
                stats.root_compositions,
                stats.component_compositions,
                stats.painted_components,
                stats.reused_components,
                stats.measured_components,
                stats.reused_layouts,
            )
        };
        assert_eq!(self.scene.commands(), other.scene.commands());
        assert_eq!(counts(self.view.stats()), counts(other.view.stats()));
        assert_eq!(
            self.view.bounds_of("button"),
            other.view.bounds_of("button")
        );
        assert_eq!(self.scroll.offset(), other.scroll.offset());
        assert_eq!(self.scroll.viewport(), other.scroll.viewport());
        assert_eq!(self.scroll.content(), other.scroll.content());
        assert_eq!(self.measured.get(), other.measured.get());
        assert_eq!(self.clicks.get(), other.clicks.get());
        assert_eq!(self.view.needs_frame(), other.view.needs_frame());
    }

    fn repaint(&mut self) -> TestResult {
        self.stripe.set(30.dp());
        self.frame()
    }

    fn frame_again(&mut self) -> TestResult {
        self.view.invalidate();
        self.frame()
    }
}

#[test]
fn a_probe_leaves_layout_paint_input_scrolling_and_measure_callbacks_as_they_were() -> TestResult {
    let mut probed = Traced::new()?;
    let mut plain = Traced::new()?;
    assert_eq!(probed.scroll.offset(), 150.dp());
    let measured = probed.measured.get();
    let proposal = Proposal {
        width: Some(30.dp()),
        height: Some(50.dp()),
    };
    let size = probed.view.size_that_fits(proposal, &mut Wrapping);
    assert_eq!(size, Ok(Size::new(80.dp(), 130.dp())));
    assert_eq!(probed.measured.get(), measured);
    probed.assert_same(&plain);

    probed.repaint()?;
    plain.repaint()?;
    probed.assert_same(&plain);

    let reused = probed.view.stats().reused_layouts;
    probed.frame_again()?;
    plain.frame_again()?;
    probed.assert_same(&plain);
    assert_eq!(probed.view.stats().reused_layouts, reused + 1);

    for traced in [&mut probed, &mut plain] {
        traced.click_button()?;
        traced.view.input(InputEvent::Scroll {
            at: Point::new(10.dp(), 10.dp()),
            delta: Point::new(Dp::ZERO, -50.dp()),
        });
    }
    probed.assert_same(&plain);
    assert_eq!(probed.clicks.get(), 1);
    assert_eq!(probed.scroll.offset(), 100.dp());

    probed.frame_again()?;
    plain.frame_again()?;
    probed.assert_same(&plain);
    Ok(())
}

#[test]
fn a_filling_child_of_a_scroll_container_has_its_minimum_height_in_a_frame() -> TestResult {
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), |ui| {
        ui.scroll(Axis::Vertical, Modifier::new().height(100.dp()), |ui| {
            ui.spacer(
                50.dp(),
                40.dp(),
                Modifier::new().tag("filling").fill_max_height(),
            );
            ui.spacer(50.dp(), 20.dp(), Modifier::new());
        });
    });
    advance_with(&mut scene, &mut view, WINDOW, &mut Wrapping)?;
    assert_eq!(view.tagged_bounds("filling")?.height, 40.dp());
    Ok(())
}

#[test]
fn an_overscroll_inside_a_scoped_list_is_taken_back_before_the_list_grows() -> TestResult {
    let scroll = ScrollState::new(Axis::Vertical);
    let rows = State::new(10);
    let (list, count) = (scroll.clone(), rows.clone());
    let mut scene = Scene::default();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        let (list, count) = (list.clone(), count.clone());
        ui.scoped("list", Modifier::new(), move |ui| {
            let rows = count.get();
            ui.scroll(list.clone(), Modifier::new().height(100.dp()), |ui| {
                for _ in 0..rows {
                    ui.spacer(80.dp(), 30.dp(), Modifier::new());
                }
            });
        });
    });
    advance_with(&mut scene, &mut view, WINDOW, &mut Wrapping)?;
    scroll.jump_to(200.dp());
    advance_with(&mut scene, &mut view, WINDOW, &mut Wrapping)?;
    scroll.scroll_by(50.dp());
    advance_with(&mut scene, &mut view, WINDOW, &mut Wrapping)?;
    rows.set(14);
    advance_with(&mut scene, &mut view, WINDOW, &mut Wrapping)?;
    assert_eq!(scroll.offset(), 200.dp());
    Ok(())
}
