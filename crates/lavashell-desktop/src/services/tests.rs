// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::{plug_in, register_in_layers};
use lavashell_actor::{Closed, mailbox};
use lavashell_core::platform::ColorScheme;
use lavashell_core::preferences::Appearance;
use lavashell_platform::SettingsEvent;
use lavashell_store::Store;
use std::{future, panic};

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
    let arrived = DesktopServices::set_up(&registered).watch_while(outlet, || view.take())??;
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
    let arrived = DesktopServices(vec![Box::new(PanicsAfterReporting)])
        .watch_while(outlet, || view.take())??;
    assert_eq!(arrived.len(), 1);
    Ok(())
}

struct WatchesForever;

impl DesktopService for WatchesForever {
    fn into_watches(self: Box<Self>, _view: &Outlet<PlatformEvent>) -> Vec<ServiceWatch> {
        vec![Box::pin(future::pending())]
    }
}

struct PanicsWhileStarting;

impl DesktopService for PanicsWhileStarting {
    fn into_watches(self: Box<Self>, _view: &Outlet<PlatformEvent>) -> Vec<ServiceWatch> {
        panic::resume_unwind(Box::new("the service panics while its watches start"))
    }
}

fn watch_service_while_serve_returns_at_once(
    service: impl DesktopService + 'static,
) -> Result<(), ServicesStartError> {
    DesktopServices(vec![Box::new(service)]).watch_while(Outlet::new(|_| Ok(())), || ())
}

#[test]
fn watch_while_returns_once_serve_returns_while_a_service_is_still_watching() {
    assert!(matches!(
        watch_service_while_serve_returns_at_once(WatchesForever),
        Ok(())
    ));
}

#[test]
fn watch_while_fails_when_a_service_panics_while_its_watches_start() {
    assert!(matches!(
        watch_service_while_serve_returns_at_once(PanicsWhileStarting),
        Err(ServicesStartError::Panicked {
            value: PanicValue::Text(text)
        }) if text == "the service panics while its watches start"
    ));
}

#[test]
fn set_up_does_not_build_a_service_from_a_store_without_the_desktop_layer() {
    assert!(DesktopServices::set_up(&Store::default()).0.is_empty());
}
