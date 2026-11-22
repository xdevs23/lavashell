// SPDX-License-Identifier: GPL-3.0-only
use crate::geometry::{Axis, Point, Size};
use crate::ui::{Align, Arrangement};
use crate::units::Dp;

pub(super) fn place_in_flow(
    sizes: &[Size],
    in_flow: &[bool],
    actual: Size,
    align: Align,
    gap: Dp,
    arrangement: Arrangement,
) -> Vec<Point> {
    let vertical = matches!(
        arrangement,
        Arrangement::Along {
            axis: Axis::Vertical,
            ..
        }
    );
    let overlay = arrangement == Arrangement::Overlay;
    let length = |size: Size| if vertical { size.height } else { size.width };
    let mut cursor = Dp::ZERO;
    let mut placements = Vec::new();
    for (size, flow) in sizes.iter().zip(in_flow) {
        if !flow {
            placements.push(Point::default());
            continue;
        }
        let extra = if vertical {
            actual.width - size.width
        } else {
            actual.height - size.height
        };
        let cross = match align {
            Align::Start => Dp::ZERO,
            Align::Center => extra / 2.0,
            Align::End => extra,
        };
        placements.push(if overlay {
            Point::default()
        } else if vertical {
            Point {
                x: cross,
                y: cursor,
            }
        } else {
            Point {
                x: cursor,
                y: cross,
            }
        });
        cursor += length(*size);
        cursor += gap;
    }
    placements
}

pub(super) fn flow_length(sizes: &[Size], in_flow: &[bool], axis: Axis, gap: Dp) -> Dp {
    (sizes.iter().zip(in_flow))
        .filter_map(|(size, flow)| flow.then_some(axis.length(*size)))
        .reduce(|stacked, length| stacked + gap + length)
        .unwrap_or(Dp::ZERO)
}
