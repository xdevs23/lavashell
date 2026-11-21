// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::layer::build_services;
use crate::{plug_in, register_in_layers};
use lavashell_actor::{Closed, mailbox};
use lavashell_core::platform::ColorScheme;
use lavashell_core::preferences::Appearance;
use lavashell_platform::SettingsEvent;
use lavashell_store::Store;

#[derive(Debug, thiserror::Error)]
enum TestError {
    #[error(transparent)]
    Start(#[from] ServicesStartError),
    #[error(transparent)]
    View(#[from] Closed),
}

struct EndsAtOnce;

impl EndsAtOnce {
    fn from_layers(_layers: &Store) -> Box<dyn DesktopService> {
        Box::new(Self)
    }
}

impl DesktopService for EndsAtOnce {
    fn into_watches(self: Box<Self>, _view: &Outlet<PlatformEvent>) -> Vec<ServiceWatch> {
        vec![Box::pin(async {})]
    }
}

#[derive(Default)]
struct AppearanceLayer {
    appearances: Vec<Appearance>,
}

struct ReportsAppearances {
    appearances: Vec<Appearance>,
}

impl ReportsAppearances {
    fn from_layers(layers: &Store) -> Box<dyn DesktopService> {
        Box::new(Self {
            appearances: (layers.get::<AppearanceLayer>().into_iter())
                .flat_map(|layer| layer.appearances.iter().copied())
                .collect(),
        })
    }
}

impl DesktopService for ReportsAppearances {
    fn into_watches(self: Box<Self>, view: &Outlet<PlatformEvent>) -> Vec<ServiceWatch> {
        (self.appearances.into_iter())
            .map(|appearance| -> ServiceWatch {
                let view = view.clone();
                Box::pin(async move {
                    let _ended_test_stops_reading =
                        view.push(SettingsEvent::Appearance(appearance).into());
                })
            })
            .collect()
    }
}

#[test]
fn a_service_registered_through_the_layers_reports_to_the_view_after_another_service_ended()
-> Result<(), TestError> {
    let mut registered = Store::default();
    plug_in(&mut registered, EndsAtOnce::from_layers);
    plug_in(&mut registered, ReportsAppearances::from_layers);
    register_in_layers(&mut registered, |layers| {
        layers.layer(|layer: &mut AppearanceLayer| {
            layer.appearances.push(Appearance {
                color_scheme: Some(ColorScheme::Dark),
                ..Appearance::default()
            });
        });
    });
    let (sender, view) = mailbox::<PlatformEvent>();
    let outlet = sender.outlet(|event| event);
    drop(sender);
    let arrived = watch_services_while(build_services(&registered), outlet, || view.take())??;
    assert!(matches!(
        arrived[..],
        [PlatformEvent::Settings(SettingsEvent::Appearance(
            Appearance {
                color_scheme: Some(ColorScheme::Dark),
                ..
            }
        ))]
    ));
    Ok(())
}

struct PanicsAfterReporting;

impl DesktopService for PanicsAfterReporting {
    fn into_watches(self: Box<Self>, view: &Outlet<PlatformEvent>) -> Vec<ServiceWatch> {
        let view = view.clone();
        vec![Box::pin(async move {
            let _ended_test_stops_reading =
                view.push(SettingsEvent::Appearance(Appearance::default()).into());
            std::panic::resume_unwind(Box::new("the service panics after it reported"))
        })]
    }
}

#[test]
fn a_service_that_panics_while_the_app_runs_leaves_the_result_of_serve_in_place()
-> Result<(), TestError> {
    let (sender, view) = mailbox::<PlatformEvent>();
    let outlet = sender.outlet(|event| event);
    drop(sender);
    let arrived =
        watch_services_while(vec![Box::new(PanicsAfterReporting)], outlet, || view.take())??;
    assert_eq!(arrived.len(), 1);
    Ok(())
}
