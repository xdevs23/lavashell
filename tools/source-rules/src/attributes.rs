// SPDX-License-Identifier: GPL-3.0-only
use syn::punctuated::Punctuated;
use syn::{Attribute, Meta, MetaList, Token};

const CONDITION_COUNT: usize = 1;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CodeKind {
    Production,
    Test,
}

pub fn configured_attributes(list: &MetaList) -> syn::Result<impl Iterator<Item = Meta>> {
    Ok(list
        .parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)?
        .into_iter()
        .skip(CONDITION_COUNT))
}

pub fn code_kind(attributes: &[Attribute]) -> syn::Result<CodeKind> {
    kind_of_required(
        (attributes.iter())
            .filter(|attribute| attribute.path().is_ident("cfg"))
            .map(Attribute::parse_args::<Meta>),
    )
}

fn kind_of_required(
    conditions: impl IntoIterator<Item = syn::Result<Meta>>,
) -> syn::Result<CodeKind> {
    for condition in conditions {
        if condition_kind(&condition?)? == CodeKind::Test {
            return Ok(CodeKind::Test);
        }
    }
    Ok(CodeKind::Production)
}

fn condition_kind(condition: &Meta) -> syn::Result<CodeKind> {
    match condition {
        Meta::Path(path) if path.is_ident("test") => Ok(CodeKind::Test),
        Meta::List(list) if list.path.is_ident("all") => kind_of_required(
            list.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)?
                .into_iter()
                .map(Ok),
        ),
        _ => Ok(CodeKind::Production),
    }
}
