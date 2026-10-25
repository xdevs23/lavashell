// SPDX-License-Identifier: GPL-3.0-only
use crate::check;
use crate::error::Error;
use crate::rules::Rule;
use std::path::Path;

#[derive(Debug, thiserror::Error)]
enum Failure {
    #[error("a file of the test crate cannot be written")]
    Write(#[from] std::io::Error),
    #[error("the test crate cannot be checked")]
    Check(#[from] Error),
}

/// Checks a crate made of `files` and returns each finding as the path relative to the crate directory, the line
/// and the rule. Each entry of `files` is a path below the crate directory and the text of that file.
fn findings(files: &[(&str, &str)]) -> Result<Vec<(String, usize, Rule)>, Failure> {
    let directory = tempfile::tempdir()?;
    for (path, text) in files {
        let path = directory.path().join(path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, text)?;
    }
    let (found, _) = check([directory.path().join(files[0].0)])?;
    Ok(found
        .into_iter()
        .map(|finding| {
            let path = finding
                .path
                .strip_prefix(directory.path())
                .unwrap_or(Path::new(""));
            (path.display().to_string(), finding.line, finding.rule)
        })
        .collect())
}

fn safe(text: &str) -> String {
    format!("#![forbid(unsafe_code)]\n{text}")
}

#[test]
fn a_value_bound_to_a_wildcard_is_reported_with_or_without_a_type() -> Result<(), Failure> {
    let text = safe(
        "fn run() {\n    let _ = send();\n    let _: Sent = send();\n    let _sent = send();\n}\n",
    );
    assert_eq!(
        findings(&[("src/lib.rs", &text)])?,
        [
            ("src/lib.rs".to_owned(), 3, Rule::WildcardBinding),
            ("src/lib.rs".to_owned(), 4, Rule::WildcardBinding),
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
    let rules: Vec<_> = findings(&[("src/lib.rs", &text)])?
        .into_iter()
        .map(|(_, line, rule)| (line, rule))
        .collect();
    assert_eq!(
        rules,
        [
            (2, Rule::SilencedLint),
            (3, Rule::SilencedLint),
            (4, Rule::SilencedLint)
        ]
    );
    Ok(())
}

#[test]
fn lint_names_in_strings_and_comments_and_attributes_that_check_more_are_accepted()
-> Result<(), Failure> {
    let text = safe(concat!(
        "// #[allow(dead_code)]\n",
        "#[doc = \"cannot warn (x) or allow(y)\"]\n",
        "#[deny(clippy::unwrap_used)]\n",
        "fn call() -> Result<(), Error> { let _text = \"let _ = x; Ok(true)\"; Ok(()) }\n",
    ));
    assert_eq!(findings(&[("src/lib.rs", &text)])?, []);
    Ok(())
}

#[test]
fn a_bool_inside_ok_is_reported_as_an_outcome() -> Result<(), Failure> {
    let text = safe(
        "fn wait() -> Result<bool, Error> {\n    Ok(false)\n}\nfn read() -> io::Result<bool> { read() }\n",
    );
    let rules: Vec<_> = findings(&[("src/lib.rs", &text)])?
        .into_iter()
        .map(|(_, line, rule)| (line, rule))
        .collect();
    assert_eq!(
        rules,
        [
            (2, Rule::BoolOutcome),
            (3, Rule::BoolOutcome),
            (5, Rule::BoolOutcome)
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
    let rules: Vec<_> = findings(&[("src/lib.rs", &text)])?
        .into_iter()
        .map(|(_, line, rule)| (line, rule))
        .collect();
    assert_eq!(rules, [(4, Rule::IgnoredTest), (6, Rule::IgnoredTest)]);
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
    let rules: Vec<_> = findings(&[("src/lib.rs", &text)])?
        .into_iter()
        .map(|(_, line, rule)| (line, rule))
        .collect();
    assert_eq!(rules, [(4, Rule::PanickingTest), (6, Rule::PanickingTest)]);
    Ok(())
}

#[test]
fn a_string_or_a_boxed_error_object_as_the_error_is_reported() -> Result<(), Failure> {
    let text = safe(concat!(
        "fn load() -> Result<Icon, String> { load() }\n",
        "fn run() -> Result<(), Box<dyn std::error::Error + Send>> { run() }\n",
        "fn text() -> Result<String, Error> { text() }\n",
        "fn cause(error: &(dyn Error + 'static)) -> Option<Box<Icon>> { cause(error) }\n",
    ));
    let rules: Vec<_> = findings(&[("src/lib.rs", &text)])?
        .into_iter()
        .map(|(_, line, rule)| (line, rule))
        .collect();
    assert_eq!(rules, [(2, Rule::ErasedError), (3, Rule::ErasedError)]);
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
        [("src/lib.rs".to_owned(), 1, Rule::MissingForbid)]
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
            "#![forbid(unsafe_code)]\nmod named;\nmod folder;\n#[path = \"other/place.rs\"]\nmod moved;\n",
        ),
        ("src/named.rs", "mod child;\n"),
        ("src/named/child.rs", "fn a() { let _ = b(); }\n"),
        ("src/folder/mod.rs", "mod inner { mod deep; }\n"),
        ("src/folder/inner/deep.rs", "fn a() { let _ = b(); }\n"),
        ("src/other/place.rs", "mod near;\n"),
        ("src/other/near.rs", "fn a() { let _ = b(); }\n"),
    ];
    let paths: Vec<_> = findings(&files)?
        .into_iter()
        .map(|(path, _, _)| path)
        .collect();
    assert_eq!(
        paths,
        [
            "src/folder/inner/deep.rs",
            "src/named/child.rs",
            "src/other/near.rs"
        ]
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
