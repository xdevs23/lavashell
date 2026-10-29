// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::animation::{MotionSpec, Playing, Program, Timing, Vector};
use crate::geometry::{Axis, Color, PixelSize};
use crate::image::{Image, StreamId, StreamImage};
use crate::test_error::TestResult;
use crate::units::{Dp, DpExt};
use std::time::Duration;

fn fill() -> DrawCommand {
    DrawCommand::Fill {
        shape: Rect::new(0.dp(), 0.dp(), 10.dp(), 10.dp()).into(),
        brush: Color::WHITE.into(),
    }
}

fn read<T: serde::de::DeserializeOwned>(
    value: &impl serde::Serialize,
) -> Result<T, postcard::Error> {
    postcard::from_bytes(&postcard::to_allocvec(value)?)
}

#[test]
fn a_packet_reads_back_with_its_commands_and_their_footprint() -> TestResult {
    let written = Packet::new(Arc::from([fill(), DrawCommand::Opacity(0.5)]));
    let packet: Packet = read(&written)?;
    assert_eq!(packet.commands(), written.commands());
    assert_eq!(packet.footprint(), written.footprint());
    assert!(packet.generation() > written.generation());
    Ok(())
}

#[test]
fn an_update_reads_back_as_the_same_changes() -> TestResult {
    let program = Program::target(
        Vector([0.0]),
        Vector([40.0]),
        Vector([0.0]),
        MotionSpec::Instant,
        0.01,
    )?;
    let timing = Timing {
        start: Duration::from_millis(100),
        paused: None,
        offset: Duration::ZERO,
        rate: 1.0,
    };
    let scroll = TransformMotion::Clamp {
        offset: Playing::new(40.dp(), program, timing)?,
        clamp: Clamp {
            min: Dp::ZERO,
            max: 30.dp(),
            axis: Axis::Vertical,
            scale: -1.0,
        },
    };
    let group = Group {
        transform: Slot::Motion(Box::new(scroll)),
        opacity: Slot::Value(0.5),
        clip: None,
    };
    let leaf = Node::Leaf(Content::Packet(Packet::new(Arc::from([fill()]))));
    let update = SceneUpdate {
        changes: vec![
            Change::Insert {
                id: SceneNodeId(1),
                parent: None,
                index: 0,
                node: Node::Group(Group::default()),
            },
            Change::Insert {
                id: SceneNodeId(2),
                parent: Some(SceneNodeId(1)),
                index: 0,
                node: leaf,
            },
            Change::SetSlot {
                id: SceneNodeId(1),
                slot: SlotChange::Transform(group.transform.clone()),
            },
            Change::SetSlot {
                id: SceneNodeId(1),
                slot: SlotChange::Opacity(group.opacity.clone()),
            },
        ],
    };
    let mut tree = SceneTree::default();
    tree.show(read(&update)?)?;
    let key = |node| NodeKey::new(SceneTree::ROOT, SceneNodeId(node));
    assert!(tree.group(key(1)) == Some(&group));
    assert_eq!(
        tree.packet(key(2)).map(Packet::commands),
        Some(&Arc::<[DrawCommand]>::from([fill()]))
    );
    Ok(())
}

#[test]
fn a_boxed_motion_is_written_as_its_variant_and_motion() -> TestResult {
    let program = Program::target(
        Vector([0.0]),
        Vector([1.0]),
        Vector([0.0]),
        MotionSpec::Instant,
        0.01,
    )?;
    let timing = Timing {
        start: Duration::from_millis(100),
        paused: None,
        offset: Duration::ZERO,
        rate: 1.0,
    };
    let motion = Playing::new(1.0, program, timing)?;
    let slot = Slot::Motion(Box::new(motion.clone()));
    let mut plain = vec![1];
    plain.extend(postcard::to_allocvec(&motion)?);
    let bytes = postcard::to_allocvec(&slot)?;
    assert_eq!(bytes, plain);
    assert!(postcard::from_bytes::<Slot<f64>>(&bytes)? == slot);
    Ok(())
}

#[test]
fn a_graft_is_not_written() {
    let mut tree = SceneTree::default();
    let graft = Change::SetContent {
        id: SceneNodeId(1),
        content: Content::Graft(tree.create_namespace()),
    };
    let update = SceneUpdate {
        changes: vec![graft],
    };
    assert!(postcard::to_allocvec(&update).is_err());
}

#[test]
fn a_scene_with_an_image_or_a_stream_frame_is_not_written() {
    let bounds = Rect::new(0.dp(), 0.dp(), 4.dp(), 4.dp());
    let image = DrawCommand::Image {
        image: Image::encoded([0_u8; 4], 1, 1),
        destination: bounds,
        frame: bounds,
    };
    let frame = DrawCommand::StreamImage {
        image: StreamImage {
            stream: StreamId::next(),
            generation: 1,
            size: PixelSize::new(4, 4),
        },
        destination: bounds,
        frame: bounds,
    };
    for command in [image, frame] {
        let packet = Packet::new(Arc::from([fill(), command]));
        assert!(postcard::to_allocvec(&packet).is_err());
    }
}
