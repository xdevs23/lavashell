// SPDX-License-Identifier: GPL-3.0-only
use super::{DrawCommand, Extent, Rect};
use crate::effects::{BlendMode, blur_reach};
use crate::output::finest_step;
use crate::scene::{Affine, Brush, path_points};
use crate::shape::{Corners, Shape};
use crate::units::Dp;

/// The area that a packet of draw commands paints and the area that it samples, in the coordinates of the packet.
/// Antialiased edges and bilinear sampling extend one device pixel past that geometry. Code that uses a footprint
/// adds that pixel in device space. A packet is analyzed before the target into which it is drawn is known. Its
/// blurs therefore count as far as they change an output at the highest bit depth.
#[derive(Clone, Debug, PartialEq)]
pub struct Footprint {
    /// The area painted by the packet, or None when the packet paints nothing.
    pub output: Option<Extent>,
    /// The area of the destination, painted before this packet, that this packet samples.
    pub backdrop: Option<Extent>,
    cover: Option<Cover>,
    /// False when a command draws content that must not be cached, such as revealed protected text or a stream
    /// frame, even when that content does not depend on the destination. A renderer must not store a rendered image
    /// of such a packet or its commands in a cache that it reuses.
    pub cacheable: bool,
}

/// The rectangle that the first command fills with an opaque color, and the area that the later commands paint and
/// sample. `analyze` drops the cover once that area extends outside the rectangle.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Cover {
    rect: Rect,
    inner: Option<Extent>,
}

impl Footprint {
    pub fn analyze(commands: &[DrawCommand]) -> Self {
        let mut result = Self {
            output: None,
            backdrop: None,
            cover: (commands.first().and_then(opaque_rectangle))
                .map(|rect| Cover { rect, inner: None }),
            cacheable: true,
        };
        let mut transform = Affine::IDENTITY;
        // How far the effect layers around the command spread its output.
        let mut reach = Dp::ZERO;
        let mut scopes = Vec::new();
        for (index, command) in commands.iter().enumerate() {
            result.cacheable &= !command.is_transient();
            match command {
                DrawCommand::Save | DrawCommand::Opacity(_) | DrawCommand::OpacityLayer { .. } => {
                    scopes.push((transform, reach));
                    continue;
                }
                DrawCommand::Restore => {
                    (transform, reach) = scopes.pop().unwrap_or((Affine::IDENTITY, Dp::ZERO));
                    continue;
                }
                DrawCommand::Transform(affine) => {
                    transform = transform.then(*affine);
                    continue;
                }
                DrawCommand::EffectLayer { blur, filter, .. } => {
                    scopes.push((transform, reach));
                    let spread = blur.get() * blur_reach(finest_step())
                        + filter
                            .as_ref()
                            .map_or(Dp::ZERO, |filter| filter.outset.get());
                    reach += spread * transform.stretch();
                }
                _ => {}
            }
            let Some((output, read)) = extents(command, transform, reach) else {
                continue;
            };
            let output = output.mapped(transform).outset(reach);
            if index > 0 {
                result.cover = result.cover.and_then(|cover| {
                    let inside = output.inside(cover.rect)
                        && read.is_none_or(|read| read.inside(cover.rect));
                    let inner = [read, cover.inner]
                        .into_iter()
                        .flatten()
                        .fold(output, Extent::union);
                    inside.then_some(Cover {
                        inner: Some(inner),
                        ..cover
                    })
                });
            }
            result.output = Some(
                result
                    .output
                    .map_or(output, |painted| painted.union(output)),
            );
            if let Some(read) = read {
                result.backdrop = Some(result.backdrop.map_or(read, |old| old.union(read)));
            }
        }
        result
    }

    /// The rectangle that the first command fills with an opaque color, when every later command paints and
    /// samples only inside that rectangle. The check includes the antialiased edges and the sampled pixels, which
    /// extend `pixel` past the geometry of the commands.
    #[must_use]
    pub fn opaque(&self, pixel: Dp) -> Option<Rect> {
        let cover = self.cover?;
        (cover.inner)
            .is_none_or(|inner| inner.outset(pixel).inside(cover.rect))
            .then_some(cover.rect)
    }
}

/// The extent that `command` paints and the extent that it reads, or `None` for a command that paints nothing. The
/// painted extent is returned before `transform` and the spread of the surrounding effect layers apply to it. The
/// read extent is already mapped by `transform`.
fn extents(
    command: &DrawCommand,
    transform: Affine,
    reach: Dp,
) -> Option<(Extent, Option<Extent>)> {
    let extents = match command {
        DrawCommand::Shadow { shape, shadow } => (
            shape.bounds().outset(shadow.reach(finest_step())).into(),
            None,
        ),
        DrawCommand::Stroke { shape, stroke } => {
            (shape.bounds().outset(stroke.reach()).into(), None)
        }
        DrawCommand::Fill { shape, .. } => (shape.bounds().into(), None),
        DrawCommand::Backdrop { shape, effect } => {
            let bounds = shape.bounds();
            let read = Extent::from(bounds.outset(effect.reach(finest_step())));
            (bounds.into(), Some(read.mapped(transform)))
        }
        DrawCommand::EffectLayer { bounds, blend, .. } => (
            (*bounds).into(),
            (*blend != BlendMode::SourceOver)
                .then(|| Extent::from(*bounds).mapped(transform).outset(reach)),
        ),
        DrawCommand::Text { rect, .. } | DrawCommand::PrivateText { rect, .. } => {
            ((*rect).into(), None)
        }
        DrawCommand::Image { destination, .. } | DrawCommand::StreamImage { destination, .. } => {
            ((*destination).into(), None)
        }
        DrawCommand::Circle { center, radius, .. } => (
            Rect::new(
                center.x - *radius,
                center.y - *radius,
                *radius * 2.0,
                *radius * 2.0,
            )
            .into(),
            None,
        ),
        DrawCommand::Path {
            start,
            segments,
            stroke,
            ..
        } => {
            let path = Rect::bounding(path_points(*start, segments))?;
            (path.outset(stroke.unwrap_or(Dp::ZERO).abs()).into(), None)
        }
        _ => return None,
    };
    Some(extents)
}

fn opaque_rectangle(command: &DrawCommand) -> Option<Rect> {
    let DrawCommand::Fill {
        shape: Shape::Rounded(shape),
        brush: Brush::Solid(color),
    } = command
    else {
        return None;
    };
    let Extent::Within(rect) = Extent::from(shape.rect) else {
        return None;
    };
    let area = rect.width > Dp::ZERO && rect.height > Dp::ZERO;
    (area && shape.corners == Corners::default() && color.alpha() >= 1.0).then_some(rect)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effects::{Backdrop, BorderPlacement, Glass, Shadow, Stroke};
    use crate::geometry::{Color, PixelSize, Point};
    use crate::image::{StreamId, StreamImage};
    use crate::protected_text::ProtectedText;
    use crate::scene::TextStyle;
    use crate::shape::Shape;
    use crate::test_error::{Required, TestResult};
    use crate::units::{Density, DpExt, NonNegative, NonNegativeDp, PxExt, SpExt};

    fn style() -> TextStyle<Dp> {
        TextStyle::new("sans-serif", 14.sp(), Color::WHITE).resolve_unscaled()
    }

    #[test]
    fn revealed_private_text_is_never_cached() -> TestResult {
        let private = ProtectedText::new();
        assert_eq!(private.replace(0..0, "synthetic"), 9);
        private.set_revealed(true);
        let bounds = Rect::new(0.dp(), 0.dp(), 100.dp(), 40.dp());
        let revealed = Footprint::analyze(&[DrawCommand::PrivateText {
            text: private.presentation('•').required("presentation")?,
            rect: bounds,
            style: style(),
        }]);
        assert!(!revealed.cacheable);
        let plain = Footprint::analyze(&[text(bounds)]);
        assert!(plain.cacheable);
        Ok(())
    }

    #[test]
    fn packets_with_a_stream_image_paint_its_destination_and_are_never_cached() {
        let destination = Rect::new(10.dp(), 5.dp(), 60.dp(), 30.dp());
        let stream = DrawCommand::StreamImage {
            image: StreamImage {
                stream: StreamId::next(),
                generation: 1,
                size: PixelSize::new(120, 60),
            },
            destination,
            frame: Rect::from_size(destination.size()),
        };
        let footprint = Footprint::analyze(&[stream]);
        assert!(!footprint.cacheable);
        assert!(covers(footprint.output, destination));
    }

    fn covers(outer: Option<Extent>, inner: Rect) -> bool {
        matches!(outer, Some(Extent::Within(outer)) if Extent::from(inner).inside(outer))
    }

    fn fill(rect: Rect) -> DrawCommand {
        DrawCommand::Fill {
            shape: rect.into(),
            brush: Color::WHITE.into(),
        }
    }

    fn text(rect: Rect) -> DrawCommand {
        DrawCommand::Text {
            text: "ordinary".into(),
            rect,
            style: style(),
        }
    }

    #[test]
    fn a_packet_that_paints_inside_its_first_opaque_fill_is_covered_by_it() {
        let card = Rect::new(4.dp(), 4.dp(), 40.dp(), 30.dp());
        let inside = Rect::new(6.dp(), 6.dp(), 36.dp(), 26.dp());
        let footprint = Footprint::analyze(&[
            fill(card),
            DrawCommand::Save,
            DrawCommand::Opacity(0.5),
            fill(inside),
            DrawCommand::Restore,
            text(inside),
        ]);
        assert_eq!(footprint.opaque(Density::ONE.to_dp(1.px())), Some(card));
    }

    #[test]
    fn a_first_fill_that_is_translucent_rounded_or_painted_past_covers_nothing() -> TestResult {
        let card = Rect::new(4.dp(), 4.dp(), 40.dp(), 30.dp());
        let translucent = DrawCommand::Fill {
            shape: card.into(),
            brush: Color::WHITE.with_alpha(0.99).into(),
        };
        let rounded = DrawCommand::Fill {
            shape: Shape::rounded(card, 1.dp()),
            brush: Color::WHITE.into(),
        };
        // The antialiased edge of text that starts at the side of the card extends past the card.
        let edge = text(Rect::new(4.dp(), 6.dp(), 36.dp(), 26.dp()));
        let glass = DrawCommand::Backdrop {
            shape: Rect::new(10.dp(), 10.dp(), 20.dp(), 10.dp()).into(),
            effect: Backdrop::glass(
                8.dp(),
                Glass {
                    refraction: NonNegativeDp::try_new(12.dp())?,
                    dispersion: NonNegativeDp::try_new(0.7.dp())?,
                    rim: NonNegative::try_new(0.14)?,
                },
            )?,
        };
        for commands in [
            vec![translucent],
            vec![rounded],
            vec![fill(card), edge],
            vec![fill(card), glass],
            vec![text(card), fill(card)],
        ] {
            let opaque = Footprint::analyze(&commands).opaque(Density::ONE.to_dp(1.px()));
            assert_eq!(opaque, None, "{commands:?}");
        }
        Ok(())
    }

    /// A quarter turn maps the rectangle with the corners (0, 0) and (20, 10) onto the rectangle with the corners
    /// (-10, 0) and (0, 20).
    #[test]
    fn commands_under_a_transform_paint_their_mapped_rectangle() {
        let quarter_turn = Affine {
            linear: [0.0, -1.0, 1.0, 0.0],
            translation: Point::default(),
        };
        let footprint = Footprint::analyze(&[
            DrawCommand::Save,
            DrawCommand::Transform(quarter_turn),
            fill(Rect::new(0.dp(), 0.dp(), 20.dp(), 10.dp())),
            DrawCommand::Restore,
            fill(Rect::new(1.dp(), 1.dp(), 2.dp(), 2.dp())),
        ]);
        let painted = Rect::new(-10.dp(), 0.dp(), 13.dp(), 20.dp());
        assert_eq!(footprint.output, Some(Extent::Within(painted)));
    }

    /// A blur of 1 dp spreads `blur_reach(finest_step())` dp, and twice that under a map that doubles lengths.
    #[test]
    fn a_blur_under_a_scale_spreads_by_the_scaled_reach() -> TestResult {
        let rect = Rect::new(0.dp(), 0.dp(), 10.dp(), 10.dp());
        let footprint = Footprint::analyze(&[
            DrawCommand::Save,
            DrawCommand::Transform(Affine::scale(2.0, 2.0)),
            DrawCommand::EffectLayer {
                bounds: rect,
                blend: BlendMode::SourceOver,
                blur: NonNegativeDp::try_new(1.dp())?,
                filter: None,
            },
            fill(rect),
            DrawCommand::Restore,
            DrawCommand::Restore,
        ]);
        let mapped = Rect::new(0.dp(), 0.dp(), 20.dp(), 20.dp());
        let painted = mapped.outset(1.dp() * blur_reach(finest_step()) * 2.0);
        assert_eq!(footprint.output, Some(Extent::Within(painted)));
        Ok(())
    }

    #[test]
    fn commands_that_paint_nothing_have_no_output() {
        let footprint = Footprint::analyze(&[
            DrawCommand::Save,
            DrawCommand::Transform(Affine::scale(2.0, 2.0)),
            DrawCommand::Restore,
        ]);
        assert_eq!(footprint.output, None);
    }

    #[test]
    fn geometry_past_finite_numbers_paints_everywhere_and_covers_nothing() {
        let footprint = Footprint::analyze(&[
            fill(Rect::new(-Dp::MAX, 0.dp(), 1.dp(), 1.dp())),
            fill(Rect::new(Dp::MAX - 1.dp(), 0.dp(), 1.dp(), 1.dp())),
        ]);
        assert_eq!(footprint.output, Some(Extent::Everywhere));
        assert_eq!(footprint.opaque(Density::ONE.to_dp(1.px())), None);
    }

    #[test]
    fn footprints_cover_every_pixel_that_effects_read_or_paint() -> TestResult {
        let bounds = Rect::new(0.dp(), 0.dp(), 10.dp(), 10.dp());
        let shape = Shape::rounded(bounds, 2.dp());
        let analyze = |command| Footprint::analyze(&[command]);
        // A blur is visible up to `blur_reach(finest_step())` sigmas away. A lens samples as far as it refracts and
        // disperses.
        let glass = Glass {
            refraction: NonNegativeDp::try_new(300.dp())?,
            dispersion: NonNegativeDp::try_new(20.dp())?,
            rim: NonNegative::ZERO,
        };
        let effect = Backdrop::glass(200.dp(), glass)?;
        let read = analyze(DrawCommand::Backdrop {
            shape: shape.clone(),
            effect,
        });
        let lens = 300.dp() + 20.dp();
        assert!(covers(
            read.backdrop,
            bounds.outset(200.dp() * blur_reach(finest_step()) + lens)
        ));
        // A shadow paints its shape moved by the offset, grown by the spread and blurred.
        let offset = Point::new(-30.dp(), 12.dp());
        let shadow = Shadow::new(Color::WHITE, 6.dp(), offset)?.spread(4.dp())?;
        let painted = bounds
            .translated(offset)
            .outset(4.dp() + 6.dp() * blur_reach(finest_step()));
        let output = analyze(DrawCommand::Shadow {
            shape: shape.clone(),
            shadow,
        });
        assert!(covers(output.output, painted));
        // An outside stroke paints its whole width beyond the shape, a centred one half of it.
        for (placement, beyond) in [
            (BorderPlacement::Outside, 8.dp()),
            (BorderPlacement::Center, 4.dp()),
        ] {
            let stroke = Stroke::new(Color::WHITE, 8.dp())?.placement(placement);
            let output = analyze(DrawCommand::Stroke {
                shape: shape.clone(),
                stroke,
            });
            assert!(covers(output.output, bounds.outset(beyond)));
        }
        Ok(())
    }
}
