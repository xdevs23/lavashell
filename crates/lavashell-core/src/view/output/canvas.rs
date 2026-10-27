// SPDX-License-Identifier: GPL-3.0-only
use super::{GroupId, Painted};
use crate::effects::{BlendMode, SourceFilter};
use crate::geometry::Rect;
use crate::scene::tree::{Group, GroupValues, NamespaceId};
use crate::scene::{Affine, DrawCommand};
use crate::shape::{Corners, Radius, RoundedRect};
use crate::units::NonNegativeDp;
use crate::view::FrameError;
use std::convert::Infallible;

#[derive(Default)]
pub(in crate::view) struct Canvas {
    top: Level,
    /// The levels opened outside every scope painted as drawing commands and not yet closed, innermost last.
    shown: Vec<OpenLevel>,
    /// The levels opened inside a scope painted as drawing commands and not yet closed, innermost last. They are
    /// inside every level of `shown`.
    flattened: Vec<OpenLevel<FlatKind, Infallible>>,
    /// Whether the canvas paints inside a scope of an enclosing canvas that no group can apply, such as a clip with
    /// corners that are not circular.
    inside_flattened: bool,
}

struct Level<G = NamespaceId> {
    items: Vec<Painted<G>>,
    leaf: Vec<DrawCommand>,
}

impl<G> Default for Level<G> {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            leaf: Vec::new(),
        }
    }
}

struct OpenLevel<K = LevelKind, G = NamespaceId> {
    kind: K,
    level: Level<G>,
}

enum LevelKind {
    Scope(GroupValues),
    Group { id: GroupId, group: Group },
}

/// The kind of a level opened inside a scope painted as drawing commands.
enum FlatKind {
    Level(LevelKind),
    /// The drawing commands that open the scope. One `Restore` command closes the scope.
    Commands(Vec<DrawCommand>),
}

pub(in crate::view) enum ScopeKind {
    Group(GroupValues),
    /// The drawing commands that open the scope, such as a save and a clip. One `Restore` command closes the scope.
    Commands(Vec<DrawCommand>),
}

impl ScopeKind {
    pub fn transform(transform: Affine) -> Self {
        Self::Group(GroupValues {
            transform,
            ..GroupValues::default()
        })
    }

    pub fn opacity(opacity: f64) -> Self {
        Self::Group(GroupValues {
            opacity,
            ..GroupValues::default()
        })
    }

    pub fn clip(rect: Rect, corners: Corners<Radius>) -> Self {
        match corners.circular() {
            Some(corners) => Self::Group(GroupValues {
                clip: Some(RoundedRect::new(rect, corners)),
                ..GroupValues::default()
            }),
            None => Self::Commands(vec![DrawCommand::Save, DrawCommand::Clip { rect, corners }]),
        }
    }

    pub fn effect(bounds: Rect, blur: NonNegativeDp, filter: Option<SourceFilter>) -> Self {
        Self::Commands(vec![DrawCommand::EffectLayer {
            bounds,
            blend: BlendMode::SourceOver,
            blur,
            filter,
        }])
    }
}

impl<G> Level<G> {
    fn nest(&mut self, item: Painted<G>) {
        self.flush();
        self.items.push(item);
    }

    fn flush(&mut self) {
        if !self.leaf.is_empty() {
            let commands = std::mem::take(&mut self.leaf).into();
            self.items.push(Painted::Leaf { commands });
        }
    }

    fn finish(mut self) -> Vec<Painted<G>> {
        self.flush();
        self.items
    }

    fn close(&mut self, kind: LevelKind, mut inner: Self) {
        match kind {
            LevelKind::Scope(values) if !inner.items.is_empty() => {
                self.nest(Painted::Scope {
                    values,
                    items: inner.finish(),
                });
            }
            LevelKind::Scope(values) => {
                super::flatten_group(values, &mut self.leaf, |out| out.append(&mut inner.leaf));
            }
            LevelKind::Group { id, group } => {
                let items = inner.finish().into();
                self.nest(Painted::Group { id, group, items });
            }
        }
    }
}

impl Level<Infallible> {
    fn flatten_into(mut self, out: &mut Vec<DrawCommand>) {
        for item in &self.items {
            item.flatten_into(out);
        }
        out.append(&mut self.leaf);
    }

    fn close_flat(&mut self, kind: FlatKind, inner: Self) {
        match kind {
            FlatKind::Level(kind) => self.close(kind, inner),
            FlatKind::Commands(opening) => {
                self.leaf.extend(opening);
                inner.flatten_into(&mut self.leaf);
                self.leaf.push(DrawCommand::Restore);
            }
        }
    }
}

impl Canvas {
    pub fn nested(&self) -> Self {
        Self {
            inside_flattened: self.flattened(),
            ..Self::default()
        }
    }

    fn current_shown(&mut self) -> &mut Level {
        match self.shown.last_mut() {
            Some(open) => &mut open.level,
            None => &mut self.top,
        }
    }

    pub fn commands(&mut self) -> &mut Vec<DrawCommand> {
        match self.flattened.last_mut() {
            Some(open) => &mut open.level.leaf,
            None => match self.shown.last_mut() {
                Some(open) => &mut open.level.leaf,
                None => &mut self.top.leaf,
            },
        }
    }

    pub fn push(&mut self, command: DrawCommand) {
        self.commands().push(command);
    }

    /// # Errors
    ///
    /// Fails when `item` grafts a namespace inside a scope painted as drawing commands.
    pub fn nest(&mut self, item: Painted) -> Result<(), FrameError> {
        match self.flattened.last_mut() {
            Some(open) => open
                .level
                .nest(item.flat().map_err(FrameError::GraftInLayer)?),
            None => self.current_shown().nest(item),
        }
        Ok(())
    }

    /// # Errors
    ///
    /// Fails when one of `items` grafts a namespace inside a scope painted as drawing commands.
    pub fn append(&mut self, items: Vec<Painted>) -> Result<(), FrameError> {
        for item in items {
            match item {
                Painted::Leaf { commands } => self.commands().extend(commands.iter().cloned()),
                item => self.nest(item)?,
            }
        }
        Ok(())
    }

    /// Whether the canvas paints inside a scope that no group can apply, such as an effect layer. In such a scope,
    /// groups paint as drawing commands. The slots of those groups must then contain fixed values that do not
    /// animate.
    pub fn flattened(&self) -> bool {
        self.inside_flattened || !self.flattened.is_empty()
    }

    /// Shows the root of `namespace` inside a group with `values`.
    ///
    /// # Errors
    ///
    /// Fails inside a scope that no group can apply, where the commands of a packet cannot contain the namespace.
    pub fn graft(&mut self, values: GroupValues, namespace: NamespaceId) -> Result<(), FrameError> {
        if self.flattened() {
            return Err(FrameError::GraftInLayer(namespace));
        }
        self.open_scope(ScopeKind::Group(values));
        self.current_shown().nest(Painted::Graft(namespace));
        self.close();
        Ok(())
    }

    pub fn open_scope(&mut self, scope: ScopeKind) {
        match scope {
            ScopeKind::Group(values) => self.open(LevelKind::Scope(values)),
            ScopeKind::Commands(opening) => self.open_flat(FlatKind::Commands(opening)),
        }
    }

    pub fn open_group(&mut self, id: GroupId, group: Group) {
        self.open(LevelKind::Group { id, group });
    }

    fn open(&mut self, kind: LevelKind) {
        if self.flattened() {
            self.open_flat(FlatKind::Level(kind));
        } else {
            let level = Level::default();
            self.shown.push(OpenLevel { kind, level });
        }
    }

    fn open_flat(&mut self, kind: FlatKind) {
        let level = Level::default();
        self.flattened.push(OpenLevel { kind, level });
    }

    pub fn close(&mut self) {
        if let Some(OpenLevel { kind, level }) = self.flattened.pop() {
            if let Some(open) = self.flattened.last_mut() {
                open.level.close_flat(kind, level);
            } else {
                let mut closed = Level::<Infallible>::default();
                closed.close_flat(kind, level);
                closed.flatten_into(&mut self.current_shown().leaf);
            }
        } else if let Some(OpenLevel { kind, level }) = self.shown.pop() {
            self.current_shown().close(kind, level);
        }
    }

    pub fn finish(mut self) -> Vec<Painted> {
        while !self.flattened.is_empty() || !self.shown.is_empty() {
            self.close();
        }
        self.top.finish()
    }
}
