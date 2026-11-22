// SPDX-License-Identifier: GPL-3.0-only
use super::{Failure, findings, safe};
use crate::calloop_errors::{CalloopError, EVENT_LOOP_ROOT, StoredCalloopError};
use crate::findings::Rule;

const WAYLAND_ROOT: &str = "crates/lavashell-wayland/src/lib.rs";
const WAYLAND_MODULE: &str = "crates/lavashell-wayland/src/stored.rs";

const STORED_ERRORS: &str = concat!(
    "use sctk::reexports::calloop::{self, InsertError};\n",
    "#[derive(Debug, thiserror::Error)]\n",
    "pub enum HostError {\n",
    "    #[error(\"the event loop failed\")]\n",
    "    EventLoop(#[from] calloop::Error),\n",
    "    #[error(\"the source was not inserted\")]\n",
    "    Insert(#[source] Box<InsertError<()>>),\n",
    "}\n",
    "pub struct Stored {\n    error: ::calloop::Error,\n}\n",
    "pub struct Modules(calloop::error::Error, calloop::error::InsertError<()>);\n",
    "pub struct Results(calloop::Result<()>, calloop::error::Result<()>);\n",
);

fn stored(path: &str, line: usize, error: CalloopError) -> (String, Option<usize>, Rule) {
    (
        path.to_owned(),
        Some(line),
        StoredCalloopError { error }.into(),
    )
}

#[test]
fn the_check_reports_a_field_typed_as_a_calloop_error_outside_the_event_loop_crate()
-> Result<(), Failure> {
    assert_eq!(
        findings(&[(WAYLAND_ROOT, &safe(STORED_ERRORS))])?,
        [
            stored(WAYLAND_ROOT, 6, CalloopError::Error),
            stored(WAYLAND_ROOT, 8, CalloopError::InsertError),
            stored(WAYLAND_ROOT, 11, CalloopError::Error),
            stored(WAYLAND_ROOT, 13, CalloopError::Error),
            stored(WAYLAND_ROOT, 13, CalloopError::InsertError),
            stored(WAYLAND_ROOT, 14, CalloopError::Result),
        ]
    );
    Ok(())
}

#[test]
fn the_event_loop_crate_may_contain_a_calloop_error() -> Result<(), Failure> {
    assert_eq!(findings(&[(EVENT_LOOP_ROOT, &safe(STORED_ERRORS))])?, []);
    Ok(())
}

#[test]
fn the_check_does_not_report_a_conversion_from_a_calloop_error_and_an_error_from_another_crate()
-> Result<(), Failure> {
    let root = safe(concat!(
        "use std::io::Error;\n",
        "pub struct Failure(Error);\n",
        "pub struct Stored(lavashell_event_loop::EventLoopFailure);\n",
        "impl From<calloop::Error> for Stored {\n",
        "    fn from(error: calloop::Error) -> Self {\n",
        "        Self(error.into())\n",
        "    }\n",
        "}\n",
    ));
    assert_eq!(findings(&[(WAYLAND_ROOT, &root)])?, []);
    Ok(())
}

#[test]
fn the_check_resolves_a_calloop_error_named_through_a_glob_import() -> Result<(), Failure> {
    let root = safe("use calloop::*;\npub struct Stored(InsertError<()>);\n");
    assert_eq!(
        findings(&[(WAYLAND_ROOT, &root)])?,
        [stored(WAYLAND_ROOT, 3, CalloopError::InsertError)]
    );
    Ok(())
}

#[test]
fn the_check_reports_a_type_alias_of_a_calloop_error_where_it_is_declared() -> Result<(), Failure> {
    let root = safe("pub type LoopResult<T> = Result<T, calloop::Error>;\n");
    assert_eq!(
        findings(&[(WAYLAND_ROOT, &root)])?,
        [stored(WAYLAND_ROOT, 2, CalloopError::Error)]
    );
    Ok(())
}

#[test]
fn a_nearer_local_item_shadows_a_calloop_error() -> Result<(), Failure> {
    let root = safe(concat!(
        "use calloop::Error;\n",
        "fn run() {\n",
        "    struct Error;\n",
        "    struct Stored(Error);\n",
        "}\n",
        "mod inner {\n",
        "    mod calloop {\n        pub struct Error;\n    }\n",
        "    pub struct Stored(calloop::Error);\n",
        "}\n",
    ));
    assert_eq!(findings(&[(WAYLAND_ROOT, &root)])?, []);
    Ok(())
}

#[test]
fn the_check_resolves_names_in_the_scope_of_each_inline_module_and_block() -> Result<(), Failure> {
    let root = safe(concat!(
        "use calloop::Error;\n",
        "mod outside {\n    pub struct Stored(Error);\n}\n",
        "mod inside {\n    use calloop::Error;\n    pub struct Stored(Error);\n}\n",
        "fn run() {\n",
        "    use calloop::InsertError;\n",
        "    struct Stored(InsertError<()>);\n",
        "}\n",
        "pub struct After(InsertError<()>);\n",
    ));
    assert_eq!(
        findings(&[(WAYLAND_ROOT, &root)])?,
        [
            stored(WAYLAND_ROOT, 8, CalloopError::Error),
            stored(WAYLAND_ROOT, 12, CalloopError::InsertError),
        ]
    );
    Ok(())
}

#[test]
fn the_check_follows_renames_and_aliases_across_modules() -> Result<(), Failure> {
    let root = safe(concat!(
        "mod stored;\n",
        "pub use calloop::Error as LoopError;\n",
        "use calloop::InsertError as Inserted;\n",
        "pub type LoopInsertError = Inserted<()>;\n",
    ));
    let module = concat!(
        "use crate::{LoopError, LoopInsertError};\n",
        "pub struct Stored(LoopError, LoopInsertError);\n",
    );
    assert_eq!(
        findings(&[(WAYLAND_ROOT, &root), (WAYLAND_MODULE, module)])?,
        [
            stored(WAYLAND_ROOT, 5, CalloopError::InsertError),
            stored(WAYLAND_MODULE, 2, CalloopError::Error),
            stored(WAYLAND_MODULE, 2, CalloopError::InsertError),
        ]
    );
    Ok(())
}
