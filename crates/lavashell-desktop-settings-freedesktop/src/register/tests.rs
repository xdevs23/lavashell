// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use lavashell_actor::Outlet;
use lavashell_desktop::build_services;

#[test]
fn register_adds_one_source_that_the_settings_service_watches() {
    let mut store = Store::default();
    lavashell_desktop_settings::register::register(&mut store);
    register(&mut store);
    let view = Outlet::new(|_| Ok(()));
    let watches = (build_services(&store).into_iter())
        .flat_map(|service| service.into_watches(&view))
        .count();
    assert_eq!(watches, 1);
}
