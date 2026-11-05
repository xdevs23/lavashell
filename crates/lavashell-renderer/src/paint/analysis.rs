// SPDX-License-Identifier: GPL-3.0-only
use super::parent_copy;
use crate::effect_registry::EffectRegistry;
use lavashell_core::prelude::*;
use lavashell_core::retained::{Extent, Footprint};
use lavashell_core::scene::tree::{Packet, PacketGeneration};
use lavashell_core::surface::SurfaceId;
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Clone, Copy)]
enum Scope {
    Save,
    /// A layer that starts from a copy of the parent when a command inside reads the pixels below the layer.
    Transparent,
    /// A layer inside which a command reads only the layer.
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

/// A scope opened by the command at `opener` and not closed yet.
struct OpenScope {
    opener: usize,
    scope: Scope,
    /// Whether a transform applies to commands inside the scope.
    transformed: bool,
}

/// Whether `command` paints past the layout bounds of the scopes around it.
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

/// Whether the commands `inside` the layer opened by `opener` paint past the layout bounds of the layer.
fn paints_past_bounds(opener: &DrawCommand, inside: &[DrawCommand]) -> bool {
    match opener {
        DrawCommand::OpacityLayer { bounds, .. } | DrawCommand::EffectLayer { bounds, .. } => {
            Footprint::analyze(inside)
                .output
                .is_some_and(|painted| !painted.inside(*bounds))
        }
        _ => false,
    }
}

/// Facts about the draw commands of one packet that painting the commands needs, found in one pass over them.
pub(crate) struct Analysis {
    /// For each command that opens a transparent layer, whether a command inside reads the pixels below it.
    groups: Vec<bool>,
    /// For each command that opens an effect layer that a command inside reads through, the layout bounds of the
    /// layer joined with the area that the commands inside paint or read, in the coordinates of the layer.
    areas: Vec<Option<Extent>>,
    /// For each command that opens a scope, whether a command inside the scope paints past the layout bounds of the
    /// scope.
    expanding: Vec<bool>,
    /// What the registered effects among the commands read, or `None` when the packet has none.
    effects: Option<EffectReads>,
    /// Whether a command opens a layer. Such a layer gets the size of the clip around the packet.
    pub layers: bool,
}

#[derive(Clone, Copy, Default)]
struct EffectReads {
    /// Whether an effect reads pixels painted before the packet, outside an isolated layer.
    parent: bool,
    /// Whether an effect samples pixels outside every extent given by the footprint of the packet.
    unknown: bool,
}

impl Analysis {
    pub fn has_effects(&self) -> bool {
        self.effects.is_some()
    }

    /// Whether the packet reads pixels painted before it, outside an isolated layer.
    pub fn reads_parent(&self) -> bool {
        self.effects.is_some_and(|reads| reads.parent)
    }

    /// Whether the packet samples pixels outside every extent given by its footprint.
    pub fn unknown_sampling(&self) -> bool {
        self.effects.is_some_and(|reads| reads.unknown)
    }

    /// Whether a command inside the layer opened at `index` reads the pixels below the layer.
    pub fn reads_group(&self, index: usize) -> bool {
        self.groups.get(index).copied().unwrap_or(false)
    }

    /// The area that the effect layer opened at `index` covers when a command inside reads through it. None means the
    /// whole clip, for a layer that paints past its layout bounds or a packet whose sampling is unknown.
    pub fn layer_area(&self, index: usize) -> Option<Extent> {
        (self.areas.get(index).copied().flatten())
            .filter(|_| !self.expands_group(index) && !self.unknown_sampling())
    }

    /// Whether the scope opened by the command at `index` paints past its layout bounds. Layout bounds do not clip,
    /// so the layer of such a scope gets the size of the clip around it.
    pub fn expands_group(&self, index: usize) -> bool {
        self.expanding.get(index).copied().unwrap_or(false)
    }

    fn mark_expanding(&mut self, opener: usize, command_count: usize) {
        self.expanding.resize(command_count, false);
        self.expanding[opener] = true;
    }

    /// Marks `scope`, which ends before `commands[end]`, as expanding when a transform inside it moves the commands
    /// past the layout bounds of the scope, and records the area of an effect layer that a command inside reads
    /// through.
    fn close(&mut self, commands: &[DrawCommand], scope: &OpenScope, end: usize) {
        let (opener, inside) = (&commands[scope.opener], &commands[scope.opener + 1..end]);
        if scope.transformed
            && !self.expands_group(scope.opener)
            && paints_past_bounds(opener, inside)
        {
            self.mark_expanding(scope.opener, commands.len());
        }
        if let (Scope::Transparent, DrawCommand::EffectLayer { bounds, .. }) = (scope.scope, opener)
            && self.reads_group(scope.opener)
        {
            let footprint = Footprint::analyze(inside);
            self.areas.resize(commands.len(), None);
            self.areas[scope.opener] = Some(
                [footprint.output, footprint.backdrop]
                    .into_iter()
                    .flatten()
                    .fold(Extent::from(*bounds), Extent::union),
            );
        }
    }
}

/// The analysis of every packet shown by the last plan of each surface. All analyses come from one revision of the
/// effect registry. A new revision drops them.
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
    ) -> Rc<Analysis> {
        if self.revision != registry.revision() {
            self.revision = registry.revision();
            self.packets.clear();
        }
        let analyzed =
            (self.packets.entry((surface, packet.generation()))).or_insert_with(|| Analyzed {
                analysis: Rc::new(inspect(packet.commands(), registry)),
                shown: true,
            });
        analyzed.shown = true;
        analyzed.analysis.clone()
    }

    /// Removes the packets of `surface` that the last plan for the surface did not show.
    pub fn end(&mut self, surface: SurfaceId) {
        self.packets
            .retain(|(shown_in, _), analyzed| *shown_in != surface || analyzed.shown);
    }

    pub fn forget_surface(&mut self, surface: SurfaceId) {
        self.packets.retain(|(shown_in, _), _| *shown_in != surface);
    }
}

fn inspect(commands: &[DrawCommand], registry: &EffectRegistry) -> Analysis {
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
                    analysis.close(commands, &closed, index);
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
                    // An isolated layer stops the effect from reading pixels painted before the packet.
                    // A partial repaint must still rebuild every pixel that the effect samples.
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
    // A scope without a restore ends with the packet.
    for open in scopes.iter().rev() {
        analysis.close(commands, open, commands.len());
    }
    analysis
}
