// SPDX-License-Identifier: GPL-3.0-only
use crate::attributes::{CodeKind, code_kind};
use crate::checks::CrateRule;
use crate::error::Error;
use crate::findings::{Finding, Rule};
use crate::modules::Crate;
use proc_macro2::{Literal, TokenStream, TokenTree};
use quote::ToTokens;
use std::path::Path;
use syn::visit::{self, Visit};
use syn::{
    Attribute, Expr, ExprLit, ImplItemConst, ImplItemFn, Item, ItemConst, ItemFn, ItemImpl,
    ItemMod, Lit, LitStr,
};

const PROBE_FAMILY: &str = "PROBE_FAMILY";
const TEST_SURFACE: &str = "TestSurface";

pub const RENDERER_TEST_RULES: CrateRule = CrateRule {
    root: "crates/lavashell-renderer/src/lib.rs",
    findings: renderer_test_findings,
};

fn renderer_test_findings(crate_files: &Crate<'_>) -> Result<Vec<Finding>, Error> {
    let (probe_file, family) = only_definition(crate_files, PROBE_FAMILY, |item| match item {
        Item::Const(constant) if constant.ident == PROBE_FAMILY => string_value(&constant.expr),
        _ => None,
    })?;
    let (surface_file, ()) = only_definition(crate_files, TEST_SURFACE, |item| {
        matches!(item, Item::Struct(surface) if surface.ident == TEST_SURFACE).then_some(())
    })?;
    let root = std::iter::once((crate_files.root, false));
    let modules = (crate_files.modules.iter()).map(|module| (module.source, module.test_only));
    let mut findings = Vec::new();
    for (source, test_only) in root.chain(modules) {
        let mut code = TestCode {
            forbidden_family: (source.path != probe_file).then_some(family.as_str()),
            pixel_size_allowed: source.path == surface_file,
            lines: Vec::new(),
            unparsed_condition: None,
        };
        if test_only {
            code.scan(source.syntax.to_token_stream());
        } else {
            code.visit_file(&source.syntax);
        }
        if let Some(error) = code.unparsed_condition {
            return Err(Error::ParseSource {
                path: source.path.clone(),
                source: error,
            });
        }
        findings.extend(code.lines.into_iter().map(|(line, rule)| Finding {
            path: source.path.clone(),
            line: Some(line),
            rule,
        }));
    }
    Ok(findings)
}

fn only_definition<'files, T>(
    crate_files: &'files Crate<'_>,
    name: &'static str,
    read: impl Fn(&Item) -> Option<T>,
) -> Result<(&'files Path, T), Error> {
    let mut definitions = crate_files.sources().flat_map(|source| {
        (source.syntax.items.iter())
            .filter_map(&read)
            .map(|value| (source.path.as_path(), value))
    });
    match (definitions.next(), definitions.next()) {
        (Some(definition), None) => Ok(definition),
        _ => Err(Error::Definition {
            root: crate_files.root.path.clone(),
            name,
        }),
    }
}

fn string_value(expression: &Expr) -> Option<String> {
    match expression {
        Expr::Lit(ExprLit {
            lit: Lit::Str(text),
            ..
        }) => Some(text.value()),
        _ => None,
    }
}

struct TestCode<'family> {
    forbidden_family: Option<&'family str>,
    pixel_size_allowed: bool,
    lines: Vec<(usize, Rule)>,
    unparsed_condition: Option<syn::Error>,
}

impl TestCode<'_> {
    fn visit_marked<'ast, T: ToTokens>(
        &mut self,
        attributes: &[Attribute],
        item: &'ast T,
        walk: fn(&mut Self, &'ast T),
    ) {
        match code_kind(attributes) {
            Ok(CodeKind::Test) => self.scan(item.to_token_stream()),
            Ok(CodeKind::Production) => walk(self, item),
            Err(error) => match &mut self.unparsed_condition {
                Some(first) => first.combine(error),
                None => self.unparsed_condition = Some(error),
            },
        }
    }

    fn scan(&mut self, tokens: TokenStream) {
        let tokens: Vec<_> = tokens.into_iter().collect();
        for (index, token) in tokens.iter().enumerate() {
            match token {
                TokenTree::Group(group) => self.scan(group.stream()),
                TokenTree::Literal(literal) => {
                    if let Some(family) = self.forbidden_family
                        && is_string(literal, family)
                    {
                        let rule = Rule::ProbeFamilyLiteral {
                            family: family.to_owned(),
                        };
                        self.lines.push((literal.span().start().line, rule));
                    }
                }
                TokenTree::Ident(name)
                    if !self.pixel_size_allowed
                        && name == "PixelSize"
                        && is_new(&tokens[index + 1..]) =>
                {
                    self.lines
                        .push((name.span().start().line, Rule::PixelSizeInTest));
                }
                _ => {}
            }
        }
    }
}

impl<'ast> Visit<'ast> for TestCode<'_> {
    fn visit_item_mod(&mut self, item: &'ast ItemMod) {
        self.visit_marked(&item.attrs, item, visit::visit_item_mod);
    }

    fn visit_item_fn(&mut self, item: &'ast ItemFn) {
        self.visit_marked(&item.attrs, item, visit::visit_item_fn);
    }

    fn visit_item_impl(&mut self, item: &'ast ItemImpl) {
        self.visit_marked(&item.attrs, item, visit::visit_item_impl);
    }

    fn visit_item_const(&mut self, item: &'ast ItemConst) {
        self.visit_marked(&item.attrs, item, visit::visit_item_const);
    }

    fn visit_impl_item_fn(&mut self, item: &'ast ImplItemFn) {
        self.visit_marked(&item.attrs, item, visit::visit_impl_item_fn);
    }

    fn visit_impl_item_const(&mut self, item: &'ast ImplItemConst) {
        self.visit_marked(&item.attrs, item, visit::visit_impl_item_const);
    }
}

fn is_string(literal: &Literal, text: &str) -> bool {
    syn::parse2::<LitStr>(literal.to_token_stream()).is_ok_and(|string| string.value() == text)
}

fn is_new(tokens: &[TokenTree]) -> bool {
    matches!(tokens, [
        TokenTree::Punct(first),
        TokenTree::Punct(second),
        TokenTree::Ident(function),
        ..
    ] if first.as_char() == ':' && second.as_char() == ':' && function == "new")
}
