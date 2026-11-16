// SPDX-License-Identifier: GPL-3.0-only
use super::Translator;
use crate::app::{PresentationFeedbackTarget, presentation_flags};
use crate::sctk;
use lavashell_platform::TimingEvent;
use sctk::reexports::client::{Connection, Dispatch, QueueHandle};
use sctk::reexports::protocols::wp::presentation_time::client::{
    wp_presentation::{self, WpPresentation},
    wp_presentation_feedback::{self, WpPresentationFeedback},
};
use std::time::Duration;

impl Dispatch<WpPresentation, ()> for Translator {
    fn event(
        state: &mut Self,
        _: &WpPresentation,
        event: wp_presentation::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wp_presentation::Event::ClockId { clk_id } = event
            && let Some(presentation) = &mut state.presentation
        {
            presentation.read_clock(clk_id, state.links.clock.as_ref());
        }
    }
}

impl Dispatch<WpPresentationFeedback, PresentationFeedbackTarget> for Translator {
    fn event(
        state: &mut Self,
        _: &WpPresentationFeedback,
        event: wp_presentation_feedback::Event,
        data: &PresentationFeedbackTarget,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if !state.surfaces.is_live(data.surface) {
            return;
        }
        let timing = match event {
            wp_presentation_feedback::Event::Presented {
                tv_sec_hi,
                tv_sec_lo,
                tv_nsec,
                refresh,
                flags,
                ..
            } => {
                let absolute =
                    Duration::new((u64::from(tv_sec_hi) << 32) | u64::from(tv_sec_lo), tv_nsec);
                TimingEvent::Presented {
                    surface: data.surface,
                    frame: data.timing,
                    time: (state.presentation.as_ref()).and_then(|presentation| {
                        presentation.observed(absolute, state.links.clock.as_ref())
                    }),
                    refresh: (refresh != 0).then(|| Duration::from_nanos(u64::from(refresh))),
                    flags: presentation_flags(wp_presentation_feedback::Kind::from_bits_truncate(
                        flags.into(),
                    )),
                }
            }
            wp_presentation_feedback::Event::Discarded => TimingEvent::Discarded {
                surface: data.surface,
                frame: data.timing,
            },
            _ => return,
        };
        state.links.time(timing);
    }
}
