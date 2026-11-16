// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use rand::SeedableRng;

#[test]
fn recent_history_records_real_selections_in_order() {
    let mut model = Model::default();
    assert!(model.recent.is_empty());
    let (first, second) = (TrackId::at(2), TrackId::at(4));
    model.select(first);
    model.select(second);
    model.select(first);
    model.section = Section::Recent;
    assert_eq!(model.visible(), vec![first, second]);
}

#[test]
fn shuffle_varies_steps_without_repeats_or_leaving_the_album() {
    let album = AlbumId::all().next();
    let mut model = Model {
        shuffle: true,
        album,
        shuffle_order: SmallRng::seed_from_u64(42),
        ..Model::default()
    };
    let mut visited = BTreeSet::new();
    let mut steps = BTreeSet::new();
    for _ in 0..32 {
        let previous = model.current;
        model.next();
        assert_ne!(model.current, previous);
        assert_eq!(Some(model.current.track().album), album);
        visited.extend([model.current]);
        steps.extend([(model.current.number() + 4 - previous.number()) % 4]);
    }
    assert_eq!(visited.len(), 4);
    assert!(steps.len() > 1);
}

#[test]
fn skipping_from_outside_the_selected_album_enters_it_at_either_end() {
    let outside = Model::default().current.track().album;
    let album = AlbumId::all().find(|id| *id != outside);
    let queue: Vec<_> = TrackId::all()
        .filter(|id| Some(id.track().album) == album)
        .collect();
    let mut forward = Model {
        album,
        ..Model::default()
    };
    forward.next();
    assert_eq!(Some(forward.current), queue.first().copied());
    let mut backward = Model {
        album,
        ..Model::default()
    };
    backward.previous();
    assert_eq!(Some(backward.current), queue.last().copied());
}
