// SPDX-License-Identifier: GPL-3.0-only
use serde::{Deserialize, Serialize};

pub type Position = (f64, f64);

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct GridSize {
    pub width: f64,
    pub height: f64,
}

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
