// SPDX-License-Identifier: GPL-3.0-only
use syn::punctuated::Punctuated;
use syn::{Meta, MetaList, Token};

const CONDITION_COUNT: usize = 1;

pub fn configured_attributes(list: &MetaList) -> syn::Result<impl Iterator<Item = Meta>> {
    Ok(list
        .parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)?
        .into_iter()
        .skip(CONDITION_COUNT))
}
