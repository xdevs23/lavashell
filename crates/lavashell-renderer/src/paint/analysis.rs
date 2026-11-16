// SPDX-License-Identifier: GPL-3.0-only
use super::parent_copy;
use crate::effect_registry::EffectRegistry;
use lavashell_core::prelude::*;
use lavashell_core::retained::{Extent, Footprint, UnbalancedRestore};
use lavashell_core::scene::tree::{Packet, PacketGeneration};
use lavashell_core::surface::{PerSurface, SurfaceId};
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::rc::Rc;

#[derive(Clone, Copy)]
enum Scope {
    Save,
    Transparent,
    Isolated,
}

impl Scope {
    fn of(command: &DrawCommand) -> Option<Self> {
        match command {
            DrawCommand::Save => Some(Self::Save),
            DrawCommand::Opacity(_) | DrawCommand::OpacityLayer { .. } => Some(Self::Transparent),
            DrawCommand::EffectLayer {
                blend,
                blur,
                filter,
                ..
            } if parent_copy::restores_effect_layer(*blend, *blur, filter.as_ref()) => {
                Some(Self::Transparent)
            }
            DrawCommand::EffectLayer { .. } => Some(Self::Isolated),
            _ => None,
        }
    }
}

struct OpenScope {
    opener: usize,
    scope: Scope,
    transformed: bool,
}

fn expands(command: &DrawCommand) -> bool {
    match command {
        DrawCommand::EffectLayer { blur, filter, .. } => {
            blur.get() > Dp::ZERO
                || (filter.as_ref()).is_some_and(|filter| filter.outset > NonNegativeDp::ZERO)
        }
        DrawCommand::Shadow { .. } => true,
        _ => false,
    }
}

fn layer_bounds(opener: &DrawCommand) -> Option<Rect> {
    match opener {
        DrawCommand::OpacityLayer { bounds, .. } | DrawCommand::EffectLayer { bounds, .. } => {
            Some(*bounds)
        }
        _ => None,
    }
}

pub(crate) struct Analysis {
    groups: Vec<bool>,
    areas: Vec<Option<Extent>>,
    expanding: Vec<bool>,
    effects: Option<EffectReads>,
    pub layers: bool,
}

#[derive(Clone, Copy, Default)]
struct EffectReads {
    parent: bool,
    unknown: bool,
}

impl Analysis {
    pub fn has_effects(&self) -> bool {
        self.effects.is_some()
    }

    pub fn reads_parent(&self) -> bool {
        self.effects.is_some_and(|reads| reads.parent)
    }

    pub fn unknown_sampling(&self) -> bool {
        self.effects.is_some_and(|reads| reads.unknown)
    }

    pub fn reads_group(&self, index: usize) -> bool {
        self.groups.get(index).copied().unwrap_or(false)
    }

    pub fn layer_area(&self, index: usize) -> Option<Extent> {
        (self.areas.get(index).copied().flatten())
            .filter(|_| !self.expands_group(index) && !self.unknown_sampling())
    }

    pub fn expands_group(&self, index: usize) -> bool {
        self.expanding.get(index).copied().unwrap_or(false)
    }

    fn mark_expanding(&mut self, opener: usize, command_count: usize) {
        self.expanding.resize(command_count, false);
        self.expanding[opener] = true;
    }

    fn close(
        &mut self,
        commands: &[DrawCommand],
        scope: &OpenScope,
        end: usize,
    ) -> Result<(), UnbalancedRestore> {
        let (opener, inside) = (&commands[scope.opener], &commands[scope.opener + 1..end]);
        if scope.transformed
            && !self.expands_group(scope.opener)
            && let Some(bounds) = layer_bounds(opener)
            && (Footprint::analyze(inside)?.output).is_some_and(|painted| !painted.inside(bounds))
        {
            self.mark_expanding(scope.opener, commands.len());
        }
        if let (Scope::Transparent, DrawCommand::EffectLayer { bounds, .. }) = (scope.scope, opener)
            && self.reads_group(scope.opener)
        {
            let footprint = Footprint::analyze(inside)?;
            self.areas.resize(commands.len(), None);
            self.areas[scope.opener] = Some(
                [footprint.output, footprint.backdrop]
                    .into_iter()
                    .flatten()
                    .fold(Extent::from(*bounds), Extent::union),
            );
        }
        Ok(())
    }
}

#[derive(Default)]
pub(crate) struct Analyses {
    packets: HashMap<(SurfaceId, PacketGeneration), Analyzed>,
    revision: u64,
}

struct Analyzed {
    analysis: Rc<Analysis>,
    shown: bool,
}

impl Analyses {
    pub fn begin(&mut self, surface: SurfaceId) {
        for ((shown_in, _), analyzed) in &mut self.packets {
            analyzed.shown &= *shown_in != surface;
        }
    }

    pub fn get(
        &mut self,
        surface: SurfaceId,
        packet: &Packet,
        registry: &EffectRegistry,
    ) -> Result<Rc<Analysis>, UnbalancedRestore> {
        if self.revision != registry.revision() {
            self.revision = registry.revision();
            self.clear();
        }
        let analyzed = match self.packets.entry((surface, packet.generation())) {
            Entry::Occupied(analyzed) => analyzed.into_mut(),
            Entry::Vacant(entry) => entry.insert(Analyzed {
                analysis: Rc::new(inspect(packet.commands(), registry)?),
                shown: true,
            }),
        };
        analyzed.shown = true;
        Ok(analyzed.analysis.clone())
    }

    pub fn end(&mut self, surface: SurfaceId) {
        self.packets
            .retain(|(shown_in, _), analyzed| *shown_in != surface || analyzed.shown);
    }

    pub fn clear(&mut self) {
        self.packets.clear();
    }
}

impl PerSurface for Analyses {
    fn forget_surface(&mut self, surface: SurfaceId) {
        self.packets.retain(|(shown_in, _), _| *shown_in != surface);
    }
}

fn inspect(
    commands: &[DrawCommand],
    registry: &EffectRegistry,
) -> Result<Analysis, UnbalancedRestore> {
    let mut analysis = Analysis {
        groups: Vec::new(),
        areas: Vec::new(),
        expanding: Vec::new(),
        effects: None,
        layers: false,
    };
    let mut scopes: Vec<OpenScope> = Vec::new();
    for (index, command) in commands.iter().enumerate() {
        let opens = Scope::of(command);
        if let Some(scope) = opens {
            analysis.layers |= !matches!(scope, Scope::Save);
            scopes.push(OpenScope {
                opener: index,
                scope,
                transformed: false,
            });
        }
        if expands(command) {
            for open in &scopes {
                analysis.mark_expanding(open.opener, commands.len());
            }
        }
        if opens.is_some() {
            continue;
        }
        let reads = match command {
            DrawCommand::Restore => {
                if let Some(closed) = scopes.pop() {
                    analysis.close(commands, &closed, index)?;
                }
                false
            }
            DrawCommand::Transform(_) => {
                for open in &mut scopes {
                    open.transformed = true;
                }
                false
            }
            _ => match registry.identify(command) {
                Some(effect) => {
                    let effects = analysis.effects.get_or_insert_default();
                    effects.unknown |= registry.needs_full_damage(effect);
                    registry.reads_destination(effect)
                }
                None => false,
            },
        };
        if reads {
            let mut isolated = false;
            for open in scopes.iter().rev() {
                match open.scope {
                    Scope::Transparent => {
                        analysis.groups.resize(commands.len(), false);
                        analysis.groups[open.opener] = true;
                    }
                    Scope::Isolated => {
                        isolated = true;
                        break;
                    }
                    Scope::Save => {}
                }
            }
            analysis.effects.get_or_insert_default().parent |= !isolated;
        }
    }
    for open in scopes.iter().rev() {
        analysis.close(commands, open, commands.len())?;
    }
    Ok(analysis)
}
