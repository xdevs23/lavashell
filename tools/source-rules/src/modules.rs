// SPDX-License-Identifier: GPL-3.0-only

use crate::attributes::configured_attributes;
use crate::error::Error;
use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};
use syn::ext::IdentExt;
use syn::{Attribute, Expr, Item, Lit, Meta};

pub struct Source {
    pub path: PathBuf,
    pub syntax: syn::File,
}

pub type Sources = BTreeMap<PathBuf, Source>;

struct Pending {
    declared_in: PathBuf,
    path: PathBuf,
    children: PathBuf,
}

pub struct Crate<'sources> {
    pub root: &'sources Source,
    pub modules: Vec<&'sources Source>,
}

#[derive(Clone, Copy)]
struct ModuleScope<'path> {
    file: &'path Path,
    children: &'path Path,
    path_base: &'path Path,
}

impl<'sources> Crate<'sources> {
    pub fn read(root: &Path, sources: &'sources Sources) -> Result<Self, Error> {
        let root_source = sources.get(root).ok_or_else(|| Error::CrateRoot {
            path: root.to_path_buf(),
        })?;
        let directory = parent(root)?;
        let mut pending = Vec::new();
        let scope = ModuleScope {
            file: root,
            children: directory,
            path_base: directory,
        };
        declared_files(&root_source.syntax.items, scope, sources, &mut pending)?;
        let mut modules = Vec::new();
        while let Some(file) = pending.pop() {
            let source = sources.get(&file.path).ok_or_else(|| Error::ModuleFile {
                declared_in: file.declared_in.clone(),
                module: file.path.display().to_string(),
            })?;
            let scope = ModuleScope {
                file: &file.path,
                children: &file.children,
                path_base: parent(&file.path)?,
            };
            declared_files(&source.syntax.items, scope, sources, &mut pending)?;
            modules.push(source);
        }
        Ok(Self {
            root: root_source,
            modules,
        })
    }

    pub fn sources(&self) -> impl Iterator<Item = &Source> {
        std::iter::once(self.root).chain(self.modules.iter().copied())
    }
}

fn declared_files(
    items: &[Item],
    scope: ModuleScope<'_>,
    sources: &Sources,
    pending: &mut Vec<Pending>,
) -> Result<(), Error> {
    for item in items {
        let Item::Mod(module) = item else {
            continue;
        };
        let name = module.ident.unraw().to_string();
        let paths = path_attributes(&module.attrs).map_err(|source| Error::ParseSource {
            path: scope.file.to_path_buf(),
            source,
        })?;
        if let Some((_, inline)) = &module.content {
            let directory = scope
                .children
                .join(paths.first().map_or(name.as_str(), String::as_str));
            let inline_scope = ModuleScope {
                children: &directory,
                path_base: &directory,
                ..scope
            };
            declared_files(inline, inline_scope, sources, pending)?;
        } else if paths.is_empty() {
            pending.push(named_file(scope, name, sources)?);
        } else {
            for path in &paths {
                let path = without_parent_components(&scope.path_base.join(path));
                pending.push(Pending {
                    declared_in: scope.file.to_path_buf(),
                    children: parent(&path)?.to_path_buf(),
                    path,
                });
            }
        }
    }
    Ok(())
}

fn named_file(scope: ModuleScope<'_>, name: String, sources: &Sources) -> Result<Pending, Error> {
    let file = scope.children.join(format!("{name}.rs"));
    if sources.contains_key(&file) {
        return Ok(Pending {
            declared_in: scope.file.to_path_buf(),
            path: file,
            children: scope.children.join(&name),
        });
    }
    let file = scope.children.join(&name).join("mod.rs");
    if sources.contains_key(&file) {
        return Ok(Pending {
            declared_in: scope.file.to_path_buf(),
            children: parent(&file)?.to_path_buf(),
            path: file,
        });
    }
    Err(Error::ModuleFile {
        declared_in: scope.file.to_path_buf(),
        module: name,
    })
}

fn path_attributes(attributes: &[Attribute]) -> syn::Result<Vec<String>> {
    let mut paths = Vec::new();
    for attribute in attributes {
        paths.extend(path_values(&attribute.meta)?);
    }
    Ok(paths)
}

fn path_values(meta: &Meta) -> syn::Result<Vec<String>> {
    match meta {
        Meta::NameValue(value) if value.path.is_ident("path") => match &value.value {
            Expr::Lit(literal) => match &literal.lit {
                Lit::Str(path) => Ok(vec![path.value()]),
                _ => Ok(Vec::new()),
            },
            _ => Ok(Vec::new()),
        },
        Meta::List(list) if list.path.is_ident("cfg_attr") => {
            let mut paths = Vec::new();
            for attribute in configured_attributes(list)? {
                paths.extend(path_values(&attribute)?);
            }
            Ok(paths)
        }
        _ => Ok(Vec::new()),
    }
}

fn parent(path: &Path) -> Result<&Path, Error> {
    path.parent().ok_or_else(|| Error::NoParentDirectory {
        path: path.to_path_buf(),
    })
}

fn without_parent_components(path: &Path) -> PathBuf {
    path.components()
        .fold(PathBuf::new(), |mut kept, component| {
            if component == Component::ParentDir {
                let _removed_last_component = kept.pop();
            } else {
                kept.push(component);
            }
            kept
        })
}
