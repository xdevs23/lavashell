// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::renderer_actor::Rendered;
use crate::scheduler::FrameKind;
use lavashell_core::frame::{FrameTime, RenderStats};
use lavashell_core::geometry::Viewport;
use lavashell_core::view::View;
use lavashell_platform::{Drawn, FrameOutcome, SurfaceContents, SurfaceEvent};
use std::cell::RefCell;

fn configured(size: Size) -> PlatformEvent {
    PlatformEvent::Surface(
        WINDOW,
        SurfaceEvent::Configured {
            width: Some(size.width),
            height: Some(size.height),
            token: None,
            window: None,
        },
    )
}

fn start_frame(views: &mut Views, kind: FrameKind, time: FrameTime) -> TestResult {
    let frame = SchedulerOutput::Frame {
        surface: WINDOW,
        kind,
        time,
    };
    views.actor.handle(ViewInput::Scheduler(frame))?;
    views.actor.turn()?;
    Ok(())
}

fn shown(views: &Views, time: FrameTime) -> Result<ViewInput, TestError> {
    let scaled_viewport = (views.actor.surfaces.records().open_scaled_viewport(WINDOW))
        .ok_or(TestError::Missing("the scaled viewport of the window"))?;
    let drawn = Drawn {
        outcome: FrameOutcome::Shown,
        stats: RenderStats::default(),
        gpu: None,
        output: None,
        scene_generation: None,
    };
    let rendered = Rendered {
        surface: WINDOW,
        time,
        scaled_viewport,
        drawn,
        contents: SurfaceContents::Current,
        next_change: None,
        cost: Duration::ZERO,
        cpu_time: None,
    };
    Ok(ViewInput::Renderer(RendererOutput::Rendered(Box::new(
        rendered,
    ))))
}

#[test]
fn a_frame_resized_while_it_is_drawn_is_viewed_at_the_size_it_was_composed_at() -> TestResult {
    let mut setup = runtime_setup(vec![window("window")]);
    let viewed = Rc::new(RefCell::new(Vec::new()));
    let seen = Rc::clone(&viewed);
    setup.observers.view = Some(Box::new(move |_, _, viewport: Viewport, _: &View| {
        seen.borrow_mut().push(viewport.size);
    }));
    let mut views = Views::start(setup, Capability::ALL, host([]))?;
    let composed = Size::new(100.dp(), 100.dp());
    views.play([configured(composed)])?;
    let time = FrameTime::default();
    start_frame(&mut views, FrameKind::Compose, time)?;
    start_frame(&mut views, FrameKind::Render, time)?;
    views.play([configured(Size::new(200.dp(), 100.dp()))])?;
    let rendered = shown(&views, time)?;
    views.actor.handle(rendered)?;
    assert_eq!(viewed.take(), [composed]);
    Ok(())
}
