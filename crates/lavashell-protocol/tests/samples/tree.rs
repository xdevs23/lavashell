// SPDX-License-Identifier: GPL-3.0-only
use super::scene::affine;
use super::shape::rounded;
use super::{Check, Samples, TestError, TestResult, motion};
use lavashell_core::geometry::Color;
use lavashell_core::scene::DrawCommand;
use lavashell_core::scene::tree::{
    Change, Content, Group, Node, Packet, SceneNodeId, Slot, SlotChange,
};
use std::sync::Arc;

fn packet() -> Packet {
    let fill = DrawCommand::Fill {
        shape: rounded().into(),
        brush: Color::WHITE.with_alpha(0.14).into(),
    };
    Packet::new(Arc::from([DrawCommand::Save, fill, DrawCommand::Restore]))
}

fn group() -> Group {
    Group {
        transform: Slot::Value(affine()),
        opacity: Slot::Value(0.75),
        clip: Some(Slot::Value(rounded())),
    }
}

pub(super) fn changes() -> Samples<Change> {
    every_variant! {
        Change::Insert { .. } => Change::Insert {
            id: SceneNodeId(1),
            parent: Some(SceneNodeId(2)),
            index: 3,
            node: Node::Group(group()),
        }, [
            0, 1, 1, 2, 3, 0, 0, 0, 0, 0, 0, 0, 0, 248, 63, 0, 0, 0, 0, 0, 0, 208, 63, 0, 0, 0, 0,
            0, 0, 224, 191, 0, 0, 0, 0, 0, 0, 0, 64, 0, 0, 0, 0, 0, 0, 8, 64, 0, 0, 0, 0, 0, 0, 28,
            64, 0, 0, 0, 0, 0, 0, 0, 232, 63, 1, 0, 0, 0, 0, 0, 0, 0, 48, 64, 0, 0, 0, 0, 0, 0, 56,
            64, 0, 0, 0, 0, 0, 0, 110, 64, 0, 0, 0, 0, 0, 128, 100, 64, 0, 0, 0, 0, 0, 0, 0, 64, 0,
            0, 0, 0, 0, 0, 16, 64, 0, 0, 0, 0, 0, 0, 24, 64, 0, 0, 0, 0, 0, 0, 32, 64,
        ];
        Change::Remove { .. } => Change::Remove { id: SceneNodeId(4) }, [1, 4];
        Change::Move { .. } => Change::Move { id: SceneNodeId(5), parent: None, index: 6 },
            [2, 5, 0, 6];
        Change::SetContent { .. } => Change::SetContent {
            id: SceneNodeId(7),
            content: Content::Packet(packet()),
        }, [
            3, 7, 0, 3, 0, 10, 0, 0, 0, 0, 0, 0, 0, 48, 64, 0, 0, 0, 0, 0, 0, 56, 64, 0, 0, 0, 0, 0,
            0, 110, 64, 0, 0, 0, 0, 0, 128, 100, 64, 0, 0, 0, 0, 0, 0, 0, 64, 0, 0, 0, 0, 0, 0, 16,
            64, 0, 0, 0, 0, 0, 0, 24, 64, 0, 0, 0, 0, 0, 0, 32, 64, 0, 0, 0, 0, 0, 0, 0, 0, 240, 63,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 236, 81, 184, 30, 133, 235, 193, 63, 1,
        ];
        Change::SetSlot { .. } => Change::SetSlot {
            id: SceneNodeId(8),
            slot: SlotChange::Opacity(Slot::Value(0.5)),
        }, [
            4, 8, 1, 0, 0, 0, 0, 0, 0, 0, 224, 63,
        ];
    }
}

fn nodes() -> Samples<Node> {
    every_variant! {
        Node::Group(_) => Node::Group(group()), [
            0, 0, 0, 0, 0, 0, 0, 0, 248, 63, 0, 0, 0, 0, 0, 0, 208, 63, 0, 0, 0, 0, 0, 0, 224, 191,
            0, 0, 0, 0, 0, 0, 0, 64, 0, 0, 0, 0, 0, 0, 8, 64, 0, 0, 0, 0, 0, 0, 28, 64, 0, 0, 0, 0,
            0, 0, 0, 232, 63, 1, 0, 0, 0, 0, 0, 0, 0, 48, 64, 0, 0, 0, 0, 0, 0, 56, 64, 0, 0, 0, 0,
            0, 0, 110, 64, 0, 0, 0, 0, 0, 128, 100, 64, 0, 0, 0, 0, 0, 0, 0, 64, 0, 0, 0, 0, 0, 0,
            16, 64, 0, 0, 0, 0, 0, 0, 24, 64, 0, 0, 0, 0, 0, 0, 32, 64,
        ];
        Node::Leaf(_) => Node::Leaf(Content::Packet(packet())), [
            1, 0, 3, 0, 10, 0, 0, 0, 0, 0, 0, 0, 48, 64, 0, 0, 0, 0, 0, 0, 56, 64, 0, 0, 0, 0, 0, 0,
            110, 64, 0, 0, 0, 0, 0, 128, 100, 64, 0, 0, 0, 0, 0, 0, 0, 64, 0, 0, 0, 0, 0, 0, 16, 64,
            0, 0, 0, 0, 0, 0, 24, 64, 0, 0, 0, 0, 0, 0, 32, 64, 0, 0, 0, 0, 0, 0, 0, 0, 240, 63, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 236, 81, 184, 30, 133, 235, 193, 63, 1,
        ];
    }
}

fn contents() -> Samples<Content> {
    every_variant! {
        #[process_local(Content::Graft(_))]
        Content::Packet(_) => Content::Packet(packet()), [
            0, 3, 0, 10, 0, 0, 0, 0, 0, 0, 0, 48, 64, 0, 0, 0, 0, 0, 0, 56, 64, 0, 0, 0, 0, 0, 0,
            110, 64, 0, 0, 0, 0, 0, 128, 100, 64, 0, 0, 0, 0, 0, 0, 0, 64, 0, 0, 0, 0, 0, 0, 16, 64,
            0, 0, 0, 0, 0, 0, 24, 64, 0, 0, 0, 0, 0, 0, 32, 64, 0, 0, 0, 0, 0, 0, 0, 0, 240, 63, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 236, 81, 184, 30, 133, 235, 193, 63, 1,
        ];
    }
}

fn slot_changes() -> Result<Samples<SlotChange>, TestError> {
    let opacity = Slot::Motion(Box::new(motion::opacity()?));
    let clip = Slot::Motion(Box::new(motion::clip()?));
    Ok(every_variant! {
        SlotChange::Transform(_) => SlotChange::Transform(Slot::Value(affine())), [
            0, 0, 0, 0, 0, 0, 0, 0, 248, 63, 0, 0, 0, 0, 0, 0, 208, 63, 0, 0, 0, 0, 0, 0, 224, 191,
            0, 0, 0, 0, 0, 0, 0, 64, 0, 0, 0, 0, 0, 0, 8, 64, 0, 0, 0, 0, 0, 0, 28, 64,
        ];
        SlotChange::Opacity(_) => SlotChange::Opacity(opacity), [
            1, 1, 0, 0, 0, 0, 0, 0, 240, 63, 0, 0, 0, 0, 0, 0, 0, 208, 63, 0, 0, 0, 0, 0, 0, 240,
            63, 0, 0, 0, 0, 0, 0, 216, 63, 1, 0, 128, 229, 154, 119, 0, 128, 225, 235, 23, 1, 1, 0,
            0, 0, 0, 0, 0, 224, 63, 1, 128, 202, 181, 238, 1, 1, 1, 128, 206, 228, 205, 2, 0, 128,
            180, 137, 19, 0, 0, 0, 0, 0, 0, 248, 63,
        ];
        SlotChange::Clip(_) => SlotChange::Clip(Some(clip)), [
            2, 1, 1, 0, 0, 0, 0, 0, 0, 16, 64, 0, 0, 0, 0, 0, 0, 32, 64, 0, 0, 0, 0, 0, 0, 94, 64,
            0, 0, 0, 0, 0, 0, 78, 64, 0, 0, 0, 0, 0, 0, 36, 64, 0, 0, 0, 0, 0, 0, 36, 64, 0, 0, 0,
            0, 0, 0, 36, 64, 0, 0, 0, 0, 0, 0, 36, 64, 0, 0, 0, 0, 0, 0, 0, 48, 64, 0, 0, 0, 0, 0,
            0, 56, 64, 0, 0, 0, 0, 0, 0, 110, 64, 0, 0, 0, 0, 0, 128, 100, 64, 0, 0, 0, 0, 0, 0, 0,
            64, 0, 0, 0, 0, 0, 0, 16, 64, 0, 0, 0, 0, 0, 0, 24, 64, 0, 0, 0, 0, 0, 0, 32, 64, 0, 0,
            0, 0, 0, 0, 16, 64, 0, 0, 0, 0, 0, 0, 32, 64, 0, 0, 0, 0, 0, 0, 94, 64, 0, 0, 0, 0, 0,
            0, 78, 64, 0, 0, 0, 0, 0, 0, 36, 64, 0, 0, 0, 0, 0, 0, 36, 64, 0, 0, 0, 0, 0, 0, 36, 64,
            0, 0, 0, 0, 0, 0, 36, 64, 0, 0, 0, 0, 0, 0, 24, 192, 0, 0, 0, 0, 0, 0, 32, 192, 0, 0, 0,
            0, 0, 0, 78, 192, 0, 0, 0, 0, 0, 0, 74, 192, 0, 0, 0, 0, 0, 0, 16, 64, 0, 0, 0, 0, 0, 0,
            8, 64, 0, 0, 0, 0, 0, 0, 0, 64, 0, 0, 0, 0, 0, 0, 240, 63, 2, 94, 56, 85, 41, 122, 106,
            47, 64, 154, 153, 153, 153, 153, 153, 233, 63, 0, 0, 0, 0, 0, 0, 224, 63, 1, 128, 202,
            181, 238, 1, 1, 1, 128, 206, 228, 205, 2, 0, 128, 180, 137, 19, 0, 0, 0, 0, 0, 0, 248,
            63,
        ];
    })
}

fn slots() -> Result<Samples<Slot<f64>>, TestError> {
    Ok(every_variant! {
        Slot::Value(_) => Slot::Value(0.5), [
            0, 0, 0, 0, 0, 0, 0, 224, 63,
        ];
        Slot::Motion(_) => Slot::Motion(Box::new(motion::opacity()?)), [
            1, 0, 0, 0, 0, 0, 0, 240, 63, 0, 0, 0, 0, 0, 0, 0, 208, 63, 0, 0, 0, 0, 0, 0, 240, 63,
            0, 0, 0, 0, 0, 0, 216, 63, 1, 0, 128, 229, 154, 119, 0, 128, 225, 235, 23, 1, 1, 0, 0,
            0, 0, 0, 0, 224, 63, 1, 128, 202, 181, 238, 1, 1, 1, 128, 206, 228, 205, 2, 0, 128, 180,
            137, 19, 0, 0, 0, 0, 0, 0, 248, 63,
        ];
    })
}

pub(super) fn check(check: &mut impl Check) -> TestResult {
    check.samples(&changes())?;
    check.samples(&nodes())?;
    check.samples(&contents())?;
    check.samples(&slot_changes()?)?;
    check.samples(&slots()?)
}
