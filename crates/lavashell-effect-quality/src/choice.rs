// SPDX-License-Identifier: GPL-3.0-only
use super::{EffectId, GpuTime, VariantId};
use lavashell_core::{scene::tree::NodeKey, surface::SurfaceId};
use std::{cmp::Reverse, collections::BTreeMap, time::Duration};

#[derive(Clone, Copy, Debug)]
pub struct FrameBudget {
    /// The surface whose frame is planned. The draws passed with this budget are all live effect draws of this surface,
    /// so a chooser may forget its choices for the other draws of this surface.
    pub surface: SurfaceId,
    /// The GPU time per frame that the host sets aside for effect draws after the work of the application. A chooser
    /// plans draws whose expected times add up to at most this time. `None` when the host sets no limit, which makes
    /// every variant fit.
    pub gpu_time: Option<Duration>,
}

/// Identifies one effect draw across frames by the surface, the scene leaf that draws it, its place among the effect
/// draws of that leaf and the effect drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DrawId {
    pub surface: SurfaceId,
    pub leaf: NodeKey,
    /// The place of the draw among the effect draws of the leaf.
    pub ordinal: u64,
    pub effect: EffectId,
}

#[derive(Clone, Copy, Debug)]
pub struct VariantOption {
    pub variant: VariantId,
    pub gpu_time: GpuTime,
}

impl VariantOption {
    /// Whether the expected GPU time of this variant fits into the time `left`. A variant without a measured time fits,
    /// so that it gets drawn and measured. Every variant fits when `left` is `None`.
    fn fits(&self, left: Option<Duration>) -> bool {
        left.is_none_or(|left| {
            self.gpu_time
                .expected()
                .is_none_or(|expected| expected <= left)
        })
    }
}

#[derive(Clone, Debug)]
pub struct EffectDraw {
    pub id: DrawId,
    pub frames_waited: u64,
    /// The variants with which the effect can be drawn, from the best quality to the lowest.
    pub options: Vec<VariantOption>,
}

/// A variant option and its index in `EffectDraw::options`. A smaller index means a better quality.
#[derive(Clone, Copy)]
struct Ranked {
    place: usize,
    option: VariantOption,
}

impl EffectDraw {
    fn first_ranked(&self, wanted: impl Fn(&VariantOption) -> bool) -> Option<Ranked> {
        (self.options.iter().enumerate())
            .find(|(_, option)| wanted(option))
            .map(|(place, option)| Ranked {
                place,
                option: *option,
            })
    }

    fn ranked(&self, variant: VariantId) -> Option<Ranked> {
        self.first_ranked(|option| option.variant == variant)
    }

    fn best_within(&self, left: Option<Duration>) -> Option<Ranked> {
        self.first_ranked(|option| option.fits(left))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Draw(VariantId),
    Wait,
}

/// The plan for one effect draw in a frame: draw it with the given variant, or leave it out of this frame.
#[derive(Clone, Copy, Debug)]
pub enum Planned {
    Draw(VariantOption),
    Wait,
}

impl Planned {
    #[must_use]
    pub fn choice(&self) -> Choice {
        match self {
            Self::Draw(option) => Choice::Draw(option.variant),
            Self::Wait => Choice::Wait,
        }
    }
}

/// The planned variant, or the wait, for each effect draw in one frame of one surface.
#[derive(Clone, Debug)]
pub struct FramePlan {
    draws: BTreeMap<DrawId, Planned>,
}

impl FramePlan {
    /// Plans each of `draws` once with `plan`, in the order of their `rank`, the smallest first.
    #[must_use]
    pub fn for_draws<Rank: Ord>(
        draws: &[EffectDraw],
        mut rank: impl FnMut(&EffectDraw) -> Rank,
        mut plan: impl FnMut(&EffectDraw) -> Planned,
    ) -> Self {
        let mut ordered = draws.iter().collect::<Vec<_>>();
        ordered.sort_by_key(|draw| rank(draw));
        Self {
            draws: (ordered.into_iter())
                .map(|draw| (draw.id, plan(draw)))
                .collect(),
        }
    }

    #[must_use]
    pub fn choice(&self, draw: DrawId) -> Option<Choice> {
        self.draws.get(&draw).map(Planned::choice)
    }

    #[must_use]
    pub fn choices(&self) -> BTreeMap<DrawId, Choice> {
        (self.draws.iter())
            .map(|(draw, planned)| (*draw, planned.choice()))
            .collect()
    }

    /// The sum of the expected GPU times of all variants drawn by the plan, taken from their drawn samples. `None`
    /// while a drawn variant has fewer than two drawn samples, because the time of that variant then comes from its
    /// estimate.
    #[must_use]
    pub fn settled_time(&self) -> Option<Duration> {
        (self.draws.values())
            .filter_map(|planned| match planned {
                Planned::Draw(option) => Some(option.gpu_time.drawn.expected()),
                Planned::Wait => None,
            })
            .sum()
    }
}

/// Plans the effect draws of a frame. Each call of `choose` passes every effect draw that is still in the scene of one
/// surface and returns the plan for those draws, built with `FramePlan::for_draws`. Between calls, the chooser stores
/// the choices for the other surfaces until `forget_surface` removes them.
pub trait QualityChooser {
    fn choose(&mut self, frame: FrameBudget, draws: &[EffectDraw]) -> FramePlan;

    fn forget_surface(&mut self, _surface: SurfaceId) {}
}

/// The variant chosen for one effect draw. `better_fit_since` is the number of drawn samples that the chosen variant
/// had when a better variant began to fit.
#[derive(Default)]
struct ChosenVariant {
    variant: Option<VariantId>,
    better_fit_since: Option<u64>,
}

impl ChosenVariant {
    /// Chooses the variant for `draw` within the GPU time `left`. A lower quality is chosen at once when the chosen
    /// variant no longer fits. A better variant is chosen only after the chosen variant has drawn a full window of new
    /// samples while the better variant fit. The jitter of those samples, which sets the time that the better variant
    /// must leave free, is then measured entirely while the better variant fit. Returns `None` when no variant fits,
    /// which makes the draw wait.
    fn choose(&mut self, draw: &EffectDraw, left: Option<Duration>) -> Option<VariantOption> {
        let best = draw.best_within(left);
        let chosen = (self.variant)
            .and_then(|variant| draw.ranked(variant))
            .filter(|chosen| chosen.option.fits(left));
        match (chosen, best) {
            (Some(chosen), Some(best)) if best.place < chosen.place => {
                Some(self.raise_after_fit(chosen.option, best.option, left))
            }
            (_, best) => self.settle(best.map(|best| best.option)),
        }
    }

    fn raise_after_fit(
        &mut self,
        chosen: VariantOption,
        better: VariantOption,
        left: Option<Duration>,
    ) -> VariantOption {
        let needed = (better.gpu_time.expected())
            .map(|expected| expected + jitter_headroom(chosen, expected));
        if needed.zip(left).is_some_and(|(needed, left)| needed > left) {
            self.better_fit_since = None;
            return chosen;
        }
        let drawn = chosen.gpu_time.drawn;
        let since = *self.better_fit_since.get_or_insert(drawn.samples());
        if !drawn.window_renewed_since(since) {
            return chosen;
        }
        self.better_fit_since = None;
        self.variant = Some(better.variant);
        better
    }

    fn settle(&mut self, best: Option<VariantOption>) -> Option<VariantOption> {
        self.better_fit_since = None;
        if let Some(option) = best {
            self.variant = Some(option.variant);
        }
        best
    }
}

/// The GPU time that must stay free beside the expected time `better` of the better variant. It is `better` scaled by
/// the jitter of the chosen variant relative to the expected time of the chosen variant, and zero while either of those
/// is unknown.
fn jitter_headroom(chosen: VariantOption, better: Duration) -> Duration {
    let drawn = chosen.gpu_time.drawn;
    match (drawn.jitter(), drawn.expected()) {
        (Some(jitter), Some(expected)) if !expected.is_zero() => {
            better.mul_f64(jitter.as_secs_f64() / expected.as_secs_f64())
        }
        _ => Duration::ZERO,
    }
}

/// Chooses for each effect draw the best variant that fits into the GPU time still free in the frame budget. The draws
/// that have waited the most frames are planned first.
#[derive(Default)]
pub struct BudgetChooser {
    chosen: BTreeMap<(SurfaceId, DrawId), ChosenVariant>,
}

impl QualityChooser for BudgetChooser {
    fn choose(&mut self, frame: FrameBudget, draws: &[EffectDraw]) -> FramePlan {
        let mut spent = Duration::ZERO;
        let most_urgent_first = |draw: &EffectDraw| (Reverse(draw.frames_waited), draw.id);
        let plan = FramePlan::for_draws(draws, most_urgent_first, |draw| {
            let left = (frame.gpu_time).map(|gpu_time| gpu_time.saturating_sub(spent));
            let chosen = self.chosen.entry((frame.surface, draw.id)).or_default();
            let Some(option) = chosen.choose(draw, left) else {
                return Planned::Wait;
            };
            if let Some(expected) = option.gpu_time.expected() {
                spent += expected;
            }
            Planned::Draw(option)
        });
        (self.chosen)
            .retain(|(surface, draw), _| *surface != frame.surface || plan.choice(*draw).is_some());
        plan
    }

    fn forget_surface(&mut self, surface: SurfaceId) {
        self.chosen
            .retain(|(chosen_surface, _), _| *chosen_surface != surface);
    }
}

#[cfg(test)]
mod tests;
