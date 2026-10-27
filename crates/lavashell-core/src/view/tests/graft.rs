// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::scene::tree::{
    Change, GroupValues, NamespaceId, NodeKey, PaintVisitor, PlacedLeaf, SceneTree,
};
use crate::shape::RoundedRect;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

/// A column with a spacer of 20 dp above a graft of `namespace` as large as `size`, which records each proposal.
fn grafting(
    namespace: NamespaceId,
    size: &State<Size>,
    proposals: &Rc<RefCell<Vec<Proposal>>>,
) -> View {
    let (size, proposals) = (size.clone(), proposals.clone());
    View::new(crate::platform::fixture::facts(), move |ui| {
        let (size, proposals) = (size.clone(), proposals.clone());
        ui.layout(Axis::Vertical, Modifier::new(), move |ui| {
            ui.spacer(10.dp(), 20.dp(), Modifier::new());
            ui.graft(namespace, Modifier::new(), move |proposal| {
                proposals.borrow_mut().push(proposal);
                size.get()
            });
        });
    })
}

/// Where a view places its graft.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Placement {
    Plain,
    Faded,
    Hidden,
}

/// A graft of `namespace` at `placement`, inside a faded column for `Placement::Faded`.
fn placed_graft(namespace: NamespaceId, placement: &State<Placement>) -> View {
    let placement = placement.clone();
    View::new(crate::platform::fixture::facts(), move |ui| {
        let graft = |ui: &mut Ui| {
            ui.graft(namespace, Modifier::new(), |_| Size::new(60.dp(), 30.dp()));
        };
        match placement.get() {
            Placement::Plain => graft(ui),
            Placement::Faded => ui.layout(Axis::Vertical, Modifier::new().opacity(0.5), graft),
            Placement::Hidden => {}
        }
    })
}

/// Finds the values of the innermost group around the graft of `namespace`.
struct GraftGroup {
    namespace: NamespaceId,
    groups: Vec<GroupValues>,
    found: Option<GroupValues>,
}

impl PaintVisitor<'_> for GraftGroup {
    /// Whether the step opened a group.
    type Saved = bool;

    fn group(&mut self, _: NodeKey, values: GroupValues, _: Affine) -> bool {
        self.groups.push(values);
        true
    }

    fn graft(&mut self, _: NodeKey, namespace: NamespaceId) -> bool {
        if namespace == self.namespace {
            self.found = self.groups.last().copied();
        }
        false
    }

    fn leaf(&mut self, _: PlacedLeaf<'_>) {}

    fn exit(&mut self, opened_group: bool) {
        if opened_group {
            let _closed_group = self.groups.pop();
        }
    }
}

/// The group around the graft of `namespace` in `tree`, or `None` where the graft shows nothing.
fn graft_group(tree: &SceneTree, namespace: NamespaceId) -> Option<GroupValues> {
    let mut visitor = GraftGroup {
        namespace,
        groups: Vec::new(),
        found: None,
    };
    tree.at(Duration::ZERO).paint_order().walk(&mut visitor);
    visitor.found
}

fn boxed(size: Size) -> GroupValues {
    GroupValues {
        transform: Affine::translate(Point::new(0.dp(), 20.dp())),
        clip: Some(RoundedRect::new(Rect::from_size(size), Corners::default())),
        ..GroupValues::default()
    }
}

#[test]
fn a_graft_shows_its_namespace_at_its_box_with_the_size_for_its_content_proposal() -> TestResult {
    let mut tree = SceneTree::default();
    let namespace = tree.create_namespace().id();
    let size = State::new(Size::new(60.dp(), 30.dp()));
    let proposals = Rc::new(RefCell::new(Vec::new()));
    let mut view = grafting(namespace, &size, &proposals);
    let viewport = Viewport::new(WINDOW_SIZE, Density::ONE);
    view.show_frame(&mut tree, viewport, &mut Metrics)?;
    assert_eq!(
        graft_group(&tree, namespace),
        Some(boxed(Size::new(60.dp(), 30.dp())))
    );
    let proposed = Proposal {
        width: Some(WINDOW_SIZE.width),
        height: Some(WINDOW_SIZE.height),
    };
    assert_eq!(proposals.borrow().last(), Some(&proposed));
    size.set(Size::new(80.dp(), 40.dp()));
    assert!(view.needs_visual_frame());
    view.show_frame(&mut tree, viewport, &mut Metrics)?;
    assert_eq!(
        graft_group(&tree, namespace),
        Some(boxed(Size::new(80.dp(), 40.dp())))
    );
    Ok(())
}

#[test]
fn a_graft_of_a_removed_namespace_shows_nothing_until_the_view_drops_it() -> TestResult {
    let mut tree = SceneTree::default();
    let created = tree.create_namespace();
    let namespace = created.id();
    let size = State::new(Size::new(60.dp(), 30.dp()));
    let mut view = grafting(namespace, &size, &Rc::default());
    let viewport = Viewport::new(WINDOW_SIZE, Density::ONE);
    view.show_frame(&mut tree, viewport, &mut Metrics)?;
    tree.remove_namespace(created);
    assert_eq!(graft_group(&tree, namespace), None);
    size.set(Size::new(70.dp(), 30.dp()));
    view.show_frame(&mut tree, viewport, &mut Metrics)?;
    assert_eq!(graft_group(&tree, namespace), None);
    Ok(())
}

#[test]
fn a_graft_inside_a_blur_fails_the_frame() {
    let mut tree = SceneTree::default();
    let namespace = tree.create_namespace().id();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        ui.layout(Axis::Vertical, Modifier::new().blur(4.dp()), move |ui| {
            ui.graft(namespace, Modifier::new(), |_| Size::new(60.dp(), 30.dp()));
        });
    });
    let shown = view.show_frame(
        &mut tree,
        Viewport::new(WINDOW_SIZE, Density::ONE),
        &mut Metrics,
    );
    assert_eq!(shown, Err(FrameError::GraftInLayer(namespace)));
    assert_eq!(graft_group(&tree, namespace), None);
}

#[test]
fn a_graft_of_a_removed_namespace_shows_nothing_wherever_its_box_moves() -> TestResult {
    let mut tree = SceneTree::default();
    let created = tree.create_namespace();
    let namespace = created.id();
    let placement = State::new(Placement::Plain);
    let mut view = placed_graft(namespace, &placement);
    let viewport = Viewport::new(WINDOW_SIZE, Density::ONE);
    view.show_frame(&mut tree, viewport, &mut Metrics)?;
    tree.remove_namespace(created);
    for moved in [Placement::Faded, Placement::Hidden, Placement::Plain] {
        placement.set(moved);
        view.show_frame(&mut tree, viewport, &mut Metrics)?;
        assert_eq!(graft_group(&tree, namespace), None);
    }
    Ok(())
}

#[test]
fn a_graft_of_a_removed_namespace_shows_nothing_after_a_failed_frame() -> TestResult {
    let mut tree = SceneTree::default();
    let created = tree.create_namespace();
    let namespace = created.id();
    let size = State::new(Size::new(60.dp(), 30.dp()));
    let mut view = grafting(namespace, &size, &Rc::default());
    let viewport = Viewport::new(WINDOW_SIZE, Density::ONE);
    view.show_frame(&mut tree, viewport, &mut Metrics)?;
    tree.remove_namespace(created);
    let shown = tree.children(SceneTree::ROOT, None).to_vec();
    tree.show(shown.into_iter().map(|id| Change::Remove { id }).collect())?;
    size.set(Size::new(70.dp(), 30.dp()));
    let failed = view.show_frame(&mut tree, viewport, &mut Metrics);
    assert!(matches!(failed, Err(FrameError::Apply(_))));
    view.show_frame(&mut tree, viewport, &mut Metrics)?;
    assert_eq!(graft_group(&tree, namespace), None);
    Ok(())
}

#[test]
fn a_namespace_grafted_twice_fails_the_frame() {
    let mut tree = SceneTree::default();
    let namespace = tree.create_namespace().id();
    let mut view = View::new(crate::platform::fixture::facts(), move |ui| {
        ui.layout(Axis::Vertical, Modifier::new(), move |ui| {
            for _ in 0..2 {
                ui.graft(namespace, Modifier::new(), |_| Size::new(60.dp(), 30.dp()));
            }
        });
    });
    let shown = view.show_frame(
        &mut tree,
        Viewport::new(WINDOW_SIZE, Density::ONE),
        &mut Metrics,
    );
    assert_eq!(shown, Err(FrameError::GraftedTwice(namespace)));
}
