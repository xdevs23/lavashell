// SPDX-License-Identifier: GPL-3.0-only
use crate::error::RoundTripError;
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

const INSTRUMENTATION_STATUS_PREFIXES: [&str; 3] = [
    "INSTRUMENTATION_RESULT:",
    "INSTRUMENTATION_FAILED:",
    "INSTRUMENTATION_CODE:",
];

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoundTripResult {
    pub step: String,
    #[serde(deserialize_with = "Option::deserialize")]
    pub matches: Option<bool>,
    pub evidence: Box<RawValue>,
}

impl RoundTripResult {
    #[must_use]
    pub fn failed(&self) -> bool {
        self.matches == Some(false)
    }
}

pub fn read_results(output: &str) -> Result<Vec<RoundTripResult>, RoundTripError> {
    let lines: Vec<&str> = output
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    if lines.iter().any(|line| {
        INSTRUMENTATION_STATUS_PREFIXES
            .iter()
            .any(|prefix| line.starts_with(prefix))
    }) {
        return Err(RoundTripError::InstrumentationCrashed {
            output: output.to_owned(),
        });
    }
    let results = lines
        .into_iter()
        .map(|line| {
            serde_json::from_str(line).map_err(|source| RoundTripError::ResultMalformed {
                line: line.to_owned(),
                source,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    if results.is_empty() {
        return Err(RoundTripError::RoundTripsMissing);
    }
    Ok(results)
}

pub fn json_lines(results: &[RoundTripResult]) -> Result<String, RoundTripError> {
    results
        .iter()
        .map(|result| serde_json::to_string(result).map(|line| line + "\n"))
        .collect::<Result<String, _>>()
        .map_err(RoundTripError::EncodeResults)
}

#[cfg(test)]
mod tests;
