// SPDX-License-Identifier: GPL-3.0-only
use crate::checks::CrateRule;
use crate::findings::{Finding, Rule};
use crate::identifiers::{identifiers, name_words};
use crate::modules::Crate;
use lavashell_enum_values::enum_with_values;
use quote::ToTokens;
use std::fmt;

enum_with_values! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum DeviceWord {
        Click,
        Tap,
    }

    const ALL;
}

impl DeviceWord {
    fn name(self) -> &'static str {
        match self {
            Self::Click => "click",
            Self::Tap => "tap",
        }
    }

    fn inflections(self) -> &'static [&'static str] {
        match self {
            Self::Click => &["clicks", "clicked", "clicking"],
            Self::Tap => &["taps", "tapped", "tapping"],
        }
    }

    fn is_spelled_as(self, word: &str) -> bool {
        word == self.name() || self.inflections().contains(&word)
    }
}

impl fmt::Display for DeviceWord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

pub const CORE_DEVICE_WORD_RULES: CrateRule = device_word_rule("crates/lavashell-core/src/lib.rs");

pub const PLATFORM_DEVICE_WORD_RULES: CrateRule =
    device_word_rule("crates/lavashell-platform/src/lib.rs");

const fn device_word_rule(root: &'static str) -> CrateRule {
    CrateRule {
        root,
        findings: |crate_files| Ok(device_word_findings(crate_files)),
    }
}

fn device_word_findings(crate_files: &Crate<'_>) -> Vec<Finding> {
    let mut findings = Vec::new();
    for source in crate_files.sources() {
        for name in identifiers(source.syntax.to_token_stream()) {
            let text = name.to_string();
            if let Some(word) = device_word_in(&text) {
                findings.push(Finding {
                    path: source.path.clone(),
                    line: Some(name.span().start().line),
                    rule: Rule::DeviceWord { name: text, word },
                });
            }
        }
    }
    findings
}

fn device_word_in(name: &str) -> Option<DeviceWord> {
    (name_words(name).iter()).find_map(|word| {
        DeviceWord::ALL
            .into_iter()
            .find(|device| device.is_spelled_as(word))
    })
}
