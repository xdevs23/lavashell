// SPDX-License-Identifier: GPL-3.0-only
use crate::{Error, write_screenshot};
use lavashell_example_support::ExampleArguments;
use lavashell_materials_example::{APP_ID, OutputArguments, build_app, model::Run};
use std::path::Path;

pub fn screenshot(
    example: &ExampleArguments,
    output: OutputArguments,
    path: &Path,
) -> Result<(), Error> {
    let app = build_app(example, output, Run::Screenshot)?;
    write_screenshot(app, APP_ID, path)
}
