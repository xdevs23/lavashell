// SPDX-License-Identifier: GPL-3.0-only
use super::StateError;
use lavashell_core::surface::SurfaceId;
use lavashell_core::units::NumberError;
use lavashell_error_chain::next_step::{NamesNextStep, NextStep};

#[test]
fn an_unknown_or_twice_attached_surface_or_an_unfit_buffer_size_asks_for_a_report() {
    let surface = SurfaceId::numbered(1);
    let failures = [
        StateError::UnknownSurface(surface),
        StateError::AttachedTwice(surface),
        StateError::BufferSize(NumberError::NotFinite),
    ];
    assert_eq!(
        failures.each_ref().map(NamesNextStep::next_step),
        [NextStep::ReportTheFailure; 3]
    );
}
