// SPDX-License-Identifier: GPL-3.0-only
//! The paths of an icon in the coordinates of its SVG grid. The build script includes this module to
//! write the paths to a file with postcard. The library includes it to read them back.
use serde::{Deserialize, Serialize};

/// A position on the SVG grid.
pub type Position = (f64, f64);

/// The width and the height of the SVG grid.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct GridSize {
    pub width: f64,
    pub height: f64,
}

/// The paths of one icon, made of lines and curves in the coordinates of an SVG grid of size `grid`.
#[derive(Debug, Deserialize, PartialEq, Serialize)]
pub struct Converted {
    pub grid: GridSize,
    pub paths: Vec<Outline>,
}

#[derive(Debug, Deserialize, PartialEq, Serialize)]
pub struct Outline {
    pub start: Position,
    pub segments: Vec<Segment>,
    pub closed: bool,
    /// The width of the stroke in units of the grid, or `None` for a filled path.
    pub stroke: Option<f64>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub enum Segment {
    Line(Position),
    Quadratic {
        control: Position,
        to: Position,
    },
    Cubic {
        first_control: Position,
        second_control: Position,
        to: Position,
    },
}
