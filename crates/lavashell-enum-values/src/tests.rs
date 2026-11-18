// SPDX-License-Identifier: GPL-3.0-only

mod declared {
    crate::enum_with_values! {
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
        pub(super) enum Season {
            Spring,
            #[default]
            Summer,
            Autumn,
        }

        pub(super) const ALL;
    }

    crate::enum_with_values! {
        #[derive(Clone, Copy)]
        pub(super) enum Ranked {
            Low = 3,
            High = 9,
        }

        pub(super) const ALL;
    }

    crate::enum_with_values! {
        #[derive(Debug, PartialEq, Eq)]
        pub(super) enum Single {
            Only
        }

        pub(super) const ALL;
    }
}

use declared::{Ranked, Season, Single};

#[test]
fn all_lists_every_variant_in_declaration_order() {
    assert_eq!(Season::ALL.map(|season| season as u8), [0, 1, 2]);
}

#[test]
fn variant_attributes_apply_to_the_enum() {
    assert_eq!(Season::default(), Season::Summer);
}

#[test]
fn discriminants_apply_to_the_enum() {
    assert_eq!(Ranked::ALL.map(|ranked| ranked as u8), [3, 9]);
}

#[test]
fn single_variant_enum_lists_its_variant() {
    assert_eq!(Single::ALL.map(|single| single as u8), [0]);
}
