// SPDX-License-Identifier: GPL-3.0-only
use super::*;

#[test]
fn dependency_fonts_decode_and_resolve_variable_weights() -> Result<(), Error> {
    let mut painter = Painter::new(
        &fonts()?,
        lavashell::renderer::UniformRange::VULKAN_REQUIRED,
    )
    .map_err(Error::Painter)?;
    let shaper = painter.shaper();
    for alias in [BODY, DISPLAY, MONO] {
        let style = TextStyle::new(alias, 32.sp(), Color::WHITE);
        let bold = style.clone().weight(FontWeight::BOLD).resolve_unscaled();
        let style = style.resolve_unscaled();
        let normal = shaper.measure("Variable typography", &style, 1024.dp());
        let bold = shaper.measure("Variable typography", &bold, 1024.dp());
        assert!(normal.width > Dp::ZERO && normal.height > Dp::ZERO);
        if alias == MONO {
            assert_eq!(
                shaper.measure("iiii", &style, 1024.dp()).width,
                shaper.measure("WWWW", &style, 1024.dp()).width
            );
            assert_eq!(
                normal.width, bold.width,
                "monospaced advances must remain constant"
            );
        } else {
            assert_ne!(
                normal.width, bold.width,
                "variable weight must affect {alias} metrics"
            );
        }
    }
    Ok(())
}

#[test]
fn a_run_exits_with_failure_only_when_it_failed() {
    assert_eq!(
        exit_code(Ok::<(), Error>(())),
        std::process::ExitCode::SUCCESS
    );
    assert_eq!(
        exit_code(Err(Error::FontDirectory)),
        std::process::ExitCode::FAILURE
    );
}
