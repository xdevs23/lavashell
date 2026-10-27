// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::effects::BlendMode;
use crate::geometry::{Color, Rect};
use crate::scene::tree::{NamespaceId, SceneTree};
use crate::shape::{Corners, Radius, RoundedRect};
use crate::test_error::{TestError, TestResult};
use crate::units::{Dp, DpExt, NonNegativeDp};
use crate::view::FrameError;

/// The structure of painted items, with leaves written as their commands.
#[derive(Debug, PartialEq)]
enum Item {
    Leaf(Vec<DrawCommand>),
    Scope(GroupValues, Vec<Item>),
    Group(GroupId),
    Graft(NamespaceId),
}

fn items(painted: &[Painted]) -> Vec<Item> {
    (painted.iter())
        .map(|painted| match painted {
            Painted::Leaf { commands } => Item::Leaf(commands.to_vec()),
            Painted::Scope {
                values,
                items: inner,
            } => Item::Scope(*values, items(inner)),
            Painted::Group { id, .. } => Item::Group(*id),
            Painted::Graft(namespace) => Item::Graft(*namespace),
        })
        .collect()
}

const BOUNDS: Rect = Rect::new(Dp::ZERO, Dp::ZERO, Dp::new(100.0), Dp::new(80.0));

fn fill(x: f64) -> DrawCommand {
    DrawCommand::Fill {
        shape: Rect::new(Dp::new(x), Dp::ZERO, 4.dp(), 4.dp()).into(),
        brush: Color::WHITE.into(),
    }
}

fn clip(corners: Corners<Radius>) -> ScopeKind {
    ScopeKind::clip(BOUNDS, corners)
}

/// A component at (10, 20) that paints `commands`.
fn component(commands: Vec<DrawCommand>) -> Painted {
    Painted::Group {
        id: GroupId::Component(NodeId(7)),
        group: placed_at(Point::new(10.dp(), 20.dp())),
        items: [Painted::Leaf {
            commands: commands.into(),
        }]
        .into(),
    }
}

const COMPONENT: Item = Item::Group(GroupId::Component(NodeId(7)));

#[test]
fn a_scope_without_nested_items_paints_as_commands_of_the_leaf() {
    let mut canvas = Canvas::default();
    canvas.push(fill(0.0));
    canvas.open_scope(clip(Corners::default()));
    canvas.push(fill(1.0));
    canvas.close();
    let clipped = DrawCommand::Clip {
        rect: BOUNDS,
        corners: Corners::default(),
    };
    assert_eq!(
        items(&canvas.finish()),
        [Item::Leaf(vec![
            fill(0.0),
            DrawCommand::Save,
            clipped,
            fill(1.0),
            DrawCommand::Restore,
        ])]
    );
}

#[test]
fn a_nested_component_paints_between_the_leaves_before_and_after_it() -> TestResult {
    let mut canvas = Canvas::default();
    canvas.push(fill(0.0));
    canvas.nest(component(vec![fill(5.0)]))?;
    canvas.push(fill(1.0));
    assert_eq!(
        items(&canvas.finish()),
        [
            Item::Leaf(vec![fill(0.0)]),
            COMPONENT,
            Item::Leaf(vec![fill(1.0)]),
        ]
    );
    Ok(())
}

/// The commands painted after the clip closes paint outside the group made for the clip.
#[test]
fn a_clip_open_around_a_nested_component_becomes_a_group() -> TestResult {
    let rounded = Corners::all(6.dp());
    let mut canvas = Canvas::default();
    canvas.push(fill(0.0));
    canvas.open_scope(clip(rounded.into()));
    canvas.push(fill(1.0));
    canvas.nest(component(vec![fill(5.0)]))?;
    canvas.push(fill(2.0));
    assert!(!canvas.flattened());
    canvas.close();
    canvas.push(fill(3.0));
    let clipped = GroupValues {
        clip: Some(RoundedRect::new(BOUNDS, rounded)),
        ..GroupValues::default()
    };
    assert_eq!(
        items(&canvas.finish()),
        [
            Item::Leaf(vec![fill(0.0)]),
            Item::Scope(
                clipped,
                vec![
                    Item::Leaf(vec![fill(1.0)]),
                    COMPONENT,
                    Item::Leaf(vec![fill(2.0)]),
                ]
            ),
            Item::Leaf(vec![fill(3.0)]),
        ]
    );
    Ok(())
}

#[test]
fn scopes_open_around_a_nested_component_nest_in_the_order_they_opened() -> TestResult {
    let moved = Affine::translate(Point::new(3.dp(), 4.dp()));
    let mut canvas = Canvas::default();
    canvas.open_scope(ScopeKind::transform(moved));
    canvas.open_scope(ScopeKind::opacity(0.5));
    canvas.nest(component(vec![fill(5.0)]))?;
    canvas.close();
    canvas.close();
    let faded = GroupValues {
        opacity: 0.5,
        ..GroupValues::default()
    };
    let transformed = GroupValues {
        transform: moved,
        ..GroupValues::default()
    };
    assert_eq!(
        items(&canvas.finish()),
        [Item::Scope(
            transformed,
            vec![Item::Scope(faded, vec![COMPONENT])]
        )]
    );
    Ok(())
}

/// An effect layer filters everything painted inside it as one image. A group cannot do that, so the component inside
/// the effect layer paints as commands.
#[test]
fn a_component_nested_in_an_effect_layer_paints_as_commands_of_the_leaf() -> TestResult {
    const BLUR: NonNegativeDp = crate::constant!(NonNegativeDp::try_new(Dp::new(2.0)));
    let mut canvas = Canvas::default();
    canvas.open_scope(ScopeKind::effect(BOUNDS, BLUR, None));
    assert!(canvas.flattened());
    canvas.push(fill(0.0));
    canvas.nest(component(vec![fill(5.0)]))?;
    canvas.close();
    assert!(!canvas.flattened());
    let effect = DrawCommand::EffectLayer {
        bounds: BOUNDS,
        blend: BlendMode::SourceOver,
        blur: BLUR,
        filter: None,
    };
    let placed = DrawCommand::Transform(Affine::translate(Point::new(10.dp(), 20.dp())));
    assert_eq!(
        items(&canvas.finish()),
        [Item::Leaf(vec![
            effect,
            fill(0.0),
            DrawCommand::Save,
            placed,
            fill(5.0),
            DrawCommand::Restore,
            DrawCommand::Restore,
        ])]
    );
    Ok(())
}

/// A group can clip only with circular corners. A clip with elliptical corners therefore paints as commands.
#[test]
fn a_clip_with_elliptical_corners_paints_the_nested_component_as_commands() -> TestResult {
    let elliptical = Corners::all(Radius::new(4.dp(), 8.dp()));
    let mut canvas = Canvas::default();
    canvas.open_scope(clip(elliptical));
    assert!(canvas.flattened());
    canvas.nest(component(Vec::new()))?;
    canvas.close();
    let clipped = DrawCommand::Clip {
        rect: BOUNDS,
        corners: elliptical,
    };
    assert_eq!(
        items(&canvas.finish()),
        [Item::Leaf(vec![
            DrawCommand::Save,
            clipped,
            DrawCommand::Save,
            DrawCommand::Transform(Affine::translate(Point::new(10.dp(), 20.dp()))),
            DrawCommand::Restore,
            DrawCommand::Restore,
        ])]
    );
    Ok(())
}

#[test]
fn a_group_contains_what_was_painted_while_it_was_open() -> TestResult {
    let mut canvas = Canvas::default();
    canvas.push(fill(0.0));
    canvas.open_group(GroupId::Content(NodeId(3)), Group::default());
    canvas.push(fill(1.0));
    canvas.close();
    let painted = canvas.finish();
    let Some(Painted::Group { items: inner, .. }) = painted.get(1) else {
        return Err(TestError::Absent("a group after the first leaf"));
    };
    assert_eq!(items(inner), [Item::Leaf(vec![fill(1.0)])]);
    assert_eq!(items(&painted[..1]), [Item::Leaf(vec![fill(0.0)])]);
    Ok(())
}

#[test]
fn a_graft_paints_inside_its_group_and_fails_inside_an_effect_layer_also_when_nested() {
    let namespace = SceneTree::default().create_namespace().id();
    let boxed = GroupValues {
        clip: Some(RoundedRect::new(BOUNDS, Corners::default())),
        ..GroupValues::default()
    };
    let mut canvas = Canvas::default();
    assert!(canvas.graft(boxed, namespace).is_ok());
    assert_eq!(
        items(&canvas.finish()),
        [Item::Scope(boxed, vec![Item::Graft(namespace)])]
    );
    let blur = crate::constant!(NonNegativeDp::try_new(Dp::new(4.0)));
    let mut canvas = Canvas::default();
    canvas.open_scope(ScopeKind::effect(BOUNDS, blur, None));
    let failed = canvas.graft(boxed, namespace);
    assert_eq!(failed, Err(FrameError::GraftInLayer(namespace)));
    let grafting = Painted::Scope {
        values: boxed,
        items: vec![Painted::Graft(namespace)],
    };
    assert_eq!(
        canvas.nest(grafting),
        Err(FrameError::GraftInLayer(namespace))
    );
    canvas.close();
    let painted = items(&canvas.finish());
    assert!(matches!(&painted[..], [Item::Leaf(_)]));
}
