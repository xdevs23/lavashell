// SPDX-License-Identifier: GPL-3.0-only
use crate::outline::{Converted, GridSize, Outline, Position, Segment};
use thiserror::Error;
use usvg::tiny_skia_path::{self, PathSegment};
use usvg::{Group, LineCap, LineJoin, Node, Opacity, Paint, Tree};

impl Segment {
    fn end(self) -> Position {
        match self {
            Self::Line(to) | Self::Quadratic { to, .. } | Self::Cubic { to, .. } => to,
        }
    }
}

#[derive(Debug, Error)]
pub enum ConvertError {
    #[error("the file is not an SVG that can be parsed")]
    ParseSvg(#[from] usvg::Error),
    #[error("a shape is transformed")]
    Transformed,
    #[error("a shape is painted with something other than an opaque color")]
    Paint,
    #[error("a stroke is dashed or lacks round caps and joins")]
    StrokeStyle,
    #[error("a group has an opacity, a blend mode, a clip path, a mask or a filter")]
    GroupEffect,
    #[error("the file contains an image or text")]
    Unsupported,
    #[error("the file contains nothing to draw")]
    Empty,
}

pub fn convert(svg: &str) -> Result<Converted, ConvertError> {
    let tree = Tree::from_str(svg, &usvg::Options::default())?;
    let mut paths = Vec::new();
    group_paths(tree.root(), &mut paths)?;
    if paths.is_empty() {
        return Err(ConvertError::Empty);
    }
    let size = tree.size();
    Ok(Converted {
        grid: GridSize {
            width: f64::from(size.width()),
            height: f64::from(size.height()),
        },
        paths,
    })
}

fn group_paths(group: &Group, paths: &mut Vec<Outline>) -> Result<(), ConvertError> {
    if group.should_isolate() {
        return Err(ConvertError::GroupEffect);
    }
    for node in group.children() {
        match node {
            Node::Group(inner) => group_paths(inner, paths)?,
            Node::Path(path) if path.is_visible() => shape_paths(path, paths)?,
            Node::Path(_) => {}
            Node::Image(_) | Node::Text(_) => return Err(ConvertError::Unsupported),
        }
    }
    Ok(())
}

fn shape_paths(path: &usvg::Path, paths: &mut Vec<Outline>) -> Result<(), ConvertError> {
    if !path.abs_transform().is_identity() {
        return Err(ConvertError::Transformed);
    }
    if let Some(fill) = path.fill() {
        solid(fill.paint(), fill.opacity())?;
        for mut outline in outlines(path.data().segments(), None) {
            outline.closed = true;
            paths.push(outline);
        }
    }
    if let Some(stroke) = path.stroke() {
        solid(stroke.paint(), stroke.opacity())?;
        if stroke.dasharray().is_some()
            || stroke.linecap() != LineCap::Round
            || stroke.linejoin() != LineJoin::Round
        {
            return Err(ConvertError::StrokeStyle);
        }
        paths.extend(outlines(
            path.data().segments(),
            Some(f64::from(stroke.width().get())),
        ));
    }
    Ok(())
}

fn solid(paint: &Paint, opacity: Opacity) -> Result<(), ConvertError> {
    match paint {
        Paint::Color(_) if opacity == Opacity::ONE => Ok(()),
        _ => Err(ConvertError::Paint),
    }
}

/// Splits the path `segments` into one outline for each sub-path. `stroke` is the width of the
/// stroke, or `None` for a fill.
fn outlines(segments: impl Iterator<Item = PathSegment>, stroke: Option<f64>) -> Vec<Outline> {
    let mut outlines = Vec::new();
    let mut current: Option<Outline> = None;
    for segment in segments {
        match segment {
            PathSegment::MoveTo(to) => {
                let started = Outline {
                    start: position(to),
                    segments: Vec::new(),
                    closed: false,
                    stroke,
                };
                finish(current.replace(started), &mut outlines);
            }
            PathSegment::Close => {
                if let Some(outline) = current.take() {
                    finish(
                        Some(Outline {
                            closed: true,
                            ..outline
                        }),
                        &mut outlines,
                    );
                }
            }
            PathSegment::LineTo(to) => extend(&mut current, Segment::Line(position(to))),
            PathSegment::QuadTo(control, to) => extend(
                &mut current,
                Segment::Quadratic {
                    control: position(control),
                    to: position(to),
                },
            ),
            PathSegment::CubicTo(first_control, second_control, to) => extend(
                &mut current,
                Segment::Cubic {
                    first_control: position(first_control),
                    second_control: position(second_control),
                    to: position(to),
                },
            ),
        }
    }
    finish(current, &mut outlines);
    outlines
}

fn extend(outline: &mut Option<Outline>, segment: Segment) {
    if let Some(outline) = outline {
        outline.segments.push(segment);
    }
}

/// Adds `outline` to `outlines` if it has at least one segment. An outline that ends at its start
/// point after a close command or after more than one segment is marked closed. Its last segment is
/// then removed if that segment is a line back to the start, because closing the outline draws that
/// line.
fn finish(outline: Option<Outline>, outlines: &mut Vec<Outline>) {
    let Some(mut outline) = outline else {
        return;
    };
    let returns = outline
        .segments
        .last()
        .is_some_and(|last| last.end() == outline.start);
    if returns && (outline.closed || outline.segments.len() > 1) {
        if matches!(outline.segments.last(), Some(Segment::Line(_))) {
            let kept = outline.segments.len() - 1;
            outline.segments.truncate(kept);
        }
        outline.closed = true;
    }
    if !outline.segments.is_empty() {
        outlines.push(outline);
    }
}

fn position(point: tiny_skia_path::Point) -> Position {
    (f64::from(point.x), f64::from(point.y))
}
