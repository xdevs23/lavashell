// SPDX-License-Identifier: GPL-3.0-only
use crate::convert::{ConvertError, convert};
use crate::decode;
use crate::outline::{Converted, GridSize, Outline, Position, Segment};
use lavashell_core::prelude::{Dp, Icon, IconPath, Point, Segment as IconSegment, Size};

#[derive(Debug, thiserror::Error)]
enum TestError {
    #[error("cannot convert the icon")]
    Convert(#[from] ConvertError),
    #[error("cannot encode the icon")]
    Encode(#[from] postcard::Error),
    #[error("cannot decode the icon")]
    Decode(#[from] decode::Error),
    #[error("the icon has {0} paths")]
    PathCount(usize),
    #[error("a segment of the circle is no cubic curve")]
    NotCubic,
}

fn lucide(shapes: &str) -> String {
    let root = concat!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" "#,
        r#"fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" "#,
        r#"stroke-linejoin="round">"#,
    );
    format!("{root}{shapes}</svg>")
}

fn distance((x, y): Position, (center_x, center_y): Position) -> f64 {
    (x - center_x).hypot(y - center_y)
}

#[test]
fn a_line_keeps_its_points_and_a_circle_stays_closed_curves_on_its_circle() -> Result<(), TestError>
{
    let icon = convert(&lucide(
        r#"<path d="m21 21-4.34-4.34" /><circle cx="11" cy="11" r="8" />"#,
    ))?;
    assert_eq!(
        icon.grid,
        GridSize {
            width: 24.0,
            height: 24.0
        }
    );
    let [line, ring] = icon.paths.as_slice() else {
        return Err(TestError::PathCount(icon.paths.len()));
    };
    // usvg returns coordinates as f32. The conversion widens them to f64 without changing their value.
    let end = f64::from(16.66_f32);
    assert_eq!(
        *line,
        Outline {
            start: (21.0, 21.0),
            segments: vec![Segment::Line((end, end))],
            closed: false,
            stroke: Some(2.0),
        }
    );
    assert!(ring.closed && ring.stroke == Some(2.0));
    let center = (11.0, 11.0);
    for segment in &ring.segments {
        let Segment::Cubic { to, .. } = segment else {
            return Err(TestError::NotCubic);
        };
        assert!((distance(*to, center) - 8.0).abs() < 0.001, "{to:?}");
    }
    Ok(())
}

#[test]
fn a_filled_and_stroked_shape_gives_its_closed_fill_before_its_stroke() -> Result<(), ConvertError>
{
    let icon = convert(&lucide(r#"<path d="M4 4h6v6" fill="currentColor" />"#))?;
    let corner = vec![Segment::Line((10.0, 4.0)), Segment::Line((10.0, 10.0))];
    assert_eq!(
        icon.paths,
        [
            Outline {
                start: (4.0, 4.0),
                segments: corner.clone(),
                closed: true,
                stroke: None,
            },
            Outline {
                start: (4.0, 4.0),
                segments: corner,
                closed: false,
                stroke: Some(2.0),
            },
        ]
    );
    Ok(())
}

#[test]
fn a_file_that_cannot_be_drawn_as_solid_paths_is_an_error() {
    assert!(matches!(convert("<svg"), Err(ConvertError::ParseSvg(_))));
    assert!(matches!(
        convert(&lucide(
            r#"<g transform="scale(2)"><path d="M2 2h4" /></g>"#
        )),
        Err(ConvertError::Transformed)
    ));
    for paint in [r#"stroke-opacity="0.5""#, r#"fill="url(#g)""#] {
        let gradient = concat!(
            r#"<linearGradient id="g"><stop offset="0" stop-color="red" />"#,
            r#"<stop offset="1" stop-color="blue" /></linearGradient>"#,
        );
        let svg = lucide(&format!(r#"{gradient}<path d="M2 2h4v4" {paint} />"#));
        assert!(matches!(convert(&svg), Err(ConvertError::Paint)), "{paint}");
    }
    for stroke in [
        r#"stroke-linecap="butt""#,
        r#"stroke-linejoin="miter""#,
        r#"stroke-dasharray="2 2""#,
    ] {
        let svg = lucide(&format!(r#"<path d="M2 2h4v4" {stroke} />"#));
        assert!(
            matches!(convert(&svg), Err(ConvertError::StrokeStyle)),
            "{stroke}"
        );
    }
    let group = lucide(r#"<g opacity="0.5"><path d="M2 2h4" /><path d="M2 4h4" /></g>"#);
    assert!(matches!(convert(&group), Err(ConvertError::GroupEffect)));
    assert!(matches!(convert(&lucide("")), Err(ConvertError::Empty)));
}

#[test]
fn an_open_path_that_returns_to_its_start_is_closed_and_keeps_its_last_curve()
-> Result<(), TestError> {
    let icon = convert(&lucide(
        r#"<path d="M2 9.5c0-3 4-6 10-1c6-5 10-2 10 1c0 4-6 8-10 11c-4-3-10-7-10-11" />"#,
    ))?;
    let [heart] = icon.paths.as_slice() else {
        return Err(TestError::PathCount(icon.paths.len()));
    };
    assert!(heart.closed && heart.stroke == Some(2.0));
    assert_eq!(heart.start, (2.0, 9.5));
    assert_eq!(heart.segments.len(), 4);
    assert!(matches!(
        heart.segments.last(),
        Some(Segment::Cubic { to: (2.0, 9.5), .. })
    ));
    Ok(())
}

#[test]
fn a_line_that_returns_to_the_start_is_the_closing_line() -> Result<(), TestError> {
    let icon = convert(&lucide(r#"<path d="M2 2h4v4L2 2" />"#))?;
    let [triangle] = icon.paths.as_slice() else {
        return Err(TestError::PathCount(icon.paths.len()));
    };
    assert!(triangle.closed);
    assert_eq!(
        triangle.segments,
        [Segment::Line((6.0, 2.0)), Segment::Line((6.0, 6.0))]
    );
    Ok(())
}

#[test]
fn the_build_converts_the_lucide_files() -> Result<(), TestError> {
    assert!(!crate::SEARCH.icon()?.paths.is_empty());
    assert!(
        crate::HEART
            .icon()?
            .paths
            .first()
            .is_some_and(|path| path.closed)
    );
    Ok(())
}

const fn at(x: f64, y: f64) -> Point {
    Point::new(Dp::new(x), Dp::new(y))
}

const DECODED: Icon = Icon {
    grid: Size::new(Dp::new(24.0), Dp::new(16.0)),
    paths: &[
        IconPath {
            start: at(1.0, 2.0),
            segments: &[
                IconSegment::Line(at(3.0, 4.0)),
                IconSegment::Quadratic {
                    control: at(5.0, 6.0),
                    to: at(7.0, 8.0),
                },
                IconSegment::Cubic {
                    first_control: at(9.0, 10.0),
                    second_control: at(11.0, 12.0),
                    to: at(13.0, 14.0),
                },
            ],
            closed: true,
            stroke: Some(2.0),
        },
        IconPath {
            start: at(0.5, 0.5),
            segments: &[],
            closed: false,
            stroke: None,
        },
    ],
};

#[test]
fn an_icon_decodes_into_the_geometry_that_the_build_encoded() -> Result<(), TestError> {
    let converted = Converted {
        grid: GridSize {
            width: 24.0,
            height: 16.0,
        },
        paths: vec![
            Outline {
                start: (1.0, 2.0),
                segments: vec![
                    Segment::Line((3.0, 4.0)),
                    Segment::Quadratic {
                        control: (5.0, 6.0),
                        to: (7.0, 8.0),
                    },
                    Segment::Cubic {
                        first_control: (9.0, 10.0),
                        second_control: (11.0, 12.0),
                        to: (13.0, 14.0),
                    },
                ],
                closed: true,
                stroke: Some(2.0),
            },
            Outline {
                start: (0.5, 0.5),
                segments: Vec::new(),
                closed: false,
                stroke: None,
            },
        ],
    };
    let encoded = postcard::to_allocvec(&converted)?;
    assert_eq!(decode::icon("encoded", &encoded)?, DECODED);
    Ok(())
}

#[test]
fn bytes_that_are_no_encoded_icon_are_an_error_that_names_the_icon() {
    assert!(matches!(
        decode::icon("truncated", &[1]),
        Err(decode::Error::DecodeIcon {
            name: "truncated",
            ..
        })
    ));
}
