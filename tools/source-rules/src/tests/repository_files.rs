// SPDX-License-Identifier: GPL-3.0-only
use super::{Failure, findings_of};
use crate::checks::repository_reports;
use crate::error::Error;
use crate::findings::{Report, Rule};
use crate::repository_files::read_repository_files;

fn repository_findings(
    files: &[(&str, &str)],
) -> Result<Vec<(String, Option<usize>, Rule)>, Failure> {
    findings_of(files, |root| {
        let files = read_repository_files(root)?;
        let reports = repository_reports(&files, root, &[])?;
        Ok(Report {
            checked: files.len(),
            findings: reports
                .into_iter()
                .flat_map(|(_, report)| report.findings)
                .collect(),
        })
    })
}

fn comment_lines(files: &[(&str, &str)]) -> Result<Vec<(String, Option<usize>)>, Failure> {
    Ok(repository_findings(files)?
        .into_iter()
        .filter(|(_, _, rule)| *rule == Rule::Comment)
        .map(|(path, line, _)| (path, line))
        .collect())
}

#[test]
fn comments_in_every_language_are_reported() -> Result<(), Failure> {
    let files = [
        (
            "Cargo.toml",
            "# SPDX-License-Identifier: GPL-3.0-only\n[package] # note\nname = \"# text\"\n",
        ),
        (
            "Main.kt",
            "// SPDX-License-Identifier: GPL-3.0-only\nfun main() {} /* note */\n",
        ),
        (
            "default.nix",
            "# SPDX-License-Identifier: GPL-3.0-only\n{ value = \"# text\"; /* note */ }\n",
        ),
        (
            "glass.sksl",
            "// SPDX-License-Identifier: GPL-3.0-only\nuniform float inner; // note\n",
        ),
        (
            "lib.rs",
            "// SPDX-License-Identifier: GPL-3.0-only\n/// Doc.\nfn a() { b(\"// text\"); } /* note */\n",
        ),
        (
            "manifest.xml",
            "<?xml version=\"1.0\"?>\n<!-- SPDX-License-Identifier: GPL-3.0-only -->\n<a><!-- note --></a>\n",
        ),
        (
            "run.sh",
            "#!/usr/bin/env bash\n# SPDX-License-Identifier: GPL-3.0-only\necho '# text' # note\n",
        ),
        (
            "tool.py",
            concat!(
                "#!/usr/bin/env python3\n# SPDX-License-Identifier: GPL-3.0-only\n",
                "\"\"\"Docstring.\"\"\"\nvalue = \"# text\"  # note\n",
            ),
        ),
    ];
    let expected = [
        ("Cargo.toml", 2),
        ("Main.kt", 2),
        ("default.nix", 2),
        ("glass.sksl", 2),
        ("lib.rs", 2),
        ("lib.rs", 3),
        ("manifest.xml", 3),
        ("run.sh", 3),
        ("tool.py", 3),
        ("tool.py", 4),
    ]
    .map(|(path, line)| (path.to_owned(), Some(line)));
    assert_eq!(comment_lines(&files)?, expected);
    Ok(())
}

#[test]
fn a_license_header_below_the_first_line_or_with_prose_is_reported() -> Result<(), Failure> {
    let files = [
        (
            "block.rs",
            "/* SPDX-License-Identifier: GPL-3.0-only\n */\nfn a() {}\n",
        ),
        (
            "inner.rs",
            "#![forbid(unsafe_code)]\n// SPDX-License-Identifier: GPL-3.0-only\n",
        ),
        (
            "lower.rs",
            "fn a() {}\n// SPDX-License-Identifier: GPL-3.0-only\n",
        ),
        (
            "prose.rs",
            "// SPDX-License-Identifier: GPL-3.0-only, see below for why\nfn a() {}\n",
        ),
        (
            "twice.rs",
            "// SPDX-License-Identifier: GPL-3.0-only\n// SPDX-License-Identifier: MIT\nfn a() {}\n",
        ),
    ];
    let expected = [
        ("block.rs", 1),
        ("inner.rs", 2),
        ("lower.rs", 2),
        ("prose.rs", 1),
        ("twice.rs", 2),
    ]
    .map(|(path, line)| (path.to_owned(), Some(line)));
    assert_eq!(comment_lines(&files)?, expected);
    Ok(())
}

#[test]
fn safety_comments_on_unsafe_code_and_links_are_accepted() -> Result<(), Failure> {
    let files = [(
        "lib.rs",
        concat!(
            "// SPDX-License-Identifier: GPL-3.0-only\n",
            "// https://example.com/specification#section\n",
            "const LIMIT: u8 = 1;\n",
            "fn read(pointer: *const u8) -> u8 {\n",
            "    // SAFETY: the caller passes a live pointer,\n",
            "    // aligned for u8.\n",
            "    let value = unsafe { *pointer };\n",
            "    value\n",
            "}\n",
            "struct Handle;\n",
            "// SAFETY: the handle has no thread affinity.\n",
            "unsafe impl Send for Handle {}\n",
            "/// # Safety\n",
            "/// `pointer` must be live.\n",
            "#[must_use]\n",
            "pub unsafe fn load(pointer: *const u8) -> u8 {\n",
            "    // SAFETY: the contract of this function.\n",
            "    unsafe { *pointer }\n",
            "}\n",
            "/// # Safety\n",
            "/// Implementers return live pointers.\n",
            "pub unsafe trait Source {}\n",
        ),
    )];
    assert_eq!(comment_lines(&files)?, []);
    Ok(())
}

#[test]
fn safety_comments_away_from_unsafe_code_and_links_with_prose_are_reported() -> Result<(), Failure>
{
    let files = [(
        "lib.rs",
        concat!(
            "// SPDX-License-Identifier: GPL-3.0-only\n",
            "// https://example.com/specification see section 2\n",
            "/// https://example.com/specification\n",
            "// note: prose that parses as a URL\n",
            "const LIMIT: u8 = 1;\n",
            "// SAFETY: nothing is unsafe here.\n",
            "fn safe() {}\n",
            "fn read(pointer: *const u8) -> u8 {\n",
            "    // SAFETY: the line below is blank.\n",
            "\n",
            "    unsafe { *pointer }\n",
            "}\n",
            "/// # Safety\n",
            "/// Nothing.\n",
            "pub fn checked() {}\n",
            "/// Reads.\n",
            "/// # Safety\n",
            "/// `pointer` must be live.\n",
            "pub unsafe fn load(pointer: *const u8) -> u8 {\n",
            "    // SAFETY: the contract of this function.\n",
            "    unsafe { *pointer }\n",
            "}\n",
            "// SAFETY: this prose sits above a function.\n",
            "fn wrapper(pointer: *const u8) -> u8 {\n",
            "    let copy = pointer;\n",
            "    unsafe { *copy }\n",
            "}\n",
            "// SAFETY: this prose sits above an impl.\n",
            "impl Wrapper {\n",
            "    fn read(&self) -> u8 { unsafe { 0 } }\n",
            "}\n",
        ),
    )];
    let lines: Vec<_> = comment_lines(&files)?
        .into_iter()
        .map(|(_, line)| line)
        .collect();
    assert_eq!(lines, [2, 3, 4, 6, 9, 13, 14, 16, 17, 18, 23, 28].map(Some));
    Ok(())
}

#[test]
fn a_doc_attribute_with_a_value_is_reported_and_a_hidden_item_is_not() -> Result<(), Failure> {
    let files = [(
        "lib.rs",
        concat!(
            "// SPDX-License-Identifier: GPL-3.0-only\n",
            "#[doc = \"Draws.\"]\n",
            "fn draw() {}\n",
            "#[cfg_attr(unix, doc = \"Waits.\")]\n",
            "fn wait() {}\n",
            "#[doc(hidden)]\n",
            "pub fn hidden() {}\n",
        ),
    )];
    assert_eq!(
        comment_lines(&files)?,
        [2, 4].map(|line| ("lib.rs".to_owned(), Some(line)))
    );
    Ok(())
}

#[test]
fn comments_added_by_a_patch_are_reported_by_their_line_in_the_patch() -> Result<(), Failure> {
    let patch = concat!(
        "--- a/src/draw.cpp\n",
        "+++ b/src/draw.cpp\n",
        "@@ -1,3 +1,4 @@\n",
        " // Upstream comment.\n",
        "-int a = 1;\n",
        "+// Added comment.\n",
        "+int a = 2;\n",
        "+int b = 3; /* added */\n",
    );
    assert_eq!(
        comment_lines(&[("nix/draw.patch", patch)])?,
        [
            ("nix/draw.patch".to_owned(), Some(6)),
            ("nix/draw.patch".to_owned(), Some(8))
        ]
    );
    Ok(())
}

#[test]
fn the_grammar_of_a_patch_follows_the_extension_of_each_patched_file() -> Result<(), Failure> {
    let patch = concat!(
        "--- a/src/draw.cpp\n",
        "+++ b/src/draw.cpp\n",
        "@@ -0,0 +1 @@\n",
        "+int a = 2; // added\n",
        "--- a/tool.py\n",
        "+++ b/tool.py\n",
        "@@ -0,0 +1,2 @@\n",
        "+value = 1  # added\n",
        "+other = \"// text\"\n",
    );
    assert_eq!(
        comment_lines(&[("nix/draw.patch", patch)])?,
        [4, 8].map(|line| ("nix/draw.patch".to_owned(), Some(line)))
    );
    Ok(())
}

#[test]
fn a_removed_and_an_added_line_that_look_like_file_headers_stay_lines_of_their_hunk()
-> Result<(), Failure> {
    let patch = concat!(
        "--- a/src/draw.cpp\n",
        "+++ b/src/draw.cpp\n",
        "@@ -1 +1,2 @@\n",
        "--- x\n",
        "+++ y;\n",
        "+// Added comment.\n",
    );
    assert_eq!(
        comment_lines(&[("nix/draw.patch", patch)])?,
        [("nix/draw.patch".to_owned(), Some(6))]
    );
    Ok(())
}

#[test]
fn a_patch_of_a_file_without_a_known_language_is_an_error() {
    let patch = "--- a/data.zzz\n+++ b/data.zzz\n@@ -0,0 +1 @@\n+text\n";
    let outcome = repository_findings(&[("nix/data.patch", patch)]);
    assert!(
        matches!(outcome, Err(Failure::Check(Error::PatchedLanguage { patched, .. })) if patched == "data.zzz")
    );
}

#[test]
fn documents_besides_agents_at_the_root_are_reported_in_any_case() -> Result<(), Failure> {
    let files = [
        ("AGENTS.md", "Rules.\n"),
        ("README.MD", "# Title\n"),
        ("docs/AGENTS.md", "Rules.\n"),
        ("docs/guide.markdown", "Guide.\n"),
        ("docs/notes.txt", "Notes.\n"),
        ("docs/manual.rst", "Manual.\n"),
    ];
    let documents: Vec<_> = repository_findings(&files)?
        .into_iter()
        .map(|(path, _, rule)| (path, rule))
        .collect();
    let expected = [
        "README.MD",
        "docs/AGENTS.md",
        "docs/guide.markdown",
        "docs/manual.rst",
        "docs/notes.txt",
    ]
    .map(|path| {
        (
            path.to_owned(),
            Rule::Document {
                allowed: "AGENTS.md",
            },
        )
    });
    assert_eq!(documents, expected);
    Ok(())
}

#[test]
fn hidden_files_are_checked_ignored_files_are_not_and_unknown_kinds_are_reported()
-> Result<(), Failure> {
    let files = [
        (".gitignore", "/target\n"),
        (
            ".cargo/config.toml",
            "# SPDX-License-Identifier: GPL-3.0-only\n# note\n",
        ),
        (".git/config", "[core]\n"),
        ("Makefile", "all:\n"),
        ("script.js", "run();\n"),
        ("target/build.rs", "// note\n"),
    ];
    assert_eq!(
        repository_findings(&files)?,
        [
            ("Makefile".to_owned(), None, Rule::UnknownFileKind),
            ("script.js".to_owned(), None, Rule::UnknownFileKind),
            (".cargo/config.toml".to_owned(), Some(2), Rule::Comment),
        ]
    );
    Ok(())
}

#[test]
fn files_excluded_by_the_exclude_file_of_the_repository_are_not_checked() -> Result<(), Failure> {
    let files = [
        (".git/info/exclude", "/notes.md\n"),
        ("notes.md", "Notes.\n"),
        ("README.md", "# Title\n"),
    ];
    let documents: Vec<_> = repository_findings(&files)?
        .into_iter()
        .map(|(path, _, _)| path)
        .collect();
    assert_eq!(documents, ["README.md"]);
    Ok(())
}

#[test]
fn comments_in_ignore_and_attribute_files_are_reported_except_the_license_header()
-> Result<(), Failure> {
    let files = [
        (
            ".gitignore",
            "# SPDX-License-Identifier: GPL-3.0-only\n/target\n# note\n\\#kept\n",
        ),
        (".gitattributes", "*.png binary\n# note\n"),
    ];
    assert_eq!(
        comment_lines(&files)?,
        [
            (".gitattributes".to_owned(), Some(2)),
            (".gitignore".to_owned(), Some(3))
        ]
    );
    Ok(())
}
