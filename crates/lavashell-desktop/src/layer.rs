// SPDX-License-Identifier: GPL-3.0-only
use crate::services::DesktopService;
use lavashell_store::Store;

type ServiceConstructor = fn(&Store) -> Box<dyn DesktopService>;

#[derive(Default)]
struct DesktopLayer {
    services: Vec<ServiceConstructor>,
    layers: Store,
}

pub fn plug_in(store: &mut Store, construct: ServiceConstructor) {
    store.layer(|desktop: &mut DesktopLayer| desktop.services.push(construct));
}

pub fn register_in_layers(store: &mut Store, register: impl FnOnce(&mut Store)) {
    store.layer(|desktop: &mut DesktopLayer| register(&mut desktop.layers));
}

#[must_use]
pub fn build_services(registered: &Store) -> Vec<Box<dyn DesktopService>> {
    (registered.get::<DesktopLayer>().into_iter())
        .flat_map(|desktop| {
            desktop
                .services
                .iter()
                .map(|construct| construct(&desktop.layers))
        })
        .collect()
}
