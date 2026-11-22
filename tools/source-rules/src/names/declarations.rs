// SPDX-License-Identifier: GPL-3.0-only
use super::{EnumKey, ModuleKey, RepositoryNames, name_of};
use crate::error::Error;
use crate::metadata::ExternPreludes;
use crate::modules::{Crate, Source, Sources};
use crate::rules::contains_template_variable;
use proc_macro2::TokenTree;
use std::collections::BTreeSet;
use std::path::Path;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::token::Brace;
use syn::visit::{self, Visit};
use syn::{Attribute, Ident, ItemEnum, ItemMod, Macro, Token, braced};

pub fn sources_by_module<'sources>(
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

impl<'libraries> RepositoryNames<'libraries> {
    pub fn read(
        extern_preludes: &'libraries ExternPreludes,
        modules: &[(ModuleKey, &Source)],
    ) -> Result<Self, Error> {
        let mut names = Self::new(extern_preludes);
        for (module, source) in modules {
            DeclarationReader {
                names: &mut names,
                module: module.clone(),
                unparsed: None,
            }
            .read(source)?;
        }
        Ok(names)
    }
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

pub struct DeclaredEnum {
    pub name: Ident,
    variants: Punctuated<DeclaredVariant, Token![,]>,
}

struct DeclaredVariant(Ident);

pub fn declared_enum(input: ParseStream<'_>) -> syn::Result<Option<DeclaredEnum>> {
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
