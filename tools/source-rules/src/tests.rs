// SPDX-License-Identifier: GPL-3.0-only
use crate::checks::rust_report;
use crate::error::Error;
use crate::findings::{Report, Rule};
use crate::repository_files::read_repository_files;
use std::path::{Path, PathBuf, StripPrefixError};

mod repository_files;

#[derive(Debug, thiserror::Error)]
pub(super) enum Failure {
    #[error("a file of the test crate cannot be written")]
    Write(#[from] std::io::Error),
    #[error("the test crate cannot be checked")]
    Check(#[from] Error),
    #[error("{path} is not inside the test directory")]
    OutsideDirectory {
        path: PathBuf,
        #[source]
        source: StripPrefixError,
    },
}

fn findings(files: &[(&str, &str)]) -> Result<Vec<(String, Option<usize>, Rule)>, Failure> {
    findings_of(files, |root| {
        rust_report(&read_repository_files(root)?, &[root.join(files[0].0)])
    })
}

pub(super) fn findings_of(
    files: &[(&str, &str)],
    run_check: impl FnOnce(&Path) -> Result<Report, Error>,
) -> Result<Vec<(String, Option<usize>, Rule)>, Failure> {
    let directory = tempfile::tempdir()?;
    for (path, text) in files {
        let path = directory.path().join(path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, text)?;
    }
    run_check(directory.path())?
        .findings
        .into_iter()
        .map(|finding| {
            let path = finding
                .path
                .strip_prefix(directory.path())
                .map_err(|source| Failure::OutsideDirectory {
                    path: finding.path.clone(),
                    source,
                })?;
            Ok((path.display().to_string(), finding.line, finding.rule))
        })
        .collect()
}

fn line_rules(text: &str) -> Result<Vec<(Option<usize>, Rule)>, Failure> {
    Ok(findings(&[("src/lib.rs", text)])?
        .into_iter()
        .map(|(_, line, rule)| (line, rule))
        .collect())
}

fn safe(text: &str) -> String {
    format!("#![forbid(unsafe_code)]\n{text}")
}

#[test]
fn a_value_bound_to_a_wildcard_is_reported_with_or_without_a_type() -> Result<(), Failure> {
    let text = safe(concat!(
        "fn run() {\n    let _ = send();\n    let _: Sent = send();\n    let _sent = send();\n",
        "    let (_, kept) = split();\n    match kept {\n        _ => {}\n    }\n}\n",
    ));
    assert_eq!(
        findings(&[("src/lib.rs", &text)])?,
        [
            ("src/lib.rs".to_owned(), Some(3), Rule::WildcardBinding),
            ("src/lib.rs".to_owned(), Some(4), Rule::WildcardBinding),
            ("src/lib.rs".to_owned(), Some(6), Rule::WildcardBinding),
        ]
    );
    Ok(())
}

#[test]
fn an_attribute_that_allows_expects_or_lowers_a_lint_is_reported() -> Result<(), Failure> {
    let text = safe(concat!(
        "#![allow(dead_code)]\n",
        "#[expect(clippy::cast_possible_truncation, reason = \"wraps\")]\n",
        "#[cfg_attr(test, cfg_attr(unix, warn(clippy::unwrap_used)))]\n",
        "fn call() {}\n",
    ));
    let rules = line_rules(&text)?;
    assert_eq!(
        rules,
        [
            (Some(2), Rule::SilencedLint),
            (Some(3), Rule::SilencedLint),
            (Some(4), Rule::SilencedLint)
        ]
    );
    Ok(())
}

#[test]
fn rules_apply_inside_macro_definitions_and_invocations() -> Result<(), Failure> {
    let text = safe(concat!(
        "macro_rules! tolerate {\n",
        "    ($lint:ident) => { #[allow($lint)] fn quiet() {} };\n",
        "    ($attribute:meta) => { #[$attribute] fn plain() {} };\n",
        "}\n",
        "wrap! {\n",
        "    #[ignore]\n",
        "    fn waits() {}\n",
        "    let _ = send();\n",
        "}\n",
        "register!(#[cfg_attr(unix, should_panic)] fn aborts() {});\n",
    ));
    let rules = line_rules(&text)?;
    assert_eq!(
        rules,
        [
            (Some(3), Rule::SilencedLint),
            (Some(7), Rule::IgnoredTest),
            (Some(9), Rule::WildcardBinding),
            (Some(11), Rule::PanickingTest)
        ]
    );
    Ok(())
}

#[test]
fn an_attribute_inside_a_macro_that_does_not_parse_is_an_error() {
    let text = safe("wrap! {\n    #[= broken]\n    fn waits() {}\n}\n");
    assert!(matches!(
        line_rules(&text),
        Err(Failure::Check(Error::ParseSource { .. }))
    ));
}

#[test]
fn lint_names_in_strings_and_comments_and_attributes_that_check_more_are_accepted()
-> Result<(), Failure> {
    let text = safe(concat!(
        "// #[allow(dead_code)]\n",
        "#[must_use = \"cannot warn (x) or allow(y)\"]\n",
        "#[deny(clippy::unwrap_used)]\n",
        "fn call() -> Result<(), Error> { let _text = \"let _ = x; Ok(true)\"; Ok(()) }\n",
    ));
    assert_eq!(findings(&[("src/lib.rs", &text)])?, []);
    Ok(())
}

#[test]
fn a_bool_inside_ok_is_reported_as_an_outcome() -> Result<(), Failure> {
    let text = safe(concat!(
        "fn wait() -> Result<bool, Error> {\n    Ok(false)\n}\n",
        "fn read() -> io::Result<bool> { read() }\n",
        "fn flag() -> Result<core::primitive::bool, Error> { flag() }\n",
    ));
    let rules = line_rules(&text)?;
    assert_eq!(
        rules,
        [
            (Some(2), Rule::BoolOutcome),
            (Some(3), Rule::BoolOutcome),
            (Some(5), Rule::BoolOutcome),
            (Some(6), Rule::BoolOutcome)
        ]
    );
    Ok(())
}

#[test]
fn a_test_marked_as_ignored_is_reported() -> Result<(), Failure> {
    let text = safe(concat!(
        "// #[ignore] stays a comment\n",
        "#[test]\n",
        "#[ignore = \"needs a device\"]\n",
        "fn draws() {}\n",
        "#[cfg_attr(unix, ignore)]\n",
        "fn waits() {}\n",
    ));
    let rules = line_rules(&text)?;
    assert_eq!(
        rules,
        [(Some(4), Rule::IgnoredTest), (Some(6), Rule::IgnoredTest)]
    );
    Ok(())
}

#[test]
fn a_test_that_expects_a_panic_is_reported() -> Result<(), Failure> {
    let text = safe(concat!(
        "// #[should_panic] stays a comment\n",
        "#[test]\n",
        "#[should_panic(expected = \"keys must be unique\")]\n",
        "fn refuses() {}\n",
        "#[cfg_attr(unix, should_panic)]\n",
        "fn aborts() {}\n",
    ));
    let rules = line_rules(&text)?;
    assert_eq!(
        rules,
        [
            (Some(4), Rule::PanickingTest),
            (Some(6), Rule::PanickingTest)
        ]
    );
    Ok(())
}

#[test]
fn a_string_or_a_boxed_error_object_as_the_error_is_reported() -> Result<(), Failure> {
    let text = safe(concat!(
        "fn load() -> Result<Icon, String> { load() }\n",
        "fn run() -> Result<(), Box<dyn std::error::Error + Send>> { run() }\n",
        "fn text() -> Result<String, Error> { text() }\n",
        "fn cause(error: &(dyn Error + 'static)) -> Option<Box<Icon>> { cause(error) }\n",
        "fn names() -> Result<Icon, std::string::String> { names() }\n",
    ));
    let rules = line_rules(&text)?;
    assert_eq!(
        rules,
        [
            (Some(2), Rule::ErasedError),
            (Some(3), Rule::ErasedError),
            (Some(6), Rule::ErasedError)
        ]
    );
    Ok(())
}

#[test]
fn a_crate_without_unsafe_code_must_forbid_it_in_its_root() -> Result<(), Failure> {
    let missing = [(
        "src/lib.rs",
        "// #![forbid(unsafe_code)]\n/// Not unsafe.\nfn call() {}\n",
    )];
    assert_eq!(
        findings(&missing)?,
        [("src/lib.rs".to_owned(), None, Rule::MissingForbid)]
    );
    assert_eq!(
        findings(&[
            ("src/lib.rs", "mod raw;\n"),
            ("src/raw.rs", "fn call() { unsafe {} }\n")
        ])?,
        []
    );
    Ok(())
}

#[test]
fn modules_are_found_by_name_by_path_attribute_and_inside_inline_modules() -> Result<(), Failure> {
    let files = [
        (
            "src/lib.rs",
            concat!(
                "#![forbid(unsafe_code)]\nmod named;\nmod folder;\n",
                "#[path = \"other/place.rs\"]\nmod moved;\n",
                "#[path = \"../shared/place.rs\"]\nmod above;\n",
            ),
        ),
        ("src/named.rs", "mod child;\n"),
        ("src/named/child.rs", "fn a() { let _ = b(); }\n"),
        ("src/folder/mod.rs", "mod inner { mod deep; }\n"),
        ("src/folder/inner/deep.rs", "fn a() { let _ = b(); }\n"),
        ("src/other/place.rs", "mod near;\n"),
        ("src/other/near.rs", "fn a() { let _ = b(); }\n"),
        ("shared/place.rs", "fn a() { let _ = b(); }\n"),
    ];
    let paths: Vec<_> = findings(&files)?
        .into_iter()
        .map(|(path, _, _)| path)
        .collect();
    assert_eq!(
        paths,
        [
            "shared/place.rs",
            "src/folder/inner/deep.rs",
            "src/named/child.rs",
            "src/other/near.rs"
        ]
    );
    Ok(())
}

#[test]
fn a_rust_file_that_no_module_declares_is_still_checked() -> Result<(), Failure> {
    let files = [
        ("src/lib.rs", "#![forbid(unsafe_code)]\n"),
        ("fixtures/orphan.rs", "fn a() { let _ = b(); }\n"),
    ];
    assert_eq!(
        findings(&files)?,
        [(
            "fixtures/orphan.rs".to_owned(),
            Some(1),
            Rule::WildcardBinding
        )]
    );
    Ok(())
}

#[test]
fn a_declared_module_without_a_file_is_an_error() {
    let missing = findings(&[("src/lib.rs", "#![forbid(unsafe_code)]\nmod absent;\n")]);
    assert!(
        matches!(missing, Err(Failure::Check(Error::ModuleFile { module, .. })) if module == "absent")
    );
}
