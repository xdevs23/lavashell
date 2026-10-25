// SPDX-License-Identifier: GPL-3.0-only
//! A presence and a presence map are read through the same checks as the values that the view makes them from.

use super::{Presence, PresenceMap, ScaleOrigin, Stretch};
use crate::geometry::{FinitePoint, Point, Rect, Scale};
use crate::scene::{Affine, Transform};
use crate::units::{Dp, Fraction, ScaleFactor};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
#[serde(rename = "Presence")]
pub(super) struct PresenceFields {
    opacity: Fraction,
    offset: FinitePoint,
    scale: ScaleFactor,
    origin: ScaleOrigin,
}

impl From<PresenceFields> for Presence {
    fn from(fields: PresenceFields) -> Self {
        Self {
            opacity: fields.opacity.get(),
            offset: fields.offset.get(),
            scale: fields.scale,
            origin: fields.origin,
        }
    }
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename = "PresenceMap")]
pub(super) struct PresenceMapParts {
    outer: Affine,
    stretch: Option<StretchParts>,
    surface: Rect,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename = "Stretch")]
struct StretchParts {
    from: Point,
    origin: Point,
    scale: [f64; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub(super) enum PresenceMapError {
    #[error("the outer transform of a presence must be a finite and invertible scale and move")]
    Outer,
    #[error("the points, the scale and the surface of a presence must be finite")]
    NotFinite,
}

fn finite(numbers: impl IntoIterator<Item = f64>) -> bool {
    numbers.into_iter().all(f64::is_finite)
}

impl StretchParts {
    fn checked(self) -> Result<Stretch, PresenceMapError> {
        let Self {
            from,
            origin,
            scale: [x, y],
        } = self;
        let points = [from.x, from.y, origin.x, origin.y].map(Dp::get);
        if !(finite(points) && finite([x, y])) {
            return Err(PresenceMapError::NotFinite);
        }
        Ok(Stretch {
            from,
            origin,
            scale: Scale { x, y },
        })
    }
}

impl From<PresenceMap> for PresenceMapParts {
    fn from(map: PresenceMap) -> Self {
        Self {
            outer: map.outer.affine(),
            stretch: map.stretch.map(|stretch| StretchParts {
                from: stretch.from,
                origin: stretch.origin,
                scale: [stretch.scale.x, stretch.scale.y],
            }),
            surface: map.surface,
        }
    }
}

impl TryFrom<PresenceMapParts> for PresenceMap {
    type Error = PresenceMapError;

    fn try_from(parts: PresenceMapParts) -> Result<Self, PresenceMapError> {
        let outer = Transform::checked(parts.outer).ok_or(PresenceMapError::Outer)?;
        let stretch = parts.stretch.map(StretchParts::checked).transpose()?;
        let Rect {
            x,
            y,
            width,
            height,
        } = parts.surface;
        if !finite([x, y, width, height].map(Dp::get)) {
            return Err(PresenceMapError::NotFinite);
        }
        Ok(Self::new(outer, stretch, parts.surface))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_error::TestResult;
    use crate::units::DpExt;

    #[test]
    fn a_presence_with_an_opacity_outside_a_fraction_is_not_read() -> TestResult {
        let fields = |opacity: f64| postcard::to_allocvec(&(opacity, (0.0, 0.0), 1.0, (0.5, 0.5)));
        assert!(postcard::from_bytes::<Presence>(&fields(5.0)?).is_err());
        assert!(postcard::from_bytes::<Presence>(&fields(f64::NAN)?).is_err());
        let half = postcard::from_bytes::<Presence>(&fields(0.5)?)?;
        assert_eq!(postcard::to_allocvec(&half)?, fields(0.5)?);
        Ok(())
    }

    #[test]
    fn a_presence_map_reads_back_as_written() -> TestResult {
        let stretch = Stretch {
            from: Point::new(1.dp(), 2.dp()),
            origin: Point::new(3.dp(), 4.dp()),
            scale: Scale { x: 0.5, y: 2.0 },
        };
        let surface = Rect::new(0.dp(), 0.dp(), 40.dp(), 20.dp());
        let outer = Transform::translation(Point::new(5.dp(), 6.dp()));
        let map = PresenceMap::new(outer, Some(stretch), surface);
        let bytes = postcard::to_allocvec(&map)?;
        assert!(postcard::from_bytes::<PresenceMap>(&bytes)? == map);
        Ok(())
    }

    #[test]
    fn a_presence_map_that_rotates_or_is_not_finite_is_not_read() -> TestResult {
        let surface = Rect::new(0.dp(), 0.dp(), 40.dp(), 20.dp());
        let parts = |outer: Affine, surface: Rect| PresenceMapParts {
            outer,
            stretch: None,
            surface,
        };
        let not_finite = Rect::new(Dp::new(f64::NAN), 0.dp(), 1.dp(), 1.dp());
        for parts in [
            parts(Affine::rotate(1.0), surface),
            parts(Affine::scale(0.0, 1.0), surface),
            parts(Affine::IDENTITY, not_finite),
        ] {
            let bytes = postcard::to_allocvec(&parts)?;
            assert!(postcard::from_bytes::<PresenceMap>(&bytes).is_err());
        }
        Ok(())
    }
}
