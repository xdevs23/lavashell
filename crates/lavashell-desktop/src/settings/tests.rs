// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use lavashell_actor::{Closed, mailbox};
use lavashell_core::platform::ColorScheme;
use lavashell_core::preferences::Appearance;
use lavashell_desktop_settings::{SettingsReporter, SettingsWatch};
use std::thread;

struct EndsAtOnce;

impl DesktopSettingSource for EndsAtOnce {
    fn watch(self: Box<Self>, _report: SettingsReporter) -> SettingsWatch {
        Box::pin(async {})
    }
}

struct ReportsDarkMode;

impl DesktopSettingSource for ReportsDarkMode {
    fn watch(self: Box<Self>, report: SettingsReporter) -> SettingsWatch {
        Box::pin(async move {
            report(DesktopSetting::Appearance(Appearance {
                color_scheme: Some(ColorScheme::Dark),
                ..Appearance::default()
            }));
        })
    }
}

#[test]
fn a_source_reports_to_the_view_after_another_source_ended() -> Result<(), Closed> {
    let (sender, view) = mailbox::<PlatformEvent>();
    let outlet = sender.outlet(|event| event);
    let (stop, stopped) = calloop::channel::channel();
    let sources: Vec<Box<dyn DesktopSettingSource>> =
        vec![Box::new(EndsAtOnce), Box::new(ReportsDarkMode)];
    let watch = thread::spawn(move || watch_settings(sources, &outlet, stopped));
    let arrived = view.take()?;
    drop(stop);
    assert!(matches!(watch.join(), Ok(Ok(()))));
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
