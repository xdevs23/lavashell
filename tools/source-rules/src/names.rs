// SPDX-License-Identifier: GPL-3.0-only
// https://doc.rust-lang.org/reference/names/name-resolution.html
mod declarations;

use crate::metadata::ExternPreludes;
use declarations::declared_enum;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use syn::ext::IdentExt;
use syn::{Block, Ident, Item, Stmt, Type, UseTree};

pub use declarations::sources_by_module;

pub type Variants = BTreeSet<String>;

fn name_of(ident: &Ident) -> String {
    ident.unraw().to_string()
}

pub fn path_names(path: &syn::Path) -> Vec<String> {
    (path.segments.iter())
        .map(|segment| name_of(&segment.ident))
        .collect()
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ModuleKey {
    crate_root: PathBuf,
    path: Vec<String>,
}

impl ModuleKey {
    fn new(crate_root: &Path, path: &[String]) -> Self {
        Self {
            crate_root: crate_root.to_path_buf(),
            path: path.to_vec(),
        }
    }

    fn child(&self, name: &str) -> Self {
        Self {
            crate_root: self.crate_root.clone(),
            path: [&self.path[..], &[name.to_owned()]].concat(),
        }
    }

    pub fn is_in_crate(&self, crate_root: &str) -> bool {
        self.crate_root.ends_with(crate_root)
    }

    fn parent(&self) -> Option<Self> {
        let (_child, parent) = self.path.split_last()?;
        Some(Self::new(&self.crate_root, parent))
    }

    fn crate_root(&self) -> Self {
        Self::new(&self.crate_root, &[])
    }
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct EnumKey {
    module: ModuleKey,
    pub name: String,
}

impl EnumKey {
    fn new(module: &ModuleKey, name: String) -> Self {
        Self {
            module: module.clone(),
            name,
        }
    }
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Variant {
    pub enum_key: EnumKey,
    pub name: String,
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Named {
    Module(ModuleKey),
    Enum(EnumKey),
    Variant(Variant),
    External(Vec<String>),
    Unchecked,
}

enum Binding {
    Path(Vec<String>),
    Module,
    Enum,
    OtherItem,
}

#[derive(Default)]
struct ScopeNames {
    bindings: BTreeMap<String, Binding>,
    globs: Vec<Vec<String>>,
}

impl ScopeNames {
    fn declared_in<'item>(items: impl IntoIterator<Item = &'item Item>) -> Self {
        let mut names = Self::default();
        for item in items {
            match item {
                Item::Use(used) => names.record_use(&[], &used.tree),
                item => names.bindings.extend(declared_name(item)),
            }
        }
        names
    }

    fn extend(&mut self, other: Self) {
        self.bindings.extend(other.bindings);
        self.globs.extend(other.globs);
    }

    fn record_use(&mut self, prefix: &[String], tree: &UseTree) {
        match tree {
            UseTree::Path(path) => {
                self.record_use(&[prefix, &[name_of(&path.ident)]].concat(), &path.tree);
            }
            UseTree::Name(used) => self.bind_use(prefix, &used.ident, &used.ident),
            UseTree::Rename(renamed) => self.bind_use(prefix, &renamed.ident, &renamed.rename),
            UseTree::Glob(_) => self.globs.push(prefix.to_vec()),
            UseTree::Group(group) => {
                for tree in &group.items {
                    self.record_use(prefix, tree);
                }
            }
        }
    }

    fn bind_use(&mut self, prefix: &[String], used: &Ident, bound: &Ident) {
        let path = if used == "self" {
            prefix.to_vec()
        } else {
            [prefix, &[name_of(used)]].concat()
        };
        let bound = if bound == "self" {
            path.last().cloned()
        } else {
            Some(name_of(bound))
        };
        self.bindings
            .extend(bound.map(|bound| (bound, Binding::Path(path))));
    }
}

fn declared_name(item: &Item) -> Option<(String, Binding)> {
    let (ident, binding) = match item {
        Item::Type(alias) => match &*alias.ty {
            Type::Path(aliased) => (&alias.ident, Binding::Path(path_names(&aliased.path))),
            _ => (&alias.ident, Binding::OtherItem),
        },
        Item::ExternCrate(external) => (
            external
                .rename
                .as_ref()
                .map_or(&external.ident, |(_, bound)| bound),
            Binding::Path(vec![name_of(&external.ident)]),
        ),
        Item::Mod(module) => (&module.ident, Binding::Module),
        Item::Enum(declared) => (&declared.ident, Binding::Enum),
        Item::Macro(invocation) => {
            let declared = invocation
                .mac
                .parse_body_with(declared_enum)
                .ok()
                .flatten()?;
            return Some((name_of(&declared.name), Binding::Enum));
        }
        Item::Struct(declared) => (&declared.ident, Binding::OtherItem),
        Item::Union(declared) => (&declared.ident, Binding::OtherItem),
        Item::Trait(declared) => (&declared.ident, Binding::OtherItem),
        Item::TraitAlias(declared) => (&declared.ident, Binding::OtherItem),
        Item::Const(declared) => (&declared.ident, Binding::OtherItem),
        Item::Static(declared) => (&declared.ident, Binding::OtherItem),
        Item::Fn(declared) => (&declared.sig.ident, Binding::OtherItem),
        _ => return None,
    };
    Some((name_of(ident), binding))
}

pub struct RepositoryNames<'libraries> {
    modules: BTreeMap<ModuleKey, ScopeNames>,
    enums: BTreeMap<EnumKey, Vec<Variants>>,
    extern_preludes: &'libraries ExternPreludes,
}

impl<'libraries> RepositoryNames<'libraries> {
    fn new(extern_preludes: &'libraries ExternPreludes) -> Self {
        Self {
            modules: BTreeMap::new(),
            enums: BTreeMap::new(),
            extern_preludes,
        }
    }

    fn record_module<'item>(
        &mut self,
        module: &ModuleKey,
        items: impl IntoIterator<Item = &'item Item>,
    ) {
        (self.modules.entry(module.clone()).or_default()).extend(ScopeNames::declared_in(items));
    }

    fn record_enum(&mut self, enum_key: EnumKey, variants: Variants) {
        self.enums.entry(enum_key).or_default().push(variants);
    }

    fn declares(&self, enum_key: &EnumKey, variants: &Variants) -> bool {
        self.enums
            .get(enum_key)
            .is_some_and(|declared| declared.contains(variants))
    }

    fn declares_variant(&self, enum_key: &EnumKey, variant: &str) -> bool {
        self.enums
            .get(enum_key)
            .is_some_and(|declared| declared.iter().any(|variants| variants.contains(variant)))
    }

    pub fn scopes_of(&self, module: &ModuleKey) -> Scopes<'_> {
        Scopes {
            repository: self,
            module: module.clone(),
            blocks: Vec::new(),
        }
    }
}

pub struct Scopes<'repository> {
    repository: &'repository RepositoryNames<'repository>,
    module: ModuleKey,
    blocks: Vec<ScopeNames>,
}

impl Scopes<'_> {
    pub fn child_module(&self, name: &Ident) -> Self {
        self.repository
            .scopes_of(&self.module.child(&name_of(name)))
    }

    pub fn enter_block(&mut self, block: &Block) {
        let items = block.stmts.iter().filter_map(|statement| match statement {
            Stmt::Item(item) => Some(item),
            _ => None,
        });
        self.blocks.push(ScopeNames::declared_in(items));
    }

    pub fn leave_block(&mut self) {
        let _left_block = self.blocks.pop();
    }

    pub fn declares(&self, enum_key: &EnumKey, variants: &Variants) -> bool {
        self.repository.declares(enum_key, variants)
    }

    pub fn resolve(&self, path: &[String], self_type: Option<&Named>) -> Named {
        let place = Place {
            module: &self.module,
            blocks: &self.blocks,
        };
        Lookup {
            repository: self.repository,
            resolving: BTreeSet::new(),
        }
        .path(place, path, self_type)
    }
}

#[derive(Clone, Copy)]
struct Place<'scope> {
    module: &'scope ModuleKey,
    blocks: &'scope [ScopeNames],
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
enum ScopeId {
    Module(ModuleKey),
    Block(usize),
}

struct Lookup<'repository> {
    repository: &'repository RepositoryNames<'repository>,
    resolving: BTreeSet<(ScopeId, String)>,
}

impl Lookup<'_> {
    fn path(&mut self, place: Place<'_>, path: &[String], self_type: Option<&Named>) -> Named {
        let Some((first, rest)) = path.split_first() else {
            return Named::Unchecked;
        };
        let start = match first.as_str() {
            "Self" => self_type.cloned(),
            "crate" => Some(Named::Module(place.module.crate_root())),
            "self" | "super" => self.member(Named::Module(place.module.clone()), first),
            name => Some(self.lookup(place, name)),
        };
        (rest.iter()).fold(start.unwrap_or(Named::Unchecked), |named, segment| {
            self.member(named, segment).unwrap_or(Named::Unchecked)
        })
    }

    fn member(&mut self, named: Named, name: &str) -> Option<Named> {
        match named {
            Named::Module(module) => match name {
                "self" => Some(Named::Module(module)),
                "super" => module.parent().map(Named::Module),
                _ => self.module_member(&module, name),
            },
            Named::Enum(enum_key) => {
                (self.repository.declares_variant(&enum_key, name)).then(|| {
                    Named::Variant(Variant {
                        enum_key,
                        name: name.to_owned(),
                    })
                })
            }
            Named::External(path) => Some(Named::External([path, vec![name.to_owned()]].concat())),
            Named::Variant(_) | Named::Unchecked => Some(Named::Unchecked),
        }
    }

    fn lookup(&mut self, place: Place<'_>, name: &str) -> Named {
        for (depth, block) in place.blocks.iter().enumerate().rev() {
            let enclosing = Place {
                module: place.module,
                blocks: &place.blocks[..=depth],
            };
            if let Some(named) = self.scope_member(ScopeId::Block(depth), enclosing, block, name) {
                return named;
            }
        }
        (self.module_member(place.module, name))
            .or_else(|| {
                (self.repository.extern_preludes)
                    .library_root(&place.module.crate_root, name)
                    .map(|root| Named::Module(ModuleKey::new(root, &[])))
            })
            .unwrap_or_else(|| Named::External(vec![name.to_owned()]))
    }

    fn module_member(&mut self, module: &ModuleKey, name: &str) -> Option<Named> {
        let repository = self.repository;
        let names = repository.modules.get(module)?;
        let place = Place {
            module,
            blocks: &[],
        };
        self.scope_member(ScopeId::Module(module.clone()), place, names, name)
    }

    fn scope_member(
        &mut self,
        scope: ScopeId,
        place: Place<'_>,
        declared: &ScopeNames,
        name: &str,
    ) -> Option<Named> {
        let resolving = (scope, name.to_owned());
        if !self.resolving.insert(resolving.clone()) {
            return None;
        }
        let named = match declared.bindings.get(name) {
            Some(binding) => Some(self.bound(place, binding, name)),
            None => self.glob_member(place, &declared.globs, name),
        };
        let _resolved = self.resolving.remove(&resolving);
        named
    }

    fn bound(&mut self, place: Place<'_>, binding: &Binding, name: &str) -> Named {
        match binding {
            Binding::Path(path) => self.path(place, path, None),
            Binding::Module => Named::Module(place.module.child(name)),
            Binding::Enum => Named::Enum(EnumKey::new(place.module, name.to_owned())),
            Binding::OtherItem => Named::Unchecked,
        }
    }

    fn glob_member(
        &mut self,
        place: Place<'_>,
        globs: &[Vec<String>],
        name: &str,
    ) -> Option<Named> {
        let supplied: BTreeSet<Named> = (globs.iter())
            .filter_map(|prefix| {
                let imported_from = self.path(place, prefix, None);
                self.member(imported_from, name)
            })
            .collect();
        let (external, repository): (Vec<&Named>, Vec<&Named>) = (supplied.iter())
            .filter(|named| **named != Named::Unchecked)
            .partition(|named| matches!(named, Named::External(_)));
        match (repository.as_slice(), external.as_slice()) {
            ([only], _) | ([], [only]) => Some((*only).clone()),
            ([], []) => supplied
                .contains(&Named::Unchecked)
                .then_some(Named::Unchecked),
            _ => Some(Named::Unchecked),
        }
    }
}
