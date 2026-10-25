// SPDX-License-Identifier: GPL-3.0-only
use crate::outline::{self, Converted, Outline, Position};
use lavashell_core::prelude::{Dp, Icon, IconPath, Point, Segment, Size};
use std::sync::LazyLock;

#[derive(Clone, Debug, thiserror::Error)]
pub enum Error {
    #[error("cannot decode the icon {name} that the build encoded")]
    DecodeIcon {
        name: &'static str,
        #[source]
        source: postcard::Error,
    },
}

/// An icon that the build encoded, decoded when it is first used.
pub struct EncodedIcon(pub(crate) LazyLock<Result<Icon, Error>>);

impl EncodedIcon {
    /// The decoded icon.
    ///
    /// # Errors
    ///
    /// Returns [`Error::DecodeIcon`] when the bytes that the build encoded for the icon do not
    /// decode.
    pub fn icon(&'static self) -> Result<&'static Icon, Error> {
        self.0.as_ref().map_err(Clone::clone)
    }
}

/// The icon called `name` that the build encoded into `bytes`.
pub(crate) fn icon(name: &'static str, bytes: &[u8]) -> Result<Icon, Error> {
    let Converted { grid, paths } =
        postcard::from_bytes(bytes).map_err(|source| Error::DecodeIcon { name, source })?;
    Ok(Icon {
        grid: Size::new(Dp::new(grid.width), Dp::new(grid.height)),
        paths: kept(&paths, path),
    })
}

/// Applies `map` to each of `items` and leaks the result as a `'static` slice, as `Icon` requires. The
/// static that contains the decoded icon lives until the program ends, so the memory would never be
/// freed anyway.
fn kept<T, U: 'static>(items: &[T], map: impl Fn(&T) -> U) -> &'static [U] {
    items.iter().map(map).collect::<Vec<_>>().leak()
}

fn path(outline: &Outline) -> IconPath {
    IconPath {
        start: point(outline.start),
        segments: kept(&outline.segments, segment),
        closed: outline.closed,
        stroke: outline.stroke,
    }
}

fn segment(segment: &outline::Segment) -> Segment {
    match *segment {
        outline::Segment::Line(to) => Segment::Line(point(to)),
        outline::Segment::Quadratic { control, to } => Segment::Quadratic {
            control: point(control),
            to: point(to),
        },
        outline::Segment::Cubic {
            first_control,
            second_control,
            to,
        } => Segment::Cubic {
            first_control: point(first_control),
            second_control: point(second_control),
            to: point(to),
        },
    }
}

fn point((x, y): Position) -> Point {
    Point::new(Dp::new(x), Dp::new(y))
}
