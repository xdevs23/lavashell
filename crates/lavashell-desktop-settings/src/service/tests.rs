// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::register::register;
use crate::{SettingsReporter, add_source};
use futures_lite::future;
use lavashell_actor::{Closed, mailbox};
use lavashell_core::platform::ColorScheme;
use lavashell_core::preferences::Appearance;
use lavashell_desktop::build_services;

struct ReportsDarkMode;

impl DesktopSettingSource for ReportsDarkMode {
    fn watch(self: Arc<Self>, report: SettingsReporter) -> ServiceWatch {
        Box::pin(async move {
            report(DesktopSetting::Appearance(Appearance {
                color_scheme: Some(ColorScheme::Dark),
                ..Appearance::default()
            }));
        })
    }
}

#[test]
fn a_source_added_after_the_settings_register_reports_to_the_view_as_a_settings_event()
-> Result<(), Closed> {
    let mut store = Store::default();
    register(&mut store);
    add_source(&mut store, Arc::new(ReportsDarkMode));
    let (sender, view) = mailbox::<PlatformEvent>();
    let outlet = sender.outlet(|event| event);
    for watch in
        (build_services(&store).into_iter()).flat_map(|service| service.into_watches(&outlet))
    {
        future::block_on(watch);
    }
    assert!(matches!(
        view.take_arrived()?[..],
        [PlatformEvent::Settings(SettingsEvent::Appearance(
            Appearance {
                color_scheme: Some(ColorScheme::Dark),
                ..
            }
        ))]
    ));
    Ok(())
}
