// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use lavashell_core::view::{CompositionError, LayoutError};

fn collection(view: &View) -> Result<NodeId, TestError> {
    view.accessibility()
        .nodes
        .values()
        .find(|node| node.role == Role::List)
        .map(|node| node.id)
        .ok_or(TestError::NotExported("a list"))
}

fn collection_of(view: &View) -> Result<CollectionInfo, TestError> {
    let list = collection(view)?;
    view.accessibility().nodes[&list]
        .collection
        .ok_or(TestError::NotExported("the collection of the list"))
}

fn item(view: &View, name: &str) -> Result<NodeId, TestError> {
    view.accessibility()
        .nodes
        .values()
        .find(|node| node.role == Role::ListItem && node.name.as_deref() == Some(name))
        .map(|node| node.id)
        .ok_or_else(|| TestError::NoNodeNamed(name.to_owned()))
}

fn item_of(view: &View, id: NodeId) -> Result<CollectionItem, TestError> {
    view.accessibility().nodes[&id]
        .item
        .ok_or(TestError::NotExported("the item of the row"))
}

/// The request to bring the cell at `row` and `column` of the list of `view` into view.
fn reveal(view: &View, row: usize, column: usize) -> Result<ActionRequest, TestError> {
    Ok(ActionRequest {
        target: collection(view)?,
        action: AccessibilityAction::RevealCollectionItem { row, column },
    })
}

#[test]
fn recycled_semantics_follow_data_identity_through_rebinding_and_sorting() -> Result<(), TestError>
{
    let recycler = Recycler::<usize>::new();
    let order = State::new((0..1000usize).collect::<Vec<_>>());
    let list = recycler.clone();
    let data = order.clone();
    let mut view = View::new(lavashell_test_platform::facts(), move |ui| {
        let order = data.get();
        list.compose(
            ui,
            RecyclerItems {
                count: order.len(),
                key: |i| order[i],
                view_type: |_| 0,
            },
            RecyclerLayout::list(constant!(PositiveDp::try_new(Dp::new(40.0)))),
            Modifier::new().fill_max_width().fill_max_height(),
            |ui, row| {
                ui.text(format!("Row {}", row.key), Modifier::new());
            },
        );
    });
    let size = Size::new(300.dp(), 200.dp());
    let viewport = Viewport::new(size, Density::ONE);
    for _ in 0..3 {
        render(&mut view, viewport, &mut Metrics)?;
    }
    let id = item(&view, "Row 0")?;
    let list_id = collection(&view)?;
    let info = collection_of(&view)?;
    assert_eq!(info.item_count, Some(1000));
    assert_eq!(info.rows, Some(1000));
    assert!(view.accessibility().nodes[&list_id].children.contains(&id));
    assert_eq!(item_of(&view, id)?.row, 0);
    assert_eq!(view.accessibility_action(reveal(&view, 500, 0)?), PERFORMED);
    render(&mut view, viewport, &mut Metrics)?;
    assert!(!view.accessibility().nodes.contains_key(&id));
    assert_eq!(
        recycler.stats().active,
        3 * 5,
        "a jump composes the 5 rows in view and 5 more on each side"
    );
    assert!(
        view.accessibility().nodes.len() < 40,
        "logical size must not materialize the complete collection"
    );
    assert_eq!(view.accessibility_action(reveal(&view, 0, 0)?), PERFORMED);
    render(&mut view, viewport, &mut Metrics)?;
    assert_eq!(
        item(&view, "Row 0")?,
        id,
        "a recycled holder must not change semantic identity"
    );
    order.update(|rows| rows.reverse());
    render(&mut view, viewport, &mut Metrics)?;
    assert_eq!(view.accessibility_action(reveal(&view, 999, 0)?), PERFORMED);
    render(&mut view, viewport, &mut Metrics)?;
    assert_eq!(collection(&view)?, list_id);
    assert_eq!(
        item(&view, "Row 0")?,
        id,
        "sorting must preserve data identity"
    );
    assert_eq!(item_of(&view, id)?.row, 999);
    assert!(recycler.stats().holders < 24);
    let offset = recycler.scroll.offset();
    assert_eq!(view.accessibility_action(reveal(&view, 1000, 0)?), IGNORED);
    assert_eq!(view.accessibility_action(reveal(&view, 0, 1)?), IGNORED);
    assert_eq!(
        view.accessibility_action(reveal(&view, usize::MAX, usize::MAX)?),
        IGNORED
    );
    assert_eq!(recycler.scroll.offset(), offset);
    Ok(())
}

#[test]
fn a_grid_exposes_logical_columns_and_rejects_a_vacant_final_cell() -> Result<(), TestError> {
    let list = Recycler::<usize>::new();
    let mut view = View::new(lavashell_test_platform::facts(), move |ui| {
        list.compose(
            ui,
            RecyclerItems {
                count: 4,
                key: |i| i,
                view_type: |_| 0,
            },
            RecyclerLayout::grid(
                constant!(PositiveDp::try_new(Dp::new(40.0))),
                constant!(PositiveDp::try_new(Dp::new(150.0))),
                constant!(NonNegativeDp::try_new(Dp::new(10.0))),
            ),
            Modifier::new().fill_max_width().fill_max_height(),
            |ui, row| {
                ui.text(format!("Row {}", row.key), Modifier::new());
            },
        );
    });
    for _ in 0..3 {
        render(
            &mut view,
            Viewport::new(Size::new(500.dp(), 100.dp()), Density::ONE),
            &mut Metrics,
        )?;
    }
    let info = collection_of(&view)?;
    assert_eq!(info.columns, Some(3));
    assert_eq!(info.rows, Some(2));
    assert_eq!(info.item_count, Some(4));
    let row = item_of(&view, item(&view, "Row 3")?)?;
    assert_eq!((row.row, row.column), (1, 0));
    assert_eq!(view.accessibility_action(reveal(&view, 1, 1)?), IGNORED);
    assert_eq!(view.accessibility_action(reveal(&view, 1, 2)?), IGNORED);
    assert_eq!(view.accessibility_action(reveal(&view, 1, 0)?), PERFORMED);
    Ok(())
}

#[test]
fn measured_collection_realizes_offscreen_rows_without_building_the_history()
-> Result<(), TestError> {
    let list = MeasuredRecycler::new();
    list.set_keys((0..10_000usize).collect())?;
    let controlled = list.clone();
    let mut view = View::new(lavashell_test_platform::facts(), move |ui| {
        controlled.compose(
            ui,
            Modifier::new().fill_max_width().fill_max_height(),
            |ui, row| {
                ui.text(format!("Row {}", row.key), Modifier::new());
                ui.spacer(
                    row.size.width,
                    24.dp() + repeat_n(12.dp(), row.index % 5).sum::<Dp>(),
                    Modifier::new(),
                );
            },
        );
    });
    let size = Size::new(300.dp(), 200.dp());
    let viewport = Viewport::new(size, Density::ONE);
    for _ in 0..3 {
        render(&mut view, viewport, &mut Metrics)?;
    }
    let id = item(&view, "Row 0")?;
    assert_eq!(collection_of(&view)?.item_count, Some(10_000));
    assert_eq!(
        view.accessibility_action(reveal(&view, 5000, 0)?),
        PERFORMED
    );
    while view.needs_frame() {
        render(&mut view, viewport, &mut Metrics)?;
    }
    assert!(!view.accessibility().nodes.contains_key(&id));
    assert!(view.accessibility().nodes.values().any(|node| {
        node.role == Role::ListItem && node.item.is_some_and(|item| item.row == 5000)
    }));
    assert!(list.stats().active < 24);
    assert!(view.accessibility().nodes.len() < 100);
    list.set_keys((0..10_000usize).rev().collect())?;
    render(&mut view, viewport, &mut Metrics)?;
    assert_eq!(
        view.accessibility_action(reveal(&view, 9999, 0)?),
        PERFORMED
    );
    for _ in 0..3 {
        render(&mut view, viewport, &mut Metrics)?;
    }
    assert_eq!(item(&view, "Row 0")?, id);
    assert_eq!(item_of(&view, id)?.row, 9999);
    assert_eq!(view.accessibility_action(reveal(&view, 0, 1)?), IGNORED);
    assert_eq!(
        view.accessibility_action(reveal(&view, 10_000, 0)?),
        IGNORED
    );
    Ok(())
}

#[test]
fn different_holder_types_cannot_disguise_duplicate_data_identity() {
    let list = Recycler::<usize>::new();
    let mut view = View::new(lavashell_test_platform::facts(), move |ui| {
        list.compose(
            ui,
            RecyclerItems {
                count: 2,
                key: |_| 7,
                view_type: |index| index as u64,
            },
            RecyclerLayout::list(constant!(PositiveDp::try_new(Dp::new(40.0)))),
            Modifier::new().fill_max_width().fill_max_height(),
            |ui, item| {
                ui.spacer(item.size.width, item.size.height, Modifier::new());
            },
        );
    });
    let viewport = Viewport::new(Size::new(300.dp(), 200.dp()), Density::ONE);
    assert_eq!(
        render(&mut view, viewport, &mut Metrics),
        Err(FrameError::Layout(LayoutError::Composition(
            CompositionError::DuplicateItemKey
        )))
    );
}
