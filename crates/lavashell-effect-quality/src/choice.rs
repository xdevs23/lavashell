// SPDX-License-Identifier: GPL-3.0-only
use super::{EffectId, GpuTime, OfferedVariant, VariantId};
use lavashell_core::scene::tree::NodeKey;
use lavashell_core::surface::{PerSurface, SurfaceId};
use std::{cmp::Reverse, collections::BTreeMap, time::Duration};

#[derive(Clone, Copy, Debug)]
pub struct FrameBudget {
    pub surface: SurfaceId,
    pub gpu_time: Option<Duration>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DrawId {
    pub surface: SurfaceId,
    pub leaf: NodeKey,
    pub ordinal: u64,
    pub effect: EffectId,
}

#[derive(Clone, Copy, Debug)]
pub struct VariantOption {
    pub offered: OfferedVariant,
    pub gpu_time: GpuTime,
}

impl VariantOption {
    #[must_use]
    pub fn variant(&self) -> VariantId {
        self.offered.variant
    }

    fn is_preferred_to(&self, other: &Self) -> bool {
        self.offered.preference() > other.offered.preference()
    }

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
    pub options: Vec<VariantOption>,
}

impl EffectDraw {
    #[must_use]
    pub fn best_option(&self) -> Option<VariantOption> {
        self.best_within(None)
    }

    fn option(&self, variant: VariantId) -> Option<VariantOption> {
        (self.options.iter().copied()).find(|option| option.variant() == variant)
    }

    fn best_within(&self, left: Option<Duration>) -> Option<VariantOption> {
        (self.options.iter().copied())
            .filter(|option| option.fits(left))
            .max_by_key(|option| option.offered.preference())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Draw(VariantId),
    Wait,
}

#[derive(Clone, Copy, Debug)]
pub enum Planned {
    Draw(VariantOption),
    Wait,
}

impl Planned {
    #[must_use]
    pub fn choice(&self) -> Choice {
        match self {
            Self::Draw(option) => Choice::Draw(option.variant()),
            Self::Wait => Choice::Wait,
        }
    }
}

#[derive(Clone, Debug)]
pub struct FramePlan {
    draws: BTreeMap<DrawId, Planned>,
}

impl FramePlan {
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

pub trait QualityChooser: PerSurface {
    fn choose(&mut self, frame: FrameBudget, draws: &[EffectDraw]) -> FramePlan;
}

#[derive(Default)]
struct ChosenVariant {
    variant: Option<VariantId>,
    better_fit_since: Option<u64>,
}

impl ChosenVariant {
    fn choose(&mut self, draw: &EffectDraw, left: Option<Duration>) -> Option<VariantOption> {
        let best = draw.best_within(left);
        let chosen = (self.variant)
            .and_then(|variant| draw.option(variant))
            .filter(|chosen| chosen.fits(left));
        match (chosen, best) {
            (Some(chosen), Some(best)) if best.is_preferred_to(&chosen) => {
                Some(self.raise_after_fit(chosen, best, left))
            }
            (_, best) => self.settle(best),
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
        self.variant = Some(better.variant());
        better
    }

    fn settle(&mut self, best: Option<VariantOption>) -> Option<VariantOption> {
        self.better_fit_since = None;
        if let Some(option) = best {
            self.variant = Some(option.variant());
        }
        best
    }
}

fn jitter_headroom(chosen: VariantOption, better: Duration) -> Duration {
    let drawn = chosen.gpu_time.drawn;
    match (drawn.jitter(), drawn.expected()) {
        (Some(jitter), Some(expected)) if !expected.is_zero() => {
            better.mul_f64(jitter.as_secs_f64() / expected.as_secs_f64())
        }
        _ => Duration::ZERO,
    }
}

#[derive(Default)]
pub struct BudgetChooser {
    chosen: BTreeMap<DrawId, ChosenVariant>,
}

impl QualityChooser for BudgetChooser {
    fn choose(&mut self, frame: FrameBudget, draws: &[EffectDraw]) -> FramePlan {
        let mut spent = Duration::ZERO;
        let most_urgent_first = |draw: &EffectDraw| (Reverse(draw.frames_waited), draw.id);
        let plan = FramePlan::for_draws(draws, most_urgent_first, |draw| {
            let left = (frame.gpu_time).map(|gpu_time| gpu_time.saturating_sub(spent));
            let chosen = self.chosen.entry(draw.id).or_default();
            let Some(option) = chosen.choose(draw, left) else {
                return Planned::Wait;
            };
            if let Some(expected) = option.gpu_time.expected() {
                spent += expected;
            }
            Planned::Draw(option)
        });
        (self.chosen)
            .retain(|draw, _| draw.surface != frame.surface || plan.choice(*draw).is_some());
        plan
    }
}

impl PerSurface for BudgetChooser {
    fn forget_surface(&mut self, surface: SurfaceId) {
        self.chosen.retain(|draw, _| draw.surface != surface);
    }
}

#[cfg(test)]
mod tests;
