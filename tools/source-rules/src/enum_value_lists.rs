// SPDX-License-Identifier: GPL-3.0-only
mod names;

use crate::error::Error;
use crate::findings::{Finding, Rule};
use crate::metadata::ExternPreludes;
use crate::modules::{Crate, Source, Sources};
use crate::rules::contains_template_variable;
use names::{EnumKey, ModuleKey, Named, RepositoryNames, Scopes, Variant, name_of, path_names};
use proc_macro2::{Delimiter, Span, TokenStream, TokenTree};
use std::collections::BTreeSet;
use std::path::Path;
use syn::parse::{Parse, ParseStream, Parser};
use syn::punctuated::Punctuated;
use syn::token::Brace;
use syn::visit::{self, Visit};
use syn::{
    Attribute, Block, Expr, ExprArray, Ident, ItemEnum, ItemImpl, ItemMod, ItemTrait, Macro,
    MacroDelimiter, Stmt, Token, Type, braced,
};

type Variants = BTreeSet<String>;

pub fn hand_written_value_lists(
    extern_preludes: &ExternPreludes,
    sources: &Sources,
    crates: &[Crate<'_>],
) -> Result<Vec<Finding>, Error> {
    let modules = sources_by_module(sources, crates);
    let mut names = RepositoryNames::new(extern_preludes);
    for (module, source) in &modules {
        DeclarationReader {
            names: &mut names,
            module: module.clone(),
            unparsed: None,
        }
        .read(source)?;
    }
    let lines: BTreeSet<(&Path, usize, String)> = (modules.iter())
        .flat_map(|(module, source)| {
            let mut lists = ValueLists {
                scopes: names.scopes_of(module),
                self_type: None,
                lines: Vec::new(),
            };
            lists.visit_file(&source.syntax);
            (lists.lines.into_iter())
                .map(|(line, enum_name)| (source.path.as_path(), line, enum_name))
        })
        .collect();
    Ok((lines.into_iter())
        .map(|(path, line, enum_name)| Finding {
            path: path.to_path_buf(),
            line: Some(line),
            rule: Rule::HandWrittenEnumValues { enum_name },
        })
        .collect())
}

fn sources_by_module<'sources>(
    sources: &'sources Sources,
    crates: &[Crate<'sources>],
) -> Vec<(ModuleKey, &'sources Source)> {
    let in_crates: Vec<_> = (crates.iter())
        .flat_map(|crate_files| {
            (crate_files.sources_by_module_path())
                .map(|(path, source)| (ModuleKey::new(&crate_files.root.path, path), source))
        })
        .collect();
    let in_a_crate: BTreeSet<&Path> = (in_crates.iter())
        .map(|(_, source)| source.path.as_path())
        .collect();
    let outside_crates = (sources.values())
        .filter(|source| !in_a_crate.contains(source.path.as_path()))
        .map(|source| (ModuleKey::new(&source.path, &[]), source));
    in_crates.into_iter().chain(outside_crates).collect()
}

struct DeclarationReader<'names, 'libraries> {
    names: &'names mut RepositoryNames<'libraries>,
    module: ModuleKey,
    unparsed: Option<syn::Error>,
}

impl DeclarationReader<'_, '_> {
    fn read(mut self, source: &Source) -> Result<(), Error> {
        self.names.record_module(&self.module, &source.syntax.items);
        self.visit_file(&source.syntax);
        self.unparsed.take().map_or(Ok(()), |error| {
            Err(Error::ParseSource {
                path: source.path.clone(),
                source: error,
            })
        })
    }

    fn record<'variant>(&mut self, name: &Ident, variants: impl Iterator<Item = &'variant Ident>) {
        self.names.record_enum(
            EnumKey::new(&self.module, name_of(name)),
            variants.map(name_of).collect(),
        );
    }
}

impl<'ast> Visit<'ast> for DeclarationReader<'_, '_> {
    fn visit_item_mod(&mut self, item: &'ast ItemMod) {
        let module = self.module.child(&name_of(&item.ident));
        let items = item.content.as_ref().map_or(&[][..], |(_, items)| items);
        self.names.record_module(&module, items);
        let enclosing = std::mem::replace(&mut self.module, module);
        visit::visit_item_mod(self, item);
        self.module = enclosing;
    }

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

struct ValueLists<'repository> {
    scopes: Scopes<'repository>,
    self_type: Option<Named>,
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
        let enum_key = &named.first()?.enum_key;
        let variants: Variants = named.iter().map(|variant| variant.name.clone()).collect();
        let one_enum = named.iter().all(|variant| variant.enum_key == *enum_key);
        let each_variant_once = variants.len() == named.len();
        (one_enum && each_variant_once && self.scopes.declares(enum_key, &variants))
            .then(|| enum_key.name.clone())
    }

    fn variant_named_by(&self, element: &Expr) -> Option<Variant> {
        let Expr::Path(path) = element else {
            return None;
        };
        match self
            .scopes
            .resolve(&path_names(&path.path), self.self_type.as_ref())
        {
            Named::Variant(variant) => Some(variant),
            _ => None,
        }
    }
}

impl<'ast> Visit<'ast> for ValueLists<'_> {
    fn visit_item_impl(&mut self, item: &'ast ItemImpl) {
        let implemented = match &*item.self_ty {
            Type::Path(path) => Some(self.scopes.resolve(&path_names(&path.path), None)),
            _ => None,
        };
        let enclosing = std::mem::replace(&mut self.self_type, implemented);
        visit::visit_item_impl(self, item);
        self.self_type = enclosing;
    }

    fn visit_item_mod(&mut self, item: &'ast ItemMod) {
        let module_scopes = self.scopes.child_module(&item.ident);
        let enclosing = std::mem::replace(&mut self.scopes, module_scopes);
        visit::visit_item_mod(self, item);
        self.scopes = enclosing;
    }

    fn visit_block(&mut self, block: &'ast Block) {
        let items = block.stmts.iter().filter_map(|statement| match statement {
            Stmt::Item(item) => Some(item),
            _ => None,
        });
        self.scopes.enter_block(items);
        visit::visit_block(self, block);
        self.scopes.leave_block();
    }

    fn visit_item_trait(&mut self, item: &'ast ItemTrait) {
        let enclosing = self.self_type.take();
        visit::visit_item_trait(self, item);
        self.self_type = enclosing;
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
