// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::scene::Affine;

const NODE: SceneNodeId = SceneNodeId(1);

fn overwrites(field: ChangeField) -> ChangeEffect {
    ChangeEffect::Overwrites(ChangeKey { id: NODE, field })
}

fn set_slot(slot: SlotChange) -> Change {
    Change::SetSlot { id: NODE, slot }
}

#[test]
fn a_packet_and_each_slot_overwrite_their_own_field_of_the_node() {
    let content = Content::Packet(Packet::new(Arc::from([])));
    let transform = SlotChange::Transform(Slot::Value(Affine::IDENTITY));
    assert_eq!(
        Change::SetContent { id: NODE, content }.effect(),
        overwrites(ChangeField::Content)
    );
    assert_eq!(
        set_slot(transform).effect(),
        overwrites(ChangeField::Transform)
    );
    assert_eq!(
        set_slot(SlotChange::Opacity(Slot::Value(0.5))).effect(),
        overwrites(ChangeField::Opacity)
    );
    assert_eq!(
        set_slot(SlotChange::Clip(None)).effect(),
        overwrites(ChangeField::Clip)
    );
}

#[test]
fn a_graft_fences_the_content_of_its_node() {
    let content = Content::Graft(SceneTree::ROOT);
    assert_eq!(
        Change::SetContent { id: NODE, content }.effect(),
        ChangeEffect::Fences(ChangeKey {
            id: NODE,
            field: ChangeField::Content,
        })
    );
}

#[test]
fn a_change_of_the_structure_builds_on_the_changes_before_it() {
    let changes = [
        test_changes::insert(1, None, 0, Node::Group(Group::default())),
        Change::Move {
            id: NODE,
            parent: None,
            index: 0,
        },
        Change::Remove { id: NODE },
    ];
    for change in changes {
        assert_eq!(change.effect(), ChangeEffect::BuildsOn);
    }
}
