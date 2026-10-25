// SPDX-License-Identifier: GPL-3.0-only
//! The source files of a crate, found the way rustc finds the files of its modules.

use crate::error::Error;
use std::path::{Path, PathBuf};
use syn::ext::IdentExt;
use syn::punctuated::Punctuated;
use syn::{Attribute, Expr, Item, Lit, Meta, Token};

/// A parsed source file of a crate.
pub struct Source {
    pub path: PathBuf,
    pub syntax: syn::File,
}

/// A file loaded by a module declaration, and the directory that contains the files of the modules declared in that
/// file.
struct Pending {
    path: PathBuf,
    children: PathBuf,
}

/// A crate: its root and each file that a `mod name;` declaration loads.
pub struct Crate {
    pub root: Source,
    pub modules: Vec<Source>,
}

impl Crate {
    pub fn read(root: &Path) -> Result<Self, Error> {
        let syntax = parse(root)?;
        let mut pending = Vec::new();
        declared_files(
            &syntax.items,
            root,
            &parent(root),
            &parent(root),
            &mut pending,
        )?;
        let mut modules = Vec::new();
        while let Some(file) = pending.pop() {
            let syntax = parse(&file.path)?;
            declared_files(
                &syntax.items,
                &file.path,
                &file.children,
                &parent(&file.path),
                &mut pending,
            )?;
            modules.push(Source {
                path: file.path,
                syntax,
            });
        }
        Ok(Self {
            root: Source {
                path: root.to_path_buf(),
                syntax,
            },
            modules,
        })
    }

    pub fn sources(&self) -> impl Iterator<Item = &Source> {
        std::iter::once(&self.root).chain(&self.modules)
    }
}

fn parse(path: &Path) -> Result<syn::File, Error> {
    let text = std::fs::read_to_string(path).map_err(|source| Error::ReadSource {
        path: path.to_path_buf(),
        source,
    })?;
    syn::parse_file(&text).map_err(|source| Error::ParseSource {
        path: path.to_path_buf(),
        source,
    })
}

/// Adds the files of the modules declared in `items` to `pending`. The file of a module without a `path` attribute
/// is looked up in `children`. A `path` attribute is resolved against `attributes`. Inside an inline module, both
/// directories become the subdirectory of `children` named after the inline module or after its `path` attribute.
fn declared_files(
    items: &[Item],
    declared_in: &Path,
    children: &Path,
    attributes: &Path,
    pending: &mut Vec<Pending>,
) -> Result<(), Error> {
    for item in items {
        let Item::Mod(module) = item else {
            continue;
        };
        let name = module.ident.unraw().to_string();
        let paths = path_attributes(&module.attrs).map_err(|source| Error::ParseSource {
            path: declared_in.to_path_buf(),
            source,
        })?;
        if let Some((_, inline)) = &module.content {
            let directory = children.join(paths.first().map_or(name.as_str(), String::as_str));
            declared_files(inline, declared_in, &directory, &directory, pending)?;
        } else if paths.is_empty() {
            pending.push(named_file(declared_in, children, name)?);
        } else {
            pending.extend(paths.iter().map(|path| {
                let path = attributes.join(path);
                Pending {
                    children: parent(&path),
                    path,
                }
            }));
        }
    }
    Ok(())
}

/// The file `name.rs` in `children`, or else the file `name/mod.rs` in `children`. The modules declared in `name.rs`
/// are looked up in `children/name`. The modules declared in `name/mod.rs` are looked up in the directory of that
/// file.
fn named_file(declared_in: &Path, children: &Path, name: String) -> Result<Pending, Error> {
    let file = children.join(format!("{name}.rs"));
    if file.is_file() {
        return Ok(Pending {
            path: file,
            children: children.join(&name),
        });
    }
    let file = children.join(&name).join("mod.rs");
    if file.is_file() {
        return Ok(Pending {
            children: parent(&file),
            path: file,
        });
    }
    Err(Error::ModuleFile {
        declared_in: declared_in.to_path_buf(),
        module: name,
    })
}

/// The values of the `path` attributes of a module, including those inside `cfg_attr`. Each `cfg_attr` can point to a
/// different file for its configuration, so one module can have several files.
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

/// The attributes that `cfg_attr(predicate, attributes..)` applies, without its predicate.
pub fn configured_attributes(list: &syn::MetaList) -> syn::Result<impl Iterator<Item = Meta>> {
    Ok(list
        .parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)?
        .into_iter()
        .skip(1))
}

fn parent(path: &Path) -> PathBuf {
    path.parent().map(Path::to_path_buf).unwrap_or_default()
}
