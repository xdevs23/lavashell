// SPDX-License-Identifier: GPL-3.0-only
use super::{Extent, LogicalRectangle};
use crate::error::SessionError;

#[test]
fn the_extent_of_outputs_side_by_side_spans_all_of_them() -> Result<(), SessionError> {
    let left = LogicalRectangle {
        left: -1280,
        top: 0,
        right: 0,
        bottom: 1024,
    };
    let right = LogicalRectangle {
        left: 0,
        top: 100,
        right: 1920,
        bottom: 1180,
    };
    let extent = left.union(right).extent()?;
    assert_eq!(
        extent,
        Extent {
            width: 3200,
            height: 1180
        }
    );
    Ok(())
}

#[test]
fn a_rectangle_that_ends_before_it_starts_is_out_of_range() {
    let backwards = LogicalRectangle {
        left: 10,
        top: 0,
        right: 0,
        bottom: 10,
    };
    assert!(matches!(
        backwards.extent(),
        Err(SessionError::ExtentOutOfRange(_))
    ));
}
