// SPDX-License-Identifier: GPL-3.0-only
use super::{Failure, findings_of};
use crate::copy_catalogs::copy_report;
use crate::findings::Rule;
use crate::repository_files::read_repository_files;

const CATALOG: &str = "examples/player/copy/player.en.tsv";

fn findings(catalog: &str) -> Result<Vec<(String, Option<usize>, Rule)>, Failure> {
    findings_of(&[(CATALOG, catalog)], |root| {
        copy_report(&read_repository_files(root)?, root)
    })
}

fn at(line: usize, rule: Rule) -> (String, Option<usize>, Rule) {
    (CATALOG.to_owned(), Some(line), rule)
}

#[test]
fn a_catalog_of_keys_and_values_passes() -> Result<(), Failure> {
    assert_eq!(findings("play\tPlay\npause\tPause\n")?, []);
    Ok(())
}

#[test]
fn a_row_without_exactly_one_tab_is_reported_at_its_line() -> Result<(), Failure> {
    assert_eq!(
        findings("play\tPlay\npause\nstop\tStop\tnow\n")?,
        [at(2, Rule::CopyColumns), at(3, Rule::CopyColumns)]
    );
    Ok(())
}

#[test]
fn a_repeated_key_and_an_empty_value_are_reported() -> Result<(), Failure> {
    let key = || "play".to_owned();
    assert_eq!(
        findings("play\tPlay\nplay\t \n")?,
        [
            at(2, Rule::DuplicateCopyKey { key: key() }),
            at(2, Rule::EmptyCopy { key: key() }),
        ]
    );
    Ok(())
}

#[test]
fn a_semicolon_or_a_dash_of_typesetting_is_reported() -> Result<(), Failure> {
    let rule = |key: &str| Rule::CopyPunctuation {
        key: key.to_owned(),
    };
    assert_eq!(
        findings("one\tPlay; pause\ntwo\tPlay — pause\nthree\tPlay – pause\nfour\tPlay - pause\n")?,
        [at(1, rule("one")), at(2, rule("two")), at(3, rule("three"))]
    );
    Ok(())
}

#[test]
fn a_repository_without_catalogs_is_reported() -> Result<(), Failure> {
    let findings = findings_of(
        &[("examples/player/src/main.rs", "fn main() {}\n")],
        |root| copy_report(&read_repository_files(root)?, root),
    )?;
    assert_eq!(
        findings,
        [("examples".to_owned(), None, Rule::NoCopyCatalogs)]
    );
    Ok(())
}
