// SPDX-License-Identifier: GPL-3.0-only
use super::{GroupId, Painted};
use crate::effects::{BlendMode, SourceFilter};
use crate::geometry::Rect;
use crate::scene::tree::{Group, GroupValues};
use crate::scene::{Affine, DrawCommand};
use crate::shape::{Corners, Radius, RoundedRect};
use crate::units::NonNegativeDp;

#[derive(Default)]
pub(in crate::view) struct Canvas {
    top: Level,
    /// The levels opened and not yet closed, with the innermost level last.
    open: Vec<OpenLevel>,
    /// Whether the canvas paints inside a scope of an enclosing canvas that no group can apply, such as a clip with
    /// corners that are not circular.
    inside_flattened: bool,
}

#[derive(Default)]
struct Level {
    items: Vec<Painted>,
    leaf: Vec<DrawCommand>,
}

struct OpenLevel {
    kind: LevelKind,
    level: Level,
}

enum LevelKind {
    Scope(ScopeKind),
    Group { id: GroupId, group: Group },
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

impl Level {
    fn nest(&mut self, item: Painted) {
        self.flush();
        self.items.push(item);
    }

    fn flush(&mut self) {
        if !self.leaf.is_empty() {
            let commands = std::mem::take(&mut self.leaf).into();
            self.items.push(Painted::Leaf { commands });
        }
    }

    fn finish(mut self) -> Vec<Painted> {
        self.flush();
        self.items
    }

    fn flatten_into(mut self, out: &mut Vec<DrawCommand>) {
        for item in &self.items {
            item.flatten_into(out);
        }
        out.append(&mut self.leaf);
    }

    fn close(&mut self, kind: LevelKind, inner: Level) {
        match kind {
            LevelKind::Scope(ScopeKind::Group(values)) if !inner.items.is_empty() => {
                self.nest(Painted::Scope {
                    values,
                    items: inner.finish(),
                });
            }
            LevelKind::Scope(ScopeKind::Group(values)) => {
                super::flatten_group(values, &mut self.leaf, |out| inner.flatten_into(out));
            }
            LevelKind::Scope(ScopeKind::Commands(opening)) => {
                self.leaf.extend(opening);
                inner.flatten_into(&mut self.leaf);
                self.leaf.push(DrawCommand::Restore);
            }
            LevelKind::Group { id, group } => {
                let items = inner.finish().into();
                self.nest(Painted::Group { id, group, items });
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

    fn current(&mut self) -> &mut Level {
        match self.open.last_mut() {
            Some(open) => &mut open.level,
            None => &mut self.top,
        }
    }

    pub fn commands(&mut self) -> &mut Vec<DrawCommand> {
        &mut self.current().leaf
    }

    pub fn push(&mut self, command: DrawCommand) {
        self.commands().push(command);
    }

    pub fn nest(&mut self, item: Painted) {
        self.current().nest(item);
    }

    pub fn append(&mut self, items: Vec<Painted>) {
        for item in items {
            match item {
                Painted::Leaf { commands } => self.commands().extend(commands.iter().cloned()),
                item => self.nest(item),
            }
        }
    }

    /// Whether the canvas paints inside a scope that no group can apply, such as an effect layer. In such a scope,
    /// groups paint as drawing commands. The slots of those groups must then contain fixed values that do not
    /// animate.
    pub fn flattened(&self) -> bool {
        self.inside_flattened
            || (self.open.iter())
                .any(|open| matches!(&open.kind, LevelKind::Scope(ScopeKind::Commands(_))))
    }

    pub fn open_scope(&mut self, scope: ScopeKind) {
        self.open(LevelKind::Scope(scope));
    }

    pub fn open_group(&mut self, id: GroupId, group: Group) {
        self.open(LevelKind::Group { id, group });
    }

    fn open(&mut self, kind: LevelKind) {
        let level = Level::default();
        self.open.push(OpenLevel { kind, level });
    }

    pub fn close(&mut self) {
        if let Some(OpenLevel { kind, level }) = self.open.pop() {
            self.current().close(kind, level);
        }
    }

    pub fn finish(mut self) -> Vec<Painted> {
        while !self.open.is_empty() {
            self.close();
        }
        self.top.finish()
    }
}
