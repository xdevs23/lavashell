// SPDX-License-Identifier: GPL-3.0-only
//! Decodes the bytes as messages between a host and a guest and runs them through a host: the
//! guest namespace is grafted into the tree of the host, each scene update is applied until one
//! fails, and the tree is presented at each frame request, in the viewport of the last resize.
#![forbid(unsafe_code)]
#![no_main]

use lavashell_core::geometry::Viewport;
use lavashell_core::scene::tree::{
    Change, Content, DamageTracker, Node, SceneNodeId, SceneTree, SceneUpdate,
};
use lavashell_protocol::{Account, Decoded, GuestMessage, HostMessage, decode};
use libfuzzer_sys::fuzz_target;
use serde::Deserialize;
use std::time::Duration;

#[derive(Deserialize)]
enum Sent {
    Host(HostMessage),
    Guest(GuestMessage),
}

fuzz_target!(|bytes: &[u8]| {
    let Ok(Decoded {
        value: conversation,
        charge: _charged_to_an_unlimited_account,
    }) = decode::<Vec<Sent>>(bytes, &mut Account::new(usize::MAX))
    else {
        return;
    };
    let mut tree = SceneTree::default();
    let guest = tree.create_namespace();
    let graft = SceneUpdate {
        changes: vec![Change::Insert {
            id: SceneNodeId(1),
            parent: None,
            index: 0,
            node: Node::Leaf(Content::Graft(guest)),
        }],
    };
    assert!(tree.show(graft).is_ok());
    let (mut damage, mut viewport) = (DamageTracker::default(), None);
    for sent in conversation {
        match sent {
            Sent::Host(HostMessage::Resize { size, density, .. }) => {
                viewport = Some(Viewport::new(size.get(), density));
            }
            Sent::Host(HostMessage::Frame { presentation, .. }) => {
                if let Some(viewport) = viewport {
                    present(&mut damage, &tree, presentation, viewport);
                    if let Some(next) = tree.next_change(presentation) {
                        present(&mut damage, &tree, next, viewport);
                    }
                }
            }
            Sent::Guest(GuestMessage::Scene { update, .. }) => {
                if tree.apply(guest, update).is_err() {
                    return;
                }
            }
            Sent::Host(_) | Sent::Guest(_) => {}
        }
    }
});

fn present(damage: &mut DamageTracker, tree: &SceneTree, now: Duration, viewport: Viewport) {
    let _repainted_by_a_renderer = damage.damage(tree.at(now), viewport);
}
