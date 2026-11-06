// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::test_error::{TestError, TestResult};
use lavashell_core::geometry::Viewport;
use lavashell_core::prelude::Modifier;
use lavashell_core::units::{Density, Dp, DpExt, FiniteDp, NonNegativeDp};
use lavashell_test_fixtures::{NoText, RecordedScene, WithoutFailures};

const PANEL: SurfaceId = SurfaceId(5);

/// A view whose content is 500 dp wide and as tall as `height`.
fn view(height: &State<Dp>) -> View {
    let height = height.clone();
    View::new(lavashell_test_fixtures::facts(), move |ui| {
        ui.spacer(500.dp(), height.get(), Modifier::new());
    })
}

/// Frames `view`, so that it needs a new frame only after something invalidates it.
fn frame(view: &mut View) -> TestResult {
    let viewport = Viewport::new(Size::new(500.dp(), 40.dp()), Density::ONE);
    (view.frame_into(viewport, &mut NoText, &mut RecordedScene::default())).without_failures()?;
    assert!(!view.needs_visual_frame());
    Ok(())
}

fn open(records: &mut PanelRecords, options: PanelOptions, view: &mut View) -> TestResult {
    records.open(PANEL, Panel::new(options, view, &mut NoText)?);
    Ok(())
}

fn panel(records: &PanelRecords) -> Result<&Panel, TestError> {
    records.get(PANEL).ok_or(TestError::NoPanel)
}

#[test]
fn the_keyboard_policy_asks_for_on_demand_focus_only_while_its_condition_holds() {
    assert_eq!(KeyboardPolicy::Off.mode(), KeyboardMode::Off);
    assert_eq!(KeyboardPolicy::OnDemand.mode(), KeyboardMode::OnDemand);
    assert_eq!(
        KeyboardPolicy::OnDemandWhen(Rc::new(|| false)).mode(),
        KeyboardMode::Off
    );
    assert_eq!(
        KeyboardPolicy::OnDemandWhen(Rc::new(|| true)).mode(),
        KeyboardMode::OnDemand
    );
}

#[test]
fn the_layout_state_replaces_the_fixed_layout() -> TestResult {
    let tall = AxisPlacement::Start(Extent::dp(345.dp())?);
    let layout = State::new(LayerLayout {
        vertical: tall,
        ..PanelOptions::default().layout
    });
    let options = PanelOptions {
        layout_state: Some(layout.clone()),
        ..PanelOptions::default()
    };
    assert_eq!(options.current_layout().vertical, tall);
    let short = AxisPlacement::Start(Extent::dp(1.dp())?);
    layout.update(|layout| layout.vertical = short);
    assert_eq!(options.current_layout().vertical, short);
    Ok(())
}

#[test]
fn a_new_panel_lays_out_its_content_and_receives_the_keyboard_after_a_shown_frame() -> TestResult {
    let mut view = view(&State::new(40.dp()));
    let options = PanelOptions {
        keyboard: KeyboardPolicy::OnDemand,
        ..PanelOptions::default()
    };
    let mut records = PanelRecords::default();
    open(&mut records, options, &mut view)?;
    let geometry = panel(&records)?.geometry();
    assert_eq!(
        geometry.vertical(),
        AxisPlacement::Start(NonNegativeDp::try_new(40.dp())?)
    );
    assert_eq!(panel(&records)?.keyboard_mode(), KeyboardMode::OnDemand);
    assert!(records.update(PANEL).is_none());
    assert!(!panel(&records)?.keyboard_granted());
    assert!(!panel(&records)?.input_enabled());
    let size = Size::new(500.dp(), 40.dp());
    records.shown(PANEL, PanelContent { size, input: true });
    let shown = panel(&records)?;
    assert!(shown.keyboard_granted() && shown.input_enabled());
    assert!(shown.content_current(size));
    assert!(!shown.content_current(Size::new(500.dp(), 41.dp())));
    Ok(())
}

#[test]
fn a_panel_lays_out_again_when_its_content_measures_differently() -> TestResult {
    let height = State::new(40.dp());
    let mut view = view(&height);
    let mut records = PanelRecords::default();
    open(&mut records, PanelOptions::default(), &mut view)?;
    records.measure(PANEL, &mut view, &mut NoText)?;
    assert!(records.update(PANEL).is_none());
    height.set(60.dp());
    frame(&mut view)?;
    records.measure(PANEL, &mut view, &mut NoText)?;
    let content = ContentSize::try_from(Size::new(500.dp(), 60.dp()))?;
    let geometry = PanelGeometry::of(PanelOptions::default().layout, content);
    assert!(records.update(PANEL).is_none());
    assert_eq!(panel(&records)?.geometry(), geometry);
    Ok(())
}

#[test]
fn a_change_commits_at_once_only_while_the_host_shows_the_current_content() -> TestResult {
    let mut view = view(&State::new(40.dp()));
    let active = State::new(false);
    let condition = active.clone();
    let options = PanelOptions {
        keyboard: KeyboardPolicy::OnDemandWhen(Rc::new(move || condition.get())),
        ..PanelOptions::default()
    };
    let mut records = PanelRecords::default();
    open(&mut records, options, &mut view)?;
    let size = Size::new(500.dp(), 40.dp());
    records.shown(PANEL, PanelContent { size, input: true });
    active.set(true);
    let waiting = records.update_transaction(PANEL, ContentState::Stale);
    let enable = PanelUpdate {
        keyboard: Some(KeyboardMode::OnDemand),
        transaction: Transaction::NeedsFrame,
    };
    assert_eq!(waiting, enable);
    assert!(!panel(&records)?.keyboard_granted());
    let committed = records.update_transaction(PANEL, ContentState::Current);
    assert_eq!(committed.transaction, Transaction::Committed);
    assert_eq!(committed.keyboard, None);
    assert!(panel(&records)?.keyboard_granted());
    let settled = records.update_transaction(PANEL, ContentState::Current);
    assert_eq!(settled.transaction, Transaction::Settled);
    assert_eq!(settled.keyboard, None);
    Ok(())
}

#[test]
fn a_forgotten_panel_makes_no_requests() -> TestResult {
    let mut view = view(&State::new(40.dp()));
    let options = PanelOptions {
        keyboard: KeyboardPolicy::OnDemand,
        ..PanelOptions::default()
    };
    let mut records = PanelRecords::default();
    open(&mut records, options, &mut view)?;
    records.forget(PANEL);
    assert!(records.get(PANEL).is_none());
    let settled = records.update_transaction(PANEL, ContentState::Current);
    assert_eq!(settled.transaction, Transaction::Settled);
    assert_eq!(settled.keyboard, None);
    Ok(())
}

#[test]
fn a_layout_change_below_half_a_dp_reaches_the_host_as_no_change() -> TestResult {
    let mut view = view(&State::new(40.dp()));
    let layout = State::new(PanelOptions::default().layout);
    let options = PanelOptions {
        layout_state: Some(layout.clone()),
        keyboard: KeyboardPolicy::OnDemand,
        ..PanelOptions::default()
    };
    let mut records = PanelRecords::default();
    open(&mut records, options, &mut view)?;
    let size = Size::new(500.dp(), 40.dp());
    records.shown(PANEL, PanelContent { size, input: true });
    let nudged = FiniteDp::try_new(0.2.dp())?;
    layout.update(|layout| layout.margins.top = nudged);
    let settled = records.update_transaction(PANEL, ContentState::Stale);
    assert_eq!(settled.transaction, Transaction::Settled);
    assert_eq!(settled.keyboard, None);
    assert!(panel(&records)?.keyboard_granted());
    Ok(())
}
