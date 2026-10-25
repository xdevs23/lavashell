// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use rand::SeedableRng;

#[test]
fn recent_history_records_real_selections_in_order() {
    let mut model = Model::default();
    assert!(model.recent.is_empty());
    model.select(2);
    model.select(4);
    model.select(2);
    model.section = Section::Recent;
    assert_eq!(model.visible(), vec![2, 4]);
}

#[test]
fn shuffle_varies_steps_without_repeats_or_leaving_the_album() {
    let mut model = Model {
        shuffle: true,
        album: Some(0),
        shuffle_order: SmallRng::seed_from_u64(42),
        ..Model::default()
    };
    let mut visited = BTreeSet::new();
    let mut steps = BTreeSet::new();
    for _ in 0..32 {
        let previous = model.current;
        model.next();
        assert_ne!(model.current, previous);
        assert_eq!(TRACKS[model.current].album, 0);
        visited.extend([model.current]);
        steps.extend([(model.current + 4 - previous) % 4]);
    }
    assert_eq!(visited.len(), 4);
    assert!(steps.len() > 1);
}
