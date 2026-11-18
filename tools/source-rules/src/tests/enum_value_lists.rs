// SPDX-License-Identifier: GPL-3.0-only
use super::{Failure, findings, safe};
use crate::error::Error;
use crate::findings::Rule;

fn hand_written(path: &str, line: usize, enum_name: &str) -> (String, Option<usize>, Rule) {
    let rule = Rule::HandWrittenEnumValues {
        enum_name: enum_name.to_owned(),
    };
    (path.to_owned(), Some(line), rule)
}

#[test]
fn an_array_of_every_variant_of_an_enum_of_the_crate_is_reported() -> Result<(), Failure> {
    let root = safe(concat!(
        "mod edges;\n",
        "pub enum Edge { Top, Bottom }\n",
        "impl Edge {\n",
        "    const ALL: [Self; 2] = [Self::Bottom, Self::Top];\n",
        "}\n",
    ));
    let edges = "fn run() {\n    for edge in [crate::Edge::Top, Edge::Bottom] {}\n}\n";
    assert_eq!(
        findings(&[("src/lib.rs", &root), ("src/edges.rs", edges)])?,
        [
            hand_written("src/edges.rs", 2, "Edge"),
            hand_written("src/lib.rs", 5, "Edge"),
        ]
    );
    Ok(())
}

#[test]
fn a_full_list_of_an_enum_declared_inside_a_macro_is_reported() -> Result<(), Failure> {
    let text = safe(concat!(
        "enum_with_values! {\n",
        "    #[derive(Clone, Copy)]\n",
        "    pub enum Edge { #[default] Top, Bottom = 4 }\n",
        "    pub const ALL;\n",
        "}\n",
        "flag_set!(pub enum Phase in Dirty { Layout => layout, Paint => paint });\n",
        "capabilities! { pub enum Capability { Panels => \"panels\", Clipboard => \"clipboard\" } }\n",
        "const EDGES: [Edge; 2] = [Edge::Top, Edge::Bottom];\n",
        "const PHASES: [Phase; 2] = [Phase::Paint, Phase::Layout];\n",
        "const CAPABILITIES: [Capability; 2] = [Capability::Panels, Capability::Clipboard];\n",
    ));
    assert_eq!(
        findings(&[("src/lib.rs", &text)])?,
        [
            hand_written("src/lib.rs", 9, "Edge"),
            hand_written("src/lib.rs", 10, "Phase"),
            hand_written("src/lib.rs", 11, "Capability"),
        ]
    );
    Ok(())
}

#[test]
fn a_full_list_of_an_enum_of_another_crate_is_reported() -> Result<(), Failure> {
    let core = safe("pub enum Edge { Top, Bottom }\n");
    let shell = safe("use core::Edge;\nfn run() { let _edges = [Edge::Top, Edge::Bottom]; }\n");
    assert_eq!(
        findings(&[("core/src/lib.rs", &core), ("shell/src/lib.rs", &shell)])?,
        [hand_written("shell/src/lib.rs", 3, "Edge")]
    );
    Ok(())
}

#[test]
fn a_macro_body_with_an_enum_that_does_not_parse_fails_the_check() {
    let text = safe(concat!(
        "macro_rules! declare { ($name:ident) => { enum_with_values! { enum $name { A } } }; }\n",
        "broken! { enum Edge }\n",
    ));
    assert!(matches!(
        findings(&[("src/lib.rs", &text)]),
        Err(Failure::Check(Error::ParseSource { .. }))
    ));
}

#[test]
fn an_array_of_some_variants_or_of_several_enums_is_accepted() -> Result<(), Failure> {
    let text = safe(concat!(
        "pub enum Edge { Top, Bottom, Left }\n",
        "pub enum Side { Top, Bottom }\n",
        "const SOME: [Edge; 2] = [Edge::Top, Edge::Bottom];\n",
        "fn run() { let _mixed = [Side::Top, Edge::Bottom]; let _other = [Axis::X, Axis::Y]; }\n",
        "trait Listed { const ALL: [Self; 2] = [Self::Top, Self::Bottom]; }\n",
        "impl Side {\n",
        "    fn run() { trait Nested { const ALL: [Self; 2] = [Self::Top, Self::Bottom]; } }\n",
        "}\n",
        "const TRACE: [Side; 3] = [Side::Top, Side::Bottom, Side::Top];\n",
        "fn sides() { assert_eq!(seen(), vec![Side::Top, Side::Top, Side::Bottom]); }\n",
    ));
    assert_eq!(findings(&[("src/lib.rs", &text)])?, []);
    Ok(())
}

#[test]
fn a_list_inside_a_macro_invocation_is_reported() -> Result<(), Failure> {
    let text = safe(concat!(
        "pub enum Side { Top, Bottom }\n",
        "fn run() {\n",
        "    let _sides = vec![Side::Top, Side::Bottom];\n",
        "    assert_eq!(sides(), [[Side::Bottom, Side::Top]]);\n",
        "}\n",
        "macro_rules! listed { ($($side:ident),+) => { [$(Side::$side),+] }; }\n",
    ));
    assert_eq!(
        findings(&[("src/lib.rs", &text)])?,
        [
            hand_written("src/lib.rs", 4, "Side"),
            hand_written("src/lib.rs", 5, "Side"),
        ]
    );
    Ok(())
}

#[test]
fn a_list_of_imported_or_aliased_names_is_reported() -> Result<(), Failure> {
    let text = safe(concat!(
        "pub enum Side { Top, Bottom }\n",
        "use Side::{Bottom, Top};\n",
        "use Side::Top as Up;\n",
        "use Side as S;\n",
        "type Alias = Side;\n",
        "fn bare() -> [Side; 2] { [Top, Bottom] }\n",
        "fn renamed() -> [Side; 2] { [Up, S::Bottom] }\n",
        "fn aliased() -> [Side; 2] { [Alias::Top, Alias::Bottom] }\n",
        "fn globbed() -> [Side; 2] { use Side::*; [Bottom, Top] }\n",
    ));
    assert_eq!(
        findings(&[("src/lib.rs", &text)])?,
        [
            hand_written("src/lib.rs", 7, "Side"),
            hand_written("src/lib.rs", 8, "Side"),
            hand_written("src/lib.rs", 9, "Side"),
            hand_written("src/lib.rs", 10, "Side"),
        ]
    );
    Ok(())
}

#[test]
fn a_bare_name_that_two_glob_imported_enums_declare_is_not_checked() -> Result<(), Failure> {
    let text = safe(concat!(
        "pub enum Side { Top, Bottom }\n",
        "pub enum Edge { Top, Bottom }\n",
        "use Edge::*;\n",
        "use Side::*;\n",
        "fn run() -> [Side; 2] { [Top, Bottom] }\n",
    ));
    assert_eq!(findings(&[("src/lib.rs", &text)])?, []);
    Ok(())
}

#[test]
fn a_list_of_an_outside_enum_named_like_a_declared_enum_is_reported() -> Result<(), Failure> {
    let text = safe(concat!(
        "pub enum Ordering { Less, Equal, Greater }\n",
        "fn run() {\n",
        "    let _outside = [std::cmp::Ordering::Less, std::cmp::Ordering::Equal, std::cmp::Ordering::Greater];\n",
        "}\n",
    ));
    assert_eq!(
        findings(&[("src/lib.rs", &text)])?,
        [hand_written("src/lib.rs", 4, "Ordering")]
    );
    Ok(())
}
