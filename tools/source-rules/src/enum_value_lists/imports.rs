// SPDX-License-Identifier: GPL-3.0-only
use std::collections::BTreeMap;
use syn::visit::{self, Visit};
use syn::{ItemType, ItemUse, Type, UseTree};

#[derive(Default)]
pub struct Imports {
    named: BTreeMap<String, Vec<String>>,
    glob_items: Vec<String>,
}

impl Imports {
    pub fn read(file: &syn::File) -> Self {
        let mut imports = Self::default();
        imports.visit_file(file);
        imports
    }

    pub fn path_named_by(&self, name: &str) -> Vec<String> {
        self.named
            .get(name)
            .cloned()
            .unwrap_or_else(|| vec![name.to_owned()])
    }

    pub fn glob_items(&self) -> impl Iterator<Item = &str> {
        self.glob_items.iter().map(String::as_str)
    }

    fn record(&mut self, prefix: &[String], tree: &UseTree) {
        let joined = |last: &syn::Ident| [prefix, &[last.to_string()]].concat();
        match tree {
            UseTree::Path(path) => self.record(&joined(&path.ident), &path.tree),
            UseTree::Name(name) => self
                .named
                .extend([(name.ident.to_string(), joined(&name.ident))]),
            UseTree::Rename(rename) => self
                .named
                .extend([(rename.rename.to_string(), joined(&rename.ident))]),
            UseTree::Glob(_) => self.glob_items.extend(prefix.last().cloned()),
            UseTree::Group(group) => {
                for tree in &group.items {
                    self.record(prefix, tree);
                }
            }
        }
    }
}

impl<'ast> Visit<'ast> for Imports {
    fn visit_item_use(&mut self, item: &'ast ItemUse) {
        self.record(&[], &item.tree);
    }

    fn visit_item_type(&mut self, item: &'ast ItemType) {
        if let Type::Path(aliased) = &*item.ty {
            let path = aliased.path.segments.iter();
            self.named.extend([(
                item.ident.to_string(),
                path.map(|segment| segment.ident.to_string()).collect(),
            )]);
        }
        visit::visit_item_type(self, item);
    }
}
