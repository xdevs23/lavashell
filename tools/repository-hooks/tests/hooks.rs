// SPDX-License-Identifier: GPL-3.0-only
#![forbid(unsafe_code)]

use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Output, Stdio};
use tempfile::TempDir;

const HOOKS: &str = env!("CARGO_BIN_EXE_lavashell-repository-hooks");
const LISTED_WORDS: &str = ".git/info/trademarks";
const EXEMPTED_SPELLINGS: &str = ".git/info/trademark-spellings";
const SOME_EDITOR: &str = "editor";
const NO_EDITOR: &str = ":";
const ABSENT_OBJECT: &str = "0000000000000000000000000000000000000000";
const OBJECT_MISSING_HERE: &str = "1111111111111111111111111111111111111111";
const WITH_LISTED_WORD: &str = "use lavashell_brandname::Thing;";
const LISTED_WORD_IN_LINE: &str = "the line contains a word from the local word list";
const LISTED_WORD_IN_NAME: &str = "the file name contains a word from the local word list";
const NOT_ONE_TITLE: &str =
    "the commit message must be exactly one title line, without a body or trailers";
const WRITE_WORD_LIST: &str = concat!(
    "Write one regular expression per line into the word list reported above, ",
    "then repeat the git command.\n",
);
const MAKE_WORD_LIST_READABLE: &str = concat!(
    "Make the word list reported above a readable UTF-8 text file, ",
    "then repeat the git command.\n",
);
const REPORT_DEFECT: &str = concat!(
    "The repository hooks are defective. ",
    "Report this output to the people who develop the project.",
);
const STARTS_WITH_TEMPORARY_PATH: &str =
    "the message starts with the path of the temporary directory and the cause";

#[derive(Debug, thiserror::Error)]
enum Failure {
    #[error("the test cannot write a file into the test repository")]
    Write(#[source] std::io::Error),
    #[error("the test cannot run {program}")]
    Run {
        program: String,
        #[source]
        source: std::io::Error,
    },
    #[error("git {arguments:?} exited with {status} in the test repository")]
    Git {
        arguments: Vec<String>,
        status: ExitStatus,
    },
}

#[derive(Debug, PartialEq, Eq)]
struct HookRun {
    code: Option<i32>,
    messages: String,
}

fn passed() -> HookRun {
    HookRun {
        code: Some(0),
        messages: String::new(),
    }
}

fn refused(messages: &[String]) -> HookRun {
    HookRun {
        code: Some(1),
        messages: messages
            .iter()
            .flat_map(|message| [message.as_str(), "\n"])
            .collect(),
    }
}

fn listed_line(commit: &str, path: &str) -> String {
    format!("{commit:.12}: {path}:1: {LISTED_WORD_IN_LINE}: {WITH_LISTED_WORD}")
}

fn write(path: &Path, content: &str) -> Result<(), Failure> {
    std::fs::write(path, content).map_err(Failure::Write)
}

fn run_without_user_settings(command: &mut Command) -> Result<Output, Failure> {
    command
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .output()
        .map_err(|source| Failure::Run {
            program: command.get_program().to_string_lossy().into_owned(),
            source,
        })
}

fn git_output(directory: &Path, arguments: &[&str]) -> Result<String, Failure> {
    let output = run_without_user_settings(
        Command::new("git")
            .current_dir(directory)
            .args(arguments)
            .stdin(Stdio::null()),
    )?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    } else {
        Err(Failure::Git {
            arguments: arguments
                .iter()
                .map(|&argument| argument.to_owned())
                .collect(),
            status: output.status,
        })
    }
}

fn git(directory: &Path, arguments: &[&str]) -> Result<(), Failure> {
    git_output(directory, arguments).map(drop)
}

struct TestRepository {
    directory: TempDir,
}

impl TestRepository {
    fn create() -> Result<Self, Failure> {
        let repository = Self {
            directory: tempfile::tempdir().map_err(Failure::Write)?,
        };
        let root = repository.root();
        std::fs::create_dir(&root).map_err(Failure::Write)?;
        git(&root, &["init", "--quiet", "--initial-branch=main"])?;
        for (key, value) in [
            ("user.name", "Test"),
            ("user.email", "test@example.invalid"),
        ] {
            git(&root, &["config", key, value])?;
        }
        write(&root.join(LISTED_WORDS), "brandname\n")?;
        write(&root.join(EXEMPTED_SPELLINGS), "target = \"brandname\"\n")?;
        repository.commit("plain.rs", "fn main() {}\n", "add plain")?;
        Ok(repository)
    }

    fn root(&self) -> PathBuf {
        self.directory.path().join("repository")
    }

    fn stage(&self, name: &str, content: &str) -> Result<(), Failure> {
        write(&self.root().join(name), content)?;
        git(&self.root(), &["--literal-pathspecs", "add", name])
    }

    fn commit(&self, name: &str, content: &str, message: &str) -> Result<(), Failure> {
        self.stage(name, content)?;
        git(&self.root(), &["commit", "--quiet", "--message", message])
    }

    fn head(&self) -> Result<String, Failure> {
        git_output(&self.root(), &["rev-parse", "HEAD"])
    }

    fn run_hook(
        &self,
        directory: &Path,
        arguments: &[&str],
        editor: &str,
        pushed_refs: &str,
    ) -> Result<HookRun, Failure> {
        let input = self.directory.path().join("pushed-refs");
        write(&input, pushed_refs)?;
        let output = run_without_user_settings(
            Command::new(HOOKS)
                .current_dir(directory)
                .args(arguments)
                .env("GIT_EDITOR", editor)
                .stdin(File::open(&input).map_err(Failure::Write)?),
        )?;
        Ok(HookRun {
            code: output.status.code(),
            messages: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }

    fn pre_commit(&self) -> Result<HookRun, Failure> {
        self.run_hook(&self.root(), &["pre-commit"], SOME_EDITOR, "")
    }

    fn commit_msg(&self, message: &str, editor: &str) -> Result<HookRun, Failure> {
        let path = self.directory.path().join("message");
        write(&path, message)?;
        let path = path.to_string_lossy();
        self.run_hook(&self.root(), &["commit-msg", &path], editor, "")
    }

    fn pre_push(&self, remote: &str, pushed_refs: &str) -> Result<HookRun, Failure> {
        let arguments = ["pre-push", remote, "../remote.git"];
        self.run_hook(&self.root(), &arguments, SOME_EDITOR, pushed_refs)
    }

    fn push_head(&self) -> Result<HookRun, Failure> {
        let head = self.head()?;
        self.pre_push(
            "origin",
            &format!("refs/heads/main {head} refs/heads/main {ABSENT_OBJECT}\n"),
        )
    }
}

#[test]
fn a_title_passes() -> Result<(), Failure> {
    let repository = TestRepository::create()?;
    let run = repository.commit_msg("core: add a thing\n", SOME_EDITOR)?;
    assert_eq!(run, passed());
    Ok(())
}

#[test]
fn a_body_or_a_trailer_is_refused() -> Result<(), Failure> {
    let repository = TestRepository::create()?;
    for message in [
        "core: add a thing\n\nbody\n",
        "core: add a thing\n\nSigned-off-by: x\n",
        "",
    ] {
        let run = repository.commit_msg(message, SOME_EDITOR)?;
        assert_eq!(run, refused(&[NOT_ONE_TITLE.to_owned()]));
    }
    Ok(())
}

#[test]
fn a_verbose_commit_counts_only_the_text_above_the_diff() -> Result<(), Failure> {
    let repository = TestRepository::create()?;
    let verbose = concat!(
        "core: add a thing\n",
        "# ------------------------ >8 ------------------------\n",
        "diff --git a/x b/x\n",
    );
    assert_eq!(repository.commit_msg(verbose, SOME_EDITOR)?, passed());
    Ok(())
}

#[test]
fn a_line_that_only_resembles_the_cut_line_does_not_end_the_message() -> Result<(), Failure> {
    let repository = TestRepository::create()?;
    let with_body = concat!(
        "core: add a thing\n",
        "x ------------------------ >8 ------------------------\n",
        "body\n",
    );
    let run = repository.commit_msg(with_body, SOME_EDITOR)?;
    assert_eq!(run, refused(&[NOT_ONE_TITLE.to_owned()]));
    Ok(())
}

#[test]
fn a_message_given_without_an_editor_keeps_its_comment_lines() -> Result<(), Failure> {
    let repository = TestRepository::create()?;
    let with_comment = "core: add a thing\n\n# body\n";
    assert_eq!(repository.commit_msg(with_comment, SOME_EDITOR)?, passed());
    let run = repository.commit_msg(with_comment, NO_EDITOR)?;
    assert_eq!(run, refused(&[NOT_ONE_TITLE.to_owned()]));
    Ok(())
}

#[test]
fn a_title_with_a_listed_word_is_refused() -> Result<(), Failure> {
    let repository = TestRepository::create()?;
    let run = repository.commit_msg("BrandName: add a thing\n", NO_EDITOR)?;
    let listed_word = "the commit message contains a word from the local word list";
    assert_eq!(
        run,
        refused(&[format!("{listed_word}: BrandName: add a thing")])
    );
    Ok(())
}

#[test]
fn staged_content_with_a_listed_word_is_refused_and_an_exempted_spelling_passes()
-> Result<(), Failure> {
    let repository = TestRepository::create()?;
    repository.stage("exempted.rs", "#[cfg(target = \"brandname\")]\nfn a() {}\n")?;
    assert_eq!(repository.pre_commit()?, passed());
    repository.stage("named.rs", &format!("fn a() {{}}\n  {WITH_LISTED_WORD}\n"))?;
    let finding = format!("named.rs:2: {LISTED_WORD_IN_LINE}: {WITH_LISTED_WORD}");
    assert_eq!(repository.pre_commit()?, refused(&[finding]));
    Ok(())
}

#[test]
fn a_staged_file_name_with_a_listed_word_is_refused() -> Result<(), Failure> {
    let repository = TestRepository::create()?;
    repository.stage("brandname.rs", "fn a() {}\n")?;
    let finding = format!("brandname.rs: {LISTED_WORD_IN_NAME}");
    assert_eq!(repository.pre_commit()?, refused(&[finding]));
    Ok(())
}

#[test]
fn a_staged_file_name_is_not_a_pattern() -> Result<(), Failure> {
    let repository = TestRepository::create()?;
    repository.commit("a.rs", WITH_LISTED_WORD, "add a")?;
    repository.stage("[a].rs", "fn a() {}\n")?;
    assert_eq!(repository.pre_commit()?, passed());
    repository.stage(":b.rs", WITH_LISTED_WORD)?;
    let finding = format!(":b.rs:1: {LISTED_WORD_IN_LINE}: {WITH_LISTED_WORD}");
    assert_eq!(repository.pre_commit()?, refused(&[finding]));
    Ok(())
}

#[test]
fn a_listed_word_is_a_regular_expression_read_by_one_engine() -> Result<(), Failure> {
    let repository = TestRepository::create()?;
    write(&repository.root().join(LISTED_WORDS), "release\\d+\n")?;
    repository.stage("notes.rs", "const NAME: u8 = RELEASE42;\n")?;
    let finding = format!("notes.rs:1: {LISTED_WORD_IN_LINE}: const NAME: u8 = RELEASE42;");
    assert_eq!(repository.pre_commit()?, refused(&[finding]));
    Ok(())
}

#[test]
fn a_binary_file_is_not_searched() -> Result<(), Failure> {
    let repository = TestRepository::create()?;
    repository.stage("image.bin", "brandname\0\n")?;
    assert_eq!(repository.pre_commit()?, passed());
    Ok(())
}

#[test]
fn a_worktree_reads_the_word_list_of_the_repository() -> Result<(), Failure> {
    let repository = TestRepository::create()?;
    let worktree = repository.directory.path().join("worktree");
    git(
        &repository.root(),
        &["worktree", "add", "--quiet", "../worktree"],
    )?;
    write(&worktree.join("named.rs"), WITH_LISTED_WORD)?;
    git(&worktree, &["add", "named.rs"])?;
    let run = repository.run_hook(&worktree, &["pre-commit"], SOME_EDITOR, "")?;
    let finding = format!("named.rs:1: {LISTED_WORD_IN_LINE}: {WITH_LISTED_WORD}");
    assert_eq!(run, refused(&[finding]));
    Ok(())
}

#[test]
fn a_word_list_that_the_hook_cannot_use_stops_it_with_a_next_step() -> Result<(), Failure> {
    let repository = TestRepository::create()?;
    let list = repository.root().join(LISTED_WORDS);
    let assert_stopped = |next_step: &str| -> Result<(), Failure> {
        let run = repository.pre_commit()?;
        assert_eq!(run.code, Some(2), "{run:?}");
        assert!(
            run.messages.ends_with(next_step),
            "{STARTS_WITH_TEMPORARY_PATH}: {run:?}"
        );
        Ok(())
    };
    for unusable in ["\n", "(\n"] {
        write(&list, unusable)?;
        assert_stopped(WRITE_WORD_LIST)?;
    }
    std::fs::remove_file(&list).map_err(Failure::Write)?;
    assert_stopped(WRITE_WORD_LIST)?;
    std::fs::create_dir(&list).map_err(Failure::Write)?;
    assert_stopped(MAKE_WORD_LIST_READABLE)
}

#[test]
fn a_push_checks_only_the_commits_missing_on_the_remote() -> Result<(), Failure> {
    let repository = TestRepository::create()?;
    let remote = repository.directory.path().join("remote.git");
    std::fs::create_dir(&remote).map_err(Failure::Write)?;
    git(&remote, &["init", "--quiet", "--bare"])?;
    let root = repository.root();
    git(&root, &["remote", "add", "origin", "../remote.git"])?;
    repository.commit("old.rs", WITH_LISTED_WORD, "add old")?;
    git(&root, &["push", "--quiet", "origin", "main"])?;
    repository.commit("new.rs", "fn new() {}\n", "add new")?;
    assert_eq!(repository.push_head()?, passed());
    repository.commit("bad.rs", WITH_LISTED_WORD, "add bad\n\nbody")?;
    let bad = repository.head()?;
    assert_eq!(
        repository.push_head()?,
        refused(&[
            format!("{bad:.12}: {NOT_ONE_TITLE}"),
            listed_line(&bad, "bad.rs"),
        ])
    );
    Ok(())
}

#[test]
fn a_push_to_a_location_checks_only_the_commits_after_the_replaced_one() -> Result<(), Failure> {
    let repository = TestRepository::create()?;
    repository.commit("old.rs", WITH_LISTED_WORD, "add old")?;
    let old = repository.head()?;
    repository.commit("new.rs", "fn new() {}\n", "add new")?;
    let head = repository.head()?;
    let push_replacing = |replaced: &str| {
        repository.pre_push(
            "../remote.git",
            &format!("refs/heads/main {head} refs/heads/main {replaced}\n"),
        )
    };
    assert_eq!(push_replacing(&old)?, passed());
    for replaced in [ABSENT_OBJECT, OBJECT_MISSING_HERE] {
        assert_eq!(
            push_replacing(replaced)?,
            refused(&[listed_line(&old, "old.rs")])
        );
    }
    Ok(())
}

#[test]
fn a_push_checks_only_the_refs_that_it_updates() -> Result<(), Failure> {
    let repository = TestRepository::create()?;
    let clean = repository.head()?;
    repository.commit("bad.rs", WITH_LISTED_WORD, "add bad")?;
    let bad = repository.head()?;
    let deleted = format!("(delete) {ABSENT_OBJECT} refs/heads/old {bad}\n");
    let updated = format!("refs/heads/new {clean} refs/heads/new {ABSENT_OBJECT}\n");
    for pushed_refs in [String::new(), deleted.clone(), deleted + &updated] {
        assert_eq!(repository.pre_push("origin", &pushed_refs)?, passed());
    }
    Ok(())
}

#[test]
fn a_commit_under_two_pushed_refs_is_reported_once() -> Result<(), Failure> {
    let repository = TestRepository::create()?;
    repository.commit("bad.rs", WITH_LISTED_WORD, "add bad")?;
    let bad = repository.head()?;
    let pushed_refs = ["refs/heads/main", "refs/heads/other"]
        .map(|name| format!("{name} {bad} {name} {ABSENT_OBJECT}\n"))
        .concat();
    assert_eq!(
        repository.pre_push("origin", &pushed_refs)?,
        refused(&[listed_line(&bad, "bad.rs")])
    );
    Ok(())
}

#[test]
fn a_pushed_ref_line_without_four_fields_stops_the_hook() -> Result<(), Failure> {
    let repository = TestRepository::create()?;
    let head = repository.head()?;
    for line in [
        format!("refs/heads/main {head} refs/heads/main"),
        format!("refs/heads/main {head} refs/heads/main {ABSENT_OBJECT} more"),
    ] {
        let stopped = HookRun {
            code: Some(2),
            messages: format!(
                "the line {line:?} of the pushed refs does not have four fields\n{REPORT_DEFECT}\n"
            ),
        };
        assert_eq!(
            repository.pre_push("origin", &format!("{line}\n"))?,
            stopped
        );
    }
    Ok(())
}

#[test]
fn a_pushed_file_name_with_a_listed_word_is_refused() -> Result<(), Failure> {
    let repository = TestRepository::create()?;
    repository.commit("brandname.rs", "fn a() {}\n", "add a file")?;
    let added = repository.head()?;
    let root = repository.root();
    git(&root, &["mv", "plain.rs", "plain_brandname.rs"])?;
    git(&root, &["commit", "--quiet", "--message", "rename plain"])?;
    let renamed = repository.head()?;
    assert_eq!(
        repository.push_head()?,
        refused(&[
            format!("{renamed:.12}: plain_brandname.rs: {LISTED_WORD_IN_NAME}"),
            format!("{added:.12}: brandname.rs: {LISTED_WORD_IN_NAME}"),
        ])
    );
    Ok(())
}

#[test]
fn a_first_commit_is_checked() -> Result<(), Failure> {
    let repository = TestRepository::create()?;
    git(
        &repository.root(),
        &["checkout", "--quiet", "--orphan", "other"],
    )?;
    repository.commit("bad.rs", WITH_LISTED_WORD, "add bad")?;
    let first = repository.head()?;
    assert_eq!(
        repository.push_head()?,
        refused(&[listed_line(&first, "bad.rs")])
    );
    Ok(())
}

#[test]
fn the_settings_of_the_user_do_not_change_what_the_hook_reads() -> Result<(), Failure> {
    let repository = TestRepository::create()?;
    for (key, value) in [
        ("color.ui", "always"),
        ("log.showSignature", "true"),
        ("diff.renames", "copies"),
        ("diff.relative", "true"),
    ] {
        git(&repository.root(), &["config", key, value])?;
    }
    repository.commit("bad.rs", WITH_LISTED_WORD, "add bad\n\nbody")?;
    let bad = repository.head()?;
    assert_eq!(
        repository.push_head()?,
        refused(&[
            format!("{bad:.12}: {NOT_ONE_TITLE}"),
            listed_line(&bad, "bad.rs"),
        ])
    );
    repository.stage("named.rs", WITH_LISTED_WORD)?;
    let finding = format!("named.rs:1: {LISTED_WORD_IN_LINE}: {WITH_LISTED_WORD}");
    assert_eq!(repository.pre_commit()?, refused(&[finding]));
    Ok(())
}
