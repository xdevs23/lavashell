// SPDX-License-Identifier: GPL-3.0-only
/// Installs a logger that writes the warnings and errors logged by Lavashell to standard error. An application
/// that installed a logger before receives those logs in its logger instead.
pub(super) fn install() {
    let installed = env_logger::Builder::new()
        .filter_module("lavashell", log::LevelFilter::Warn)
        .try_init();
    if let Err(error) = installed {
        log::debug!("the logger of the application receives the logs of Lavashell: {error}");
    }
}
