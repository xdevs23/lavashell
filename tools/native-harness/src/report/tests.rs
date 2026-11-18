// SPDX-License-Identifier: GPL-3.0-only
use super::Report;
use crate::CompositorRenderer;
use clap::ValueEnum;
use serde_json::json;

fn report_requesting(renderer: Option<CompositorRenderer>) -> Report {
    Report {
        flake: String::new(),
        all_wayland_apps: true,
        requested_renderer: renderer,
        examples: Vec::new(),
    }
}

#[test]
fn the_report_names_the_requested_renderer_as_the_console_does() -> Result<(), serde_json::Error> {
    for renderer in CompositorRenderer::value_variants().iter().copied() {
        let report = serde_json::to_value(report_requesting(Some(renderer)))?;
        assert_eq!(report["requested_renderer"], renderer.to_string());
    }
    Ok(())
}

#[test]
fn the_report_has_no_requested_renderer_when_the_compositor_chose() -> Result<(), serde_json::Error>
{
    let report = serde_json::to_value(report_requesting(None))?;
    assert_eq!(report["requested_renderer"], json!(null));
    Ok(())
}
