// SPDX-License-Identifier: GPL-3.0-only

use crate::attributes::configured_attributes;
use crate::findings::{Finding, Rule};
use crate::identifiers::identifiers;
use crate::modules::{Crate, Source};
use proc_macro2::{Delimiter, Span, TokenStream, TokenTree};
use quote::ToTokens;
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::visit::{self, Visit};
use syn::{
    AttrStyle, Attribute, Expr, ExprCall, GenericArgument, Lit, Local, Macro, Meta, PatWild,
    PathArguments, Token, Type, TypeParamBound,
};

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
            line: Some(line),
            rule,
        })
        .collect())
}

pub fn missing_forbid(crate_files: &Crate<'_>) -> syn::Result<Option<Finding>> {
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
        line: None,
        rule: Rule::MissingForbid,
    }))
}

fn has_unsafe(tokens: TokenStream) -> bool {
    identifiers(tokens)
        .into_iter()
        .any(|ident| ident == "unsafe")
}

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

fn is_named(ty: &Type, name: &str) -> bool {
    matches!(ty, Type::Path(path) if path.path.segments.last()
        .is_some_and(|last| last.ident == name && last.arguments.is_none()))
}

fn is_error_object(ty: &Type) -> bool {
    let Type::TraitObject(object) = ty else {
        return false;
    };
    object.bounds.iter().any(|bound| {
        matches!(bound, TypeParamBound::Trait(bound)
            if bound.path.segments.last().is_some_and(|last| last.ident == "Error"))
    })
}

fn attribute_rules(meta: &Meta) -> syn::Result<Vec<Rule>> {
    let path = meta.path();
    if ["allow", "expect", "warn"]
        .iter()
        .any(|level| path.is_ident(level))
    {
        Ok(vec![Rule::SilencedLint])
    } else if path.is_ident("ignore") {
        Ok(vec![Rule::IgnoredTest])
    } else if path.is_ident("should_panic") {
        Ok(vec![Rule::PanickingTest])
    } else if let Meta::List(list) = meta
        && path.is_ident("cfg_attr")
    {
        let mut rules = Vec::new();
        for attribute in configured_attributes(list)? {
            rules.extend(attribute_rules(&attribute)?);
        }
        Ok(rules)
    } else {
        Ok(Vec::new())
    }
}

#[derive(Default)]
struct WildcardPatterns(Vec<Span>);

impl<'ast> Visit<'ast> for WildcardPatterns {
    fn visit_pat_wild(&mut self, wildcard: &'ast PatWild) {
        self.0.push(wildcard.underscore_token.span);
    }
}

#[derive(Default)]
struct FileRules {
    lines: Vec<(usize, Rule)>,
    error: Option<syn::Error>,
}

impl FileRules {
    fn report(&mut self, span: Span, rule: Rule) {
        self.lines.push((span.start().line, rule));
    }

    fn report_attribute(&mut self, meta: &Meta, span: Span) {
        match attribute_rules(meta) {
            Ok(rules) => {
                for rule in rules {
                    self.report(span, rule);
                }
            }
            Err(error) => self.record_error(error),
        }
    }

    fn record_error(&mut self, error: syn::Error) {
        match &mut self.error {
            Some(first) => first.combine(error),
            None => self.error = Some(error),
        }
    }

    fn report_tokens(&mut self, tokens: TokenStream) {
        let tokens: Vec<_> = tokens.into_iter().collect();
        for (index, token) in tokens.iter().enumerate() {
            match token {
                TokenTree::Group(group) => self.report_tokens(group.stream()),
                TokenTree::Punct(pound) if pound.as_char() == '#' => {
                    match attribute_after(&tokens[index + 1..]) {
                        Some(Ok(meta)) => self.report_attribute(&meta, pound.span()),
                        Some(Err(error)) => self.record_error(error),
                        None => {}
                    }
                }
                TokenTree::Ident(keyword)
                    if keyword == "let" && is_underscore(tokens.get(index + 1)) =>
                {
                    self.report(keyword.span(), Rule::WildcardBinding);
                }
                TokenTree::Ident(_) | TokenTree::Punct(_) | TokenTree::Literal(_) => {}
            }
        }
    }
}

fn is_underscore(token: Option<&TokenTree>) -> bool {
    matches!(token, Some(TokenTree::Ident(ident)) if ident == "_")
}

fn attribute_after(tokens: &[TokenTree]) -> Option<syn::Result<Meta>> {
    let mut tokens = tokens.iter();
    let mut next = tokens.next()?;
    if matches!(next, TokenTree::Punct(bang) if bang.as_char() == '!') {
        next = tokens.next()?;
    }
    match next {
        TokenTree::Group(group) if group.delimiter() == Delimiter::Bracket => {
            match syn::parse2(group.stream()) {
                Err(_) if contains_template_variable(group.stream()) => None,
                parsed => Some(parsed),
            }
        }
        _ => None,
    }
}

pub fn contains_template_variable(tokens: TokenStream) -> bool {
    let tokens: Vec<_> = tokens.into_iter().collect();
    tokens.iter().enumerate().any(|(index, token)| match token {
        TokenTree::Punct(punct) => {
            punct.as_char() == '$'
                || punct.as_char() == '#' && is_quote_interpolation(tokens.get(index + 1))
        }
        TokenTree::Group(group) => contains_template_variable(group.stream()),
        TokenTree::Ident(_) | TokenTree::Literal(_) => false,
    })
}

fn is_quote_interpolation(after_pound: Option<&TokenTree>) -> bool {
    match after_pound {
        Some(TokenTree::Ident(_)) => true,
        Some(TokenTree::Group(group)) => group.delimiter() == Delimiter::Parenthesis,
        _ => false,
    }
}

impl<'ast> Visit<'ast> for FileRules {
    fn visit_local(&mut self, local: &'ast Local) {
        let mut wildcards = WildcardPatterns::default();
        wildcards.visit_pat(&local.pat);
        for span in wildcards.0 {
            self.report(span, Rule::WildcardBinding);
        }
        visit::visit_local(self, local);
    }

    fn visit_attribute(&mut self, attribute: &'ast Attribute) {
        self.report_attribute(&attribute.meta, attribute.pound_token.span);
        visit::visit_attribute(self, attribute);
    }

    fn visit_macro(&mut self, invocation: &'ast Macro) {
        self.report_tokens(invocation.tokens.clone());
        visit::visit_macro(self, invocation);
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
