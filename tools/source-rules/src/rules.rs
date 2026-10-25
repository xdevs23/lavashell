// SPDX-License-Identifier: GPL-3.0-only
//! The source rules that neither rustc nor clippy check.

use crate::modules::{Crate, Source, configured_attributes};
use proc_macro2::{TokenStream, TokenTree};
use quote::ToTokens;
use std::fmt;
use std::path::PathBuf;
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::visit::{self, Visit};
use syn::{
    AttrStyle, Attribute, Expr, ExprCall, GenericArgument, Lit, Local, Meta, Pat, PathArguments,
    Token, Type, TypeParamBound,
};

#[derive(Debug, PartialEq, Eq)]
pub enum Rule {
    WildcardBinding,
    SilencedLint,
    BoolOutcome,
    MissingForbid,
    IgnoredTest,
    PanickingTest,
    ErasedError,
}

impl fmt::Display for Rule {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::WildcardBinding => "bind the value to a name that says why it is not needed, never to `_`",
            Self::SilencedLint => "never allow, expect or lower a lint with an attribute, fix what it reports",
            Self::BoolOutcome => {
                "report the outcome as an enum named for its meanings or as an error variant, never as `Ok(bool)`"
            }
            Self::MissingForbid => "the crate has no unsafe code and must start with `#![forbid(unsafe_code)]`",
            Self::IgnoredTest => {
                "run the test and fail it where it lacks a capability, never mark it `#[ignore]`"
            }
            Self::PanickingTest => {
                "match the error that the code returns, never mark the test `#[should_panic]`"
            }
            Self::ErasedError => {
                "report a failure as the error enum of the crate, never as a `String` or a boxed `dyn Error`"
            }
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    pub line: usize,
    pub rule: Rule,
}

/// The findings of the rules that apply to every file.
pub fn file_findings(source: &Source) -> syn::Result<Vec<Finding>> {
    let mut visitor = FileRules::default();
    visitor.visit_file(&source.syntax);
    if let Some(error) = visitor.error {
        return Err(error);
    }
    Ok(visitor
        .lines
        .into_iter()
        .map(|(line, rule)| Finding {
            path: source.path.clone(),
            line,
            rule,
        })
        .collect())
}

/// The finding for a crate root that does not forbid unsafe code although no file of the crate contains it.
pub fn missing_forbid(crate_files: &Crate) -> syn::Result<Option<Finding>> {
    if crate_files
        .sources()
        .any(|source| has_unsafe(source.syntax.to_token_stream()))
        || forbidden_lints(&crate_files.root.syntax)?
            .iter()
            .any(|lint| lint.is_ident("unsafe_code"))
    {
        return Ok(None);
    }
    Ok(Some(Finding {
        path: crate_files.root.path.clone(),
        line: 1,
        rule: Rule::MissingForbid,
    }))
}

/// Whether the tokens contain the keyword `unsafe`. The word inside a comment or a string literal does not count,
/// because it is not an identifier token.
fn has_unsafe(tokens: TokenStream) -> bool {
    tokens.into_iter().any(|token| match token {
        TokenTree::Ident(ident) => ident == "unsafe",
        TokenTree::Group(group) => has_unsafe(group.stream()),
        TokenTree::Punct(_) | TokenTree::Literal(_) => false,
    })
}

/// The lints listed in the inner `forbid` attributes of the file.
fn forbidden_lints(file: &syn::File) -> syn::Result<Vec<syn::Path>> {
    let mut lints = Vec::new();
    for attribute in &file.attrs {
        if matches!(attribute.style, AttrStyle::Inner(_)) && attribute.path().is_ident("forbid") {
            lints.extend(
                attribute.parse_args_with(Punctuated::<syn::Path, Token![,]>::parse_terminated)?,
            );
        }
    }
    Ok(lints)
}

/// Whether `ty` is the plain type `name`.
fn is_named(ty: &Type, name: &str) -> bool {
    matches!(ty, Type::Path(path) if path.path.is_ident(name))
}

/// Whether `ty` is a trait object that has a trait named `Error` among its bounds.
fn is_error_object(ty: &Type) -> bool {
    let Type::TraitObject(object) = ty else {
        return false;
    };
    object.bounds.iter().any(|bound| {
        matches!(bound, TypeParamBound::Trait(bound)
            if bound.path.segments.last().is_some_and(|last| last.ident == "Error"))
    })
}

#[derive(Default)]
struct FileRules {
    lines: Vec<(usize, Rule)>,
    error: Option<syn::Error>,
}

impl FileRules {
    fn report(&mut self, span: proc_macro2::Span, rule: Rule) {
        self.lines.push((span.start().line, rule));
    }

    fn check_meta(&mut self, meta: &Meta, span: proc_macro2::Span) {
        let path = meta.path();
        if ["allow", "expect", "warn"]
            .iter()
            .any(|level| path.is_ident(level))
        {
            self.report(span, Rule::SilencedLint);
        } else if path.is_ident("ignore") {
            self.report(span, Rule::IgnoredTest);
        } else if path.is_ident("should_panic") {
            self.report(span, Rule::PanickingTest);
        } else if let Meta::List(list) = meta
            && path.is_ident("cfg_attr")
        {
            match configured_attributes(list) {
                Ok(attributes) => {
                    for attribute in attributes {
                        self.check_meta(&attribute, span);
                    }
                }
                Err(error) => match &mut self.error {
                    Some(first) => first.combine(error),
                    None => self.error = Some(error),
                },
            }
        }
    }
}

impl<'ast> Visit<'ast> for FileRules {
    fn visit_local(&mut self, local: &'ast Local) {
        let pattern = match &local.pat {
            Pat::Type(typed) => &*typed.pat,
            pattern => pattern,
        };
        if matches!(pattern, Pat::Wild(_)) {
            self.report(local.let_token.span, Rule::WildcardBinding);
        }
        visit::visit_local(self, local);
    }

    fn visit_attribute(&mut self, attribute: &'ast Attribute) {
        self.check_meta(&attribute.meta, attribute.pound_token.span);
        visit::visit_attribute(self, attribute);
    }

    fn visit_type_path(&mut self, path: &'ast syn::TypePath) {
        if let Some(last) = path.path.segments.last()
            && let PathArguments::AngleBracketed(arguments) = &last.arguments
        {
            let mut types = arguments.args.iter().filter_map(|argument| match argument {
                GenericArgument::Type(ty) => Some(ty),
                _ => None,
            });
            let (first, second) = (types.next(), types.next());
            if last.ident == "Result" {
                if first.is_some_and(|ty| is_named(ty, "bool")) {
                    self.report(path.span(), Rule::BoolOutcome);
                }
                if second.is_some_and(|ty| is_named(ty, "String")) {
                    self.report(path.span(), Rule::ErasedError);
                }
            } else if last.ident == "Box" && first.is_some_and(is_error_object) {
                self.report(path.span(), Rule::ErasedError);
            }
        }
        visit::visit_type_path(self, path);
    }

    fn visit_expr_call(&mut self, call: &'ast ExprCall) {
        if let Expr::Path(function) = &*call.func
            && function.path.is_ident("Ok")
            && call.args.len() == 1
            && let Some(Expr::Lit(argument)) = call.args.first()
            && matches!(argument.lit, Lit::Bool(_))
        {
            self.report(call.span(), Rule::BoolOutcome);
        }
        visit::visit_expr_call(self, call);
    }
}
