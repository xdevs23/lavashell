// SPDX-License-Identifier: GPL-3.0-only
use crate::geometry::{Axis, Point, Size};
use crate::ui::{Align, Arrangement};
use crate::units::Dp;

/// The position of each child inside the content area of its parent, which has the size `actual`. Children in the
/// flow follow each other along the axis of `arrangement` with `gap` between them, and `align` places them across
/// that axis. In an overlay, every child is at the start. Children out of the flow are at the start too.
pub(super) fn place_in_flow(
    sizes: &[Size],
    in_flow: &[bool],
    actual: Size,
    align: Align,
    gap: Dp,
    arrangement: Arrangement,
) -> Vec<Point> {
    let vertical = arrangement == Arrangement::Along(Axis::Vertical);
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
