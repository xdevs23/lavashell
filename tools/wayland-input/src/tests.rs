// SPDX-License-Identifier: GPL-3.0-only
use super::*;

fn parse(line: &str) -> Result<Command, clap::Error> {
    Command::try_parse_from(line.split_whitespace())
}

#[test]
fn every_command_of_the_protocol_parses() {
    for line in ["move 100 100", "key SPCE"] {
        assert!(parse(line).is_ok(), "{line} was rejected");
    }
}

#[test]
fn malformed_commands_are_rejected() {
    for line in ["", "move 1", "key", "jump"] {
        assert!(parse(line).is_err(), "{line:?} was accepted");
    }
}
