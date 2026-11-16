// SPDX-License-Identifier: GPL-3.0-only
pub(crate) fn install_default_logger() {
    let installed = env_logger::Builder::new()
        .filter_module("lavashell", log::LevelFilter::Warn)
        .parse_default_env()
        .try_init();
    if let Err(error) = installed {
        log::debug!("the logger of the application receives the logs of Lavashell: {error}");
    }
}
