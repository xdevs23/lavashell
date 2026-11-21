// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use lavashell_actor::{Closed, mailbox};
use lavashell_core::platform::ColorScheme;
use lavashell_core::preferences::Appearance;
use lavashell_desktop_settings::{SettingsReporter, SettingsWatch};

#[derive(Debug, thiserror::Error)]
enum TestError {
    #[error(transparent)]
    Start(#[from] SettingsStartError),
    #[error(transparent)]
    View(#[from] Closed),
}

struct EndsAtOnce;

impl DesktopSettingSource for EndsAtOnce {
    fn watch(self: Arc<Self>, _report: SettingsReporter) -> SettingsWatch {
        Box::pin(async {})
    }
}

struct ReportsDarkMode;

impl DesktopSettingSource for ReportsDarkMode {
    fn watch(self: Arc<Self>, report: SettingsReporter) -> SettingsWatch {
        Box::pin(async move {
            report(DesktopSetting::Appearance(Appearance {
                color_scheme: Some(ColorScheme::Dark),
                ..Appearance::default()
            }));
        })
    }
}

#[test]
fn a_source_reports_to_the_view_after_another_source_ended() -> Result<(), TestError> {
    let (sender, view) = mailbox::<PlatformEvent>();
    let outlet = sender.outlet(|event| event);
    drop(sender);
    let sources: Vec<Arc<dyn DesktopSettingSource>> =
        vec![Arc::new(EndsAtOnce), Arc::new(ReportsDarkMode)];
    let arrived = watch_settings_while(sources, outlet, || view.take())??;
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
