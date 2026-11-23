// SPDX-License-Identifier: GPL-3.0-only
use super::{NamesNextStep, NextStep};

#[test]
fn every_next_step_is_one_whole_sentence() {
    for step in NextStep::ALL {
        let sentence = step.to_string();
        assert!(
            sentence.starts_with(char::is_uppercase),
            "{step:?}: {sentence}"
        );
        assert_eq!(
            sentence.find('.'),
            Some(sentence.len() - 1),
            "{step:?}: {sentence}"
        );
        assert!(!sentence.contains(';'), "{step:?}: {sentence}");
    }
}

struct Failure(NextStep);

impl NamesNextStep for Failure {
    fn next_step(&self) -> NextStep {
        self.0
    }
}

#[test]
fn several_failures_ask_for_a_report_when_one_does_and_take_the_first_step_otherwise() {
    let report = [
        Failure(NextStep::FreeGpuMemory),
        Failure(NextStep::ReportTheFailure),
        Failure(NextStep::ChangeTheScene),
    ];
    assert_eq!(report.next_step(), NextStep::ReportTheFailure);
    let outside = [
        Failure(NextStep::FreeGpuMemory),
        Failure(NextStep::ChangeTheScene),
    ];
    assert_eq!(outside.next_step(), NextStep::FreeGpuMemory);
}
