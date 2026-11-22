// SPDX-License-Identifier: GPL-3.0-only
use crate::findings::Finding;
use crate::modules::Source;
use crate::names::{ModuleKey, Named, RepositoryNames, Scopes, path_names};
use lavashell_enum_values::enum_with_values;
use std::collections::BTreeSet;
use std::fmt;
use std::path::Path;
use std::str::FromStr;
use syn::spanned::Spanned;
use syn::visit::{self, Visit};
use syn::{Block, Field, ItemMod, ItemType, Type, TypePath};

pub const EVENT_LOOP_ROOT: &str = "crates/lavashell-event-loop/src/lib.rs";
const EVENT_LOOP_FAILURE: &str = "lavashell_event_loop::EventLoopFailure";
const CALLOOP: &str = "calloop";
const CALLOOP_ERROR_MODULE: &str = "error";

enum_with_values! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
    pub enum CalloopError {
        Error,
        InsertError,
        Result,
    }

    const ALL;
}

#[derive(Debug)]
pub struct UnknownCalloopError;

impl CalloopError {
    fn name(self) -> &'static str {
        match self {
            Self::Error => "Error",
            Self::InsertError => "InsertError",
            Self::Result => "Result",
        }
    }

    fn wrapped_error(self) -> Self {
        match self {
            Self::Error | Self::Result => Self::Error,
            Self::InsertError => Self::InsertError,
        }
    }
}

impl FromStr for CalloopError {
    type Err = UnknownCalloopError;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        (Self::ALL.into_iter())
            .find(|error| error.name() == name)
            .ok_or(UnknownCalloopError)
    }
}

impl fmt::Display for CalloopError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{CALLOOP}::{}", self.name())
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct StoredCalloopError {
    pub error: CalloopError,
}

impl fmt::Display for StoredCalloopError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let error = self.error.wrapped_error();
        write!(
            formatter,
            "convert the `{error}` into `{EVENT_LOOP_FAILURE}` where it arrives and store that. The message of `{error}` repeats the cause the error wraps",
        )
    }
}

pub fn find_stored_calloop_errors(
    names: &RepositoryNames<'_>,
    modules: &[(ModuleKey, &Source)],
) -> Vec<Finding> {
    let lines: BTreeSet<(&Path, usize, CalloopError)> = (modules.iter())
        .filter(|(module, _)| !module.is_in_crate(EVENT_LOOP_ROOT))
        .flat_map(|(module, source)| {
            let mut types = CalloopErrorTypes {
                scopes: names.scopes_of(module),
                lines: Vec::new(),
            };
            types.visit_file(&source.syntax);
            (types.lines.into_iter()).map(|(line, error)| (source.path.as_path(), line, error))
        })
        .collect();
    (lines.into_iter())
        .map(|(path, line, error)| Finding {
            path: path.to_path_buf(),
            line: Some(line),
            rule: StoredCalloopError { error }.into(),
        })
        .collect()
}

#[derive(Default)]
struct TypePaths<'ast>(Vec<&'ast TypePath>);

impl<'ast> Visit<'ast> for TypePaths<'ast> {
    fn visit_type_path(&mut self, path: &'ast TypePath) {
        self.0.push(path);
        visit::visit_type_path(self, path);
    }
}

struct CalloopErrorTypes<'repository> {
    scopes: Scopes<'repository>,
    lines: Vec<(usize, CalloopError)>,
}

impl CalloopErrorTypes<'_> {
    fn check_type(&mut self, checked: &Type) {
        let mut paths = TypePaths::default();
        paths.visit_type(checked);
        for path in paths.0 {
            if let Some(error) = self.calloop_error(path) {
                self.lines.push((path.span().start().line, error));
            }
        }
    }

    fn calloop_error(&self, path: &TypePath) -> Option<CalloopError> {
        let Named::External(resolved) = self.scopes.resolve(&path_names(&path.path), None) else {
            return None;
        };
        let segments: Vec<&str> = resolved.iter().map(String::as_str).collect();
        match segments.as_slice() {
            [.., CALLOOP, name] | [.., CALLOOP, CALLOOP_ERROR_MODULE, name] => name.parse().ok(),
            _ => None,
        }
    }
}

impl<'ast> Visit<'ast> for CalloopErrorTypes<'_> {
    fn visit_item_mod(&mut self, item: &'ast ItemMod) {
        let module_scopes = self.scopes.child_module(&item.ident);
        let enclosing = std::mem::replace(&mut self.scopes, module_scopes);
        visit::visit_item_mod(self, item);
        self.scopes = enclosing;
    }

    fn visit_block(&mut self, block: &'ast Block) {
        self.scopes.enter_block(block);
        visit::visit_block(self, block);
        self.scopes.leave_block();
    }

    fn visit_field(&mut self, field: &'ast Field) {
        self.check_type(&field.ty);
        visit::visit_field(self, field);
    }

    fn visit_item_type(&mut self, alias: &'ast ItemType) {
        self.check_type(&alias.ty);
        visit::visit_item_type(self, alias);
    }
}
