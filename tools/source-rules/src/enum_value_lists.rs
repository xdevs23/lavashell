// SPDX-License-Identifier: GPL-3.0-only
mod imports;

use crate::error::Error;
use crate::findings::{Finding, Rule};
use crate::modules::{Source, Sources};
use crate::rules::contains_template_variable;
use imports::Imports;
use proc_macro2::{Delimiter, Span, TokenStream, TokenTree};
use std::collections::{BTreeMap, BTreeSet};
use syn::parse::{Parse, ParseStream, Parser};
use syn::punctuated::Punctuated;
use syn::token::Brace;
use syn::visit::{self, Visit};
use syn::{
    Attribute, Expr, ExprArray, Ident, ItemEnum, ItemImpl, ItemTrait, Macro, MacroDelimiter, Token,
    Type, braced,
};

type Variants = BTreeSet<String>;

pub fn hand_written_value_lists(sources: &Sources) -> Result<Vec<Finding>, Error> {
    let mut declarations = EnumDeclarations::default();
    for source in sources.values() {
        declarations.read(source)?;
    }
    Ok(sources
        .values()
        .flat_map(|source| {
            let mut lists = ValueLists {
                declarations: &declarations,
                imports: Imports::read(&source.syntax),
                impl_type: None,
                lines: Vec::new(),
            };
            lists.visit_file(&source.syntax);
            lists.lines.into_iter().map(|(line, enum_name)| Finding {
                path: source.path.clone(),
                line: Some(line),
                rule: Rule::HandWrittenEnumValues { enum_name },
            })
        })
        .collect())
}

#[derive(Default)]
struct EnumDeclarations {
    enums: BTreeMap<String, Vec<Variants>>,
    unparsed: Option<syn::Error>,
}

impl EnumDeclarations {
    fn read(&mut self, source: &Source) -> Result<(), Error> {
        self.visit_file(&source.syntax);
        self.unparsed.take().map_or(Ok(()), |error| {
            Err(Error::ParseSource {
                path: source.path.clone(),
                source: error,
            })
        })
    }

    fn record<'variant>(&mut self, name: &Ident, variants: impl Iterator<Item = &'variant Ident>) {
        self.enums
            .entry(name.to_string())
            .or_default()
            .push(variants.map(Ident::to_string).collect());
    }

    fn declares(&self, enum_name: &str, variants: &Variants) -> bool {
        self.enums
            .get(enum_name)
            .is_some_and(|declared| declared.contains(variants))
    }

    fn declares_variant(&self, enum_name: &str, variant: &str) -> bool {
        self.enums
            .get(enum_name)
            .is_some_and(|declared| declared.iter().any(|variants| variants.contains(variant)))
    }
}

impl<'ast> Visit<'ast> for EnumDeclarations {
    fn visit_item_enum(&mut self, item: &'ast ItemEnum) {
        self.record(
            &item.ident,
            item.variants.iter().map(|variant| &variant.ident),
        );
        visit::visit_item_enum(self, item);
    }

    fn visit_macro(&mut self, invocation: &'ast Macro) {
        match invocation.parse_body_with(declared_enum) {
            Ok(Some(declared)) => {
                self.record(
                    &declared.name,
                    declared.variants.iter().map(|variant| &variant.0),
                );
            }
            Ok(None) => {}
            Err(_) if contains_template_variable(invocation.tokens.clone()) => {}
            Err(error) => match &mut self.unparsed {
                Some(first) => first.combine(error),
                None => self.unparsed = Some(error),
            },
        }
        visit::visit_macro(self, invocation);
    }
}

struct DeclaredEnum {
    name: Ident,
    variants: Punctuated<DeclaredVariant, Token![,]>,
}

struct DeclaredVariant(Ident);

fn declared_enum(input: ParseStream<'_>) -> syn::Result<Option<DeclaredEnum>> {
    while !(input.peek(Token![enum]) && input.peek2(Ident)) {
        if input.is_empty() {
            return Ok(None);
        }
        skip_token(input)?;
    }
    let _enum_keyword: Token![enum] = input.parse()?;
    let name = input.parse()?;
    while !input.peek(Brace) {
        skip_token(input)?;
    }
    let body;
    let _braces = braced!(body in input);
    let variants = body.parse_terminated(DeclaredVariant::parse, Token![,])?;
    while !input.is_empty() {
        skip_token(input)?;
    }
    Ok(Some(DeclaredEnum { name, variants }))
}

impl Parse for DeclaredVariant {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let _attributes = input.call(Attribute::parse_outer)?;
        let name = input.parse()?;
        while !input.is_empty() && !input.peek(Token![,]) {
            skip_token(input)?;
        }
        Ok(Self(name))
    }
}

fn skip_token(input: ParseStream<'_>) -> syn::Result<()> {
    let _skipped: TokenTree = input.parse()?;
    Ok(())
}

struct ValueLists<'declarations> {
    declarations: &'declarations EnumDeclarations,
    imports: Imports,
    impl_type: Option<String>,
    lines: Vec<(usize, String)>,
}

impl ValueLists<'_> {
    fn check_list<'element>(
        &mut self,
        opening: Span,
        elements: impl IntoIterator<Item = &'element Expr>,
    ) {
        if let Some(enum_name) = self.enum_listed_whole(elements) {
            self.lines.push((opening.start().line, enum_name));
        }
    }

    fn check_list_tokens(&mut self, opening: Span, tokens: TokenStream) {
        if let Ok(elements) = Punctuated::<Expr, Token![,]>::parse_terminated.parse2(tokens) {
            self.check_list(opening, &elements);
        }
    }

    fn check_bracket_groups(&mut self, tokens: TokenStream) {
        for token in tokens {
            if let TokenTree::Group(group) = token {
                if group.delimiter() == Delimiter::Bracket {
                    self.check_list_tokens(group.span_open(), group.stream());
                }
                self.check_bracket_groups(group.stream());
            }
        }
    }

    fn enum_listed_whole<'element>(
        &self,
        elements: impl IntoIterator<Item = &'element Expr>,
    ) -> Option<String> {
        let named = (elements.into_iter())
            .map(|element| self.variant_named_by(element))
            .collect::<Option<Vec<_>>>()?;
        let enum_name = &named.first()?.0;
        let variants: Variants = named.iter().map(|(_, variant)| variant.clone()).collect();
        let one_enum = named.iter().all(|(name, _)| name == enum_name);
        let each_variant_once = variants.len() == named.len();
        (one_enum && each_variant_once && self.declarations.declares(enum_name, &variants))
            .then(|| enum_name.clone())
    }

    fn variant_named_by(&self, element: &Expr) -> Option<(String, String)> {
        let Expr::Path(path) = element else {
            return None;
        };
        let mut segments = path
            .path
            .segments
            .iter()
            .map(|segment| segment.ident.to_string());
        let first = segments.next()?;
        let start = if first == "Self" {
            vec![self.impl_type.clone()?]
        } else {
            self.imports.path_named_by(&first)
        };
        let resolved: Vec<String> = start.into_iter().chain(segments).collect();
        match resolved.as_slice() {
            [variant] => Some((self.glob_enum_declaring(variant)?, variant.clone())),
            [.., enum_name, variant] => Some((enum_name.clone(), variant.clone())),
            [] => None,
        }
    }

    fn glob_enum_declaring(&self, variant: &str) -> Option<String> {
        let mut enums = (self.imports.glob_items())
            .filter(|enum_name| self.declarations.declares_variant(enum_name, variant))
            .collect::<BTreeSet<_>>()
            .into_iter();
        match (enums.next(), enums.next()) {
            (Some(only), None) => Some(only.to_owned()),
            _ => None,
        }
    }
}

impl<'ast> Visit<'ast> for ValueLists<'_> {
    fn visit_item_impl(&mut self, item: &'ast ItemImpl) {
        let implemented = match &*item.self_ty {
            Type::Path(path) => path.path.segments.last().map(|last| last.ident.to_string()),
            _ => None,
        };
        let enclosing = std::mem::replace(&mut self.impl_type, implemented);
        visit::visit_item_impl(self, item);
        self.impl_type = enclosing;
    }

    fn visit_item_trait(&mut self, item: &'ast ItemTrait) {
        let enclosing = self.impl_type.take();
        visit::visit_item_trait(self, item);
        self.impl_type = enclosing;
    }

    fn visit_expr_array(&mut self, array: &'ast ExprArray) {
        self.check_list(array.bracket_token.span.open(), &array.elems);
        visit::visit_expr_array(self, array);
    }

    fn visit_macro(&mut self, invocation: &'ast Macro) {
        if let MacroDelimiter::Bracket(brackets) = &invocation.delimiter {
            self.check_list_tokens(brackets.span.open(), invocation.tokens.clone());
        }
        self.check_bracket_groups(invocation.tokens.clone());
        visit::visit_macro(self, invocation);
    }
}
