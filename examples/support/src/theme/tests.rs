// SPDX-License-Identifier: GPL-3.0-only
use super::*;

#[test]
fn the_system_choice_follows_the_system_and_the_others_ignore_it() {
    for system in ColorScheme::ALL {
        assert_eq!(ThemeChoice::System.scheme(system), system);
        assert_eq!(ThemeChoice::Light.scheme(system), ColorScheme::Light);
        assert_eq!(ThemeChoice::Dark.scheme(system), ColorScheme::Dark);
    }
}
