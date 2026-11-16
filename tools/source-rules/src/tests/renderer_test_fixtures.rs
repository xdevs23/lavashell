// SPDX-License-Identifier: GPL-3.0-only
use super::{Failure, findings, safe};
use crate::error::Error;
use crate::findings::Rule;

const ROOT: &str = "crates/lavashell-renderer/src/lib.rs";
const TESTS: &str = "crates/lavashell-renderer/src/tests.rs";
const NESTED: &str = "crates/lavashell-renderer/src/tests/nested.rs";
const TEST_FONT: &str = "crates/lavashell-renderer/src/test_font.rs";
const TEST_SURFACES: &str = "crates/lavashell-renderer/src/test_surfaces.rs";
const FIELD: &str = "crates/lavashell-renderer/src/field.rs";
const OTHER_ROOT: &str = "crates/lavashell-core/src/lib.rs";

const DEFINITIONS: &str = "#[cfg(test)]\nmod test_font;\n#[cfg(test)]\nmod test_surfaces;\n";
const DEFINITION_LINES: usize = 5;
const PROBE_FAMILY: &str = "pub const PROBE_FAMILY: &str = \"probe\";\n";
const TEST_SURFACE: &str = concat!(
    "pub struct TestSurface;\n",
    "fn size() -> PixelSize { PixelSize::new(1, 2) }\n",
);

type Findings = Vec<(String, Option<usize>, Rule)>;

fn renderer_findings(root: &str, files: &[(&str, &str)]) -> Result<Findings, Failure> {
    renderer_findings_defined_by(PROBE_FAMILY, TEST_SURFACE, root, files)
}

fn renderer_findings_defined_by(
    test_font: &str,
    test_surfaces: &str,
    root: &str,
    files: &[(&str, &str)],
) -> Result<Findings, Failure> {
    let root = safe(&format!("{DEFINITIONS}{root}"));
    let definitions = [
        (ROOT, root.as_str()),
        (TEST_FONT, test_font),
        (TEST_SURFACES, test_surfaces),
    ];
    findings(&[definitions.as_slice(), files].concat())
}

fn at(path: &str, line: usize, rule: Rule) -> (String, Option<usize>, Rule) {
    (path.to_owned(), Some(line), rule)
}

fn probe() -> Rule {
    Rule::ProbeFamilyLiteral {
        family: "probe".to_owned(),
    }
}

#[test]
fn a_probe_literal_and_a_pixel_size_in_a_test_module_file_are_reported() -> Result<(), Failure> {
    let tests = concat!(
        "mod nested;\n",
        "fn sizes(side: u32) {\n",
        "    let family = \"probe\";\n",
        "    let raw = r\"probe\";\n",
        "    let other = \"probes\";\n",
        "    let numbers = PixelSize::new(4, 8);\n",
        "    let named = PixelSize::new(SIDE, side);\n",
        "    let surface = SURFACE.pixel_size();\n",
        "}\n",
    );
    let nested = "fn family() -> &'static str {\n    \"probe\"\n}\n";
    assert_eq!(
        renderer_findings(
            "#[cfg(test)]\nmod tests;\n",
            &[(TESTS, tests), (NESTED, nested)]
        )?,
        [
            at(NESTED, 2, probe()),
            at(TESTS, 3, probe()),
            at(TESTS, 4, probe()),
            at(TESTS, 6, Rule::PixelSizeInTest),
            at(TESTS, 7, Rule::PixelSizeInTest),
        ]
    );
    Ok(())
}

#[test]
fn a_test_module_inside_a_source_file_is_checked() -> Result<(), Failure> {
    let root = concat!(
        "fn sized() {}\n",
        "#[cfg(test)]\n",
        "mod tests {\n",
        "    fn size() {\n",
        "        let size = PixelSize::new(1, 2);\n",
        "    }\n",
        "}\n",
    );
    assert_eq!(
        renderer_findings(root, &[])?,
        [at(ROOT, DEFINITION_LINES + 5, Rule::PixelSizeInTest)]
    );
    Ok(())
}

#[test]
fn test_functions_and_impls_outside_a_test_module_are_checked() -> Result<(), Failure> {
    let field = concat!(
        "#[cfg(test)]\n",
        "fn helper() { PixelSize::new(1, 2); }\n",
        "#[cfg(test)]\n",
        "impl X { fn family() -> &'static str { \"probe\" } }\n",
        "impl Y {\n",
        "    #[cfg(test)]\n",
        "    const FAMILY: &str = \"probe\";\n",
        "}\n",
    );
    assert_eq!(
        renderer_findings("mod field;\n", &[(FIELD, field)])?,
        [
            at(FIELD, 2, Rule::PixelSizeInTest),
            at(FIELD, 4, probe()),
            at(FIELD, 7, probe()),
        ]
    );
    Ok(())
}

#[test]
fn a_module_that_requires_test_among_other_conditions_is_checked() -> Result<(), Failure> {
    let root = "#[cfg(all(unix, all(test, feature = \"gpu\")))]\nmod tests;\n";
    let tests = "fn family() -> &'static str {\n    \"probe\"\n}\n";
    assert_eq!(
        renderer_findings(root, &[(TESTS, tests)])?,
        [at(TESTS, 2, probe())]
    );
    Ok(())
}

#[test]
fn a_condition_that_does_not_parse_is_an_error() {
    let field = "#[cfg(= broken)]\nfn helper() {}\n";
    assert!(matches!(
        renderer_findings("mod field;\n", &[(FIELD, field)]),
        Err(Failure::Check(Error::ParseSource { .. }))
    ));
}

#[test]
fn the_defining_files_and_production_code_may_name_the_family_and_build_a_pixel_size()
-> Result<(), Failure> {
    let field =
        "fn uniform() -> (&'static str, PixelSize) {\n    (\"probe\", PixelSize::new(1, 2))\n}\n";
    assert_eq!(renderer_findings("mod field;\n", &[(FIELD, field)])?, []);
    Ok(())
}

#[test]
fn the_family_is_the_value_that_test_font_defines() -> Result<(), Failure> {
    let test_font = "pub const PROBE_FAMILY: &str = \"sample\";\n";
    let tests = "const OLD: &str = \"probe\";\nconst NEW: &str = \"sample\";\n";
    let sample = Rule::ProbeFamilyLiteral {
        family: "sample".to_owned(),
    };
    assert_eq!(
        renderer_findings_defined_by(
            test_font,
            TEST_SURFACE,
            "#[cfg(test)]\nmod tests;\n",
            &[(TESTS, tests)]
        )?,
        [at(TESTS, 2, sample)]
    );
    Ok(())
}

#[test]
fn a_family_or_test_surface_defined_in_no_file_or_in_two_files_is_an_error() {
    let missing_family = renderer_findings_defined_by("", TEST_SURFACE, "", &[]);
    assert!(matches!(
        missing_family,
        Err(Failure::Check(Error::Definition {
            name: "PROBE_FAMILY",
            ..
        }))
    ));
    let second_surface = "pub struct TestSurface;\n";
    let twice = renderer_findings("mod field;\n", &[(FIELD, second_surface)]);
    assert!(matches!(
        twice,
        Err(Failure::Check(Error::Definition {
            name: "TestSurface",
            ..
        }))
    ));
}

#[test]
fn a_crate_other_than_the_renderer_is_not_checked() -> Result<(), Failure> {
    let root = safe(
        "#[cfg(test)]\nmod tests {\n    const ONE: &str = \"probe\";\n    fn size() {\n        PixelSize::new(1, 2);\n    }\n}\n",
    );
    assert_eq!(findings(&[(OTHER_ROOT, &root)])?, []);
    Ok(())
}
