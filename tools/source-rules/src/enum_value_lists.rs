// SPDX-License-Identifier: GPL-3.0-only
use crate::findings::{Finding, Rule};
use crate::modules::Source;
use crate::names::{ModuleKey, Named, RepositoryNames, Scopes, Variant, Variants, path_names};
use proc_macro2::{Delimiter, Span, TokenStream, TokenTree};
use std::collections::BTreeSet;
use std::path::Path;
use syn::parse::Parser;
use syn::punctuated::Punctuated;
use syn::visit::{self, Visit};
use syn::{
    Block, Expr, ExprArray, ItemImpl, ItemMod, ItemTrait, Macro, MacroDelimiter, Token, Type,
};

pub fn hand_written_value_lists(
    names: &RepositoryNames<'_>,
    modules: &[(ModuleKey, &Source)],
) -> Vec<Finding> {
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
    (lines.into_iter())
        .map(|(path, line, enum_name)| Finding {
            path: path.to_path_buf(),
            line: Some(line),
            rule: Rule::HandWrittenEnumValues { enum_name },
        })
        .collect()
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
        self.scopes.enter_block(block);
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
