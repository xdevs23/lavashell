// SPDX-License-Identifier: GPL-3.0-only
use super::{json_lines, read_results};
use crate::error::RoundTripError;

#[test]
fn reads_one_result_per_line_and_marks_only_mismatches_as_failed() -> Result<(), RoundTripError> {
    let output = concat!(
        r#"{"step": "SolidColor", "matches": true, "evidence": {"color": "ff00ff00"}}"#,
        "\n\n",
        r#"{"step": "Resize", "matches": false, "evidence": {}}"#,
        "\n",
        r#"{"step": "Detach", "matches": null, "evidence": {}}"#,
        "\n",
    );
    let results = read_results(output)?;
    assert_eq!(results[0].step, "SolidColor");
    assert_eq!(results[0].matches, Some(true));
    assert_eq!(results[0].evidence.get(), r#"{"color": "ff00ff00"}"#);
    let failed: Vec<&str> = results
        .iter()
        .filter(|result| result.failed())
        .map(|result| result.step.as_str())
        .collect();
    assert_eq!(failed, ["Resize"]);
    Ok(())
}

#[test]
fn writes_each_result_as_a_line_with_the_evidence_as_printed() -> Result<(), RoundTripError> {
    let output = concat!(
        r#"{"step": "Resize", "matches": false, "evidence": {"width": 3, "height": 2}}"#,
        "\n",
        r#"{"step": "Detach", "matches": null, "evidence": {}}"#,
    );
    assert_eq!(
        json_lines(&read_results(output)?)?,
        concat!(
            r#"{"step":"Resize","matches":false,"evidence":{"width": 3, "height": 2}}"#,
            "\n",
            r#"{"step":"Detach","matches":null,"evidence":{}}"#,
            "\n",
        )
    );
    Ok(())
}

#[test]
fn reports_an_instrumentation_that_crashed() {
    let output = "INSTRUMENTATION_FAILED: com.example/.Runner\nINSTRUMENTATION_CODE: 0\n";
    assert!(matches!(
        read_results(output),
        Err(RoundTripError::InstrumentationCrashed { output: reported }) if reported == output
    ));
}

#[test]
fn reports_missing_and_malformed_results() {
    assert!(matches!(
        read_results("\n  \n"),
        Err(RoundTripError::RoundTripsMissing)
    ));
    assert!(matches!(
        read_results(r#"{"step": "SolidColor", "matches": true, "evidence": {}, "extra": 1}"#),
        Err(RoundTripError::ResultMalformed { .. })
    ));
    assert!(matches!(
        read_results(r#"{"step": "SolidColor", "evidence": {}}"#),
        Err(RoundTripError::ResultMalformed { .. })
    ));
    assert!(matches!(
        read_results("not json"),
        Err(RoundTripError::ResultMalformed { line, .. }) if line == "not json"
    ));
}
