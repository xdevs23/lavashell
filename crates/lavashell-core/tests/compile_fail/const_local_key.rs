// SPDX-License-Identifier: GPL-3.0-only
use lavashell_core::{local::Local, ui::Ui};

const ACCENT: Local<u32> = Local::new(|| 0x3366ff);

fn accent(ui: &Ui) -> u32 {
    ui.local(&ACCENT)
}

fn main() {}
