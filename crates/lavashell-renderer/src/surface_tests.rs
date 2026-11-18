// SPDX-License-Identifier: GPL-3.0-only
use crate::test_error::TestError;
use crate::test_font::probe_style;
use crate::test_scenes::{TEST_SURFACE, insert};
use crate::test_surfaces::{Pixels, TestSurface, working_pixels};
use crate::{Fonts, Painter, StreamFrames, SurfaceFrame};
use lavashell_core::prelude::*;
use lavashell_core::retained::UnbalancedRestore;
use lavashell_core::scene::tree::*;
use lavashell_test_fixtures::show_all;
use skia_safe::Surface;
use std::{sync::Arc, time::Duration};

const SURFACE: TestSurface = TestSurface::new(320, 320);

const LARGE: Rect = Rect::new(Dp::new(10.0), Dp::new(10.0), Dp::new(300.0), Dp::new(300.0));

const INSIDE: Rect = Rect::new(Dp::new(60.0), Dp::new(60.0), Dp::new(200.0), Dp::new(200.0));

const CUT: Rect = Rect::new(Dp::new(30.0), Dp::new(30.0), Dp::new(200.0), Dp::new(200.0));

const OUTSIDE: Rect = Rect::new(Dp::new(270.0), Dp::new(10.0), Dp::new(40.0), Dp::new(40.0));

const PANEL: Rect = Rect::new(
    Dp::new(100.0),
    Dp::new(100.0),
    Dp::new(200.0),
    Dp::new(140.3),
);

const BADGE: Rect = Rect::new(Dp::new(120.0), Dp::new(150.0), Dp::new(40.0), Dp::new(20.5));
const DOT: Rect = Rect::new(Dp::new(258.0), Dp::new(100.0), Dp::new(4.0), Dp::new(4.0));
const BESIDE: Rect = Rect::new(Dp::new(20.0), Dp::new(40.0), Dp::new(120.0), Dp::new(70.0));
const CORNER: Rect = Rect::new(Dp::new(180.0), Dp::new(40.0), Dp::new(120.0), Dp::new(70.0));
const WHOLE_PIXEL_CLIP: Rect = Rect::new(Dp::ZERO, Dp::ZERO, Dp::new(280.0), Dp::new(320.0));
const HALF_PIXEL_CLIP: Rect = Rect::new(Dp::ZERO, Dp::ZERO, Dp::new(280.5), Dp::new(320.0));
const AROUND_INSIDE: Rect = Rect::new(Dp::new(40.5), Dp::new(40.5), Dp::new(240.0), Dp::new(240.0));

const BETWEEN_STEPS: Color = Color::linear([0.3001, 0.1003, 0.0021], 1.0, Gamut::Rec2020);

const BRIGHT: Color = Color::linear([2.0, 1.5, 1.0], 1.0, Gamut::Rec2020);

fn fill(rect: Rect, color: Color) -> DrawCommand {
    DrawCommand::Fill {
        shape: rect.into(),
        brush: color.into(),
    }
}

fn leaf(commands: Vec<DrawCommand>) -> Result<Node, UnbalancedRestore> {
    Ok(Node::Leaf(Content::Packet(Packet::new(Arc::from(
        commands,
    ))?)))
}

fn large() -> DrawCommand {
    fill(LARGE, Color::rgb(0x00e9_b5a3))
}

fn small() -> DrawCommand {
    fill(INSIDE, Color::rgb(0x00e9_b5a3))
}

fn clip(rect: Rect, radius: Dp) -> RoundedRect {
    RoundedRect::new(rect, Corners::all(radius))
}

fn uneven() -> [RoundedRect; 2] {
    [
        RoundedRect::new(BESIDE, Corners::new(8.dp(), Dp::ZERO, 10.dp(), 10.dp())),
        RoundedRect::new(CORNER, Corners::new(Dp::ZERO, 8.dp(), 10.dp(), 10.dp())),
    ]
}

fn group(opacity: f64, clip: Option<RoundedRect>) -> Node {
    Node::Group(Group {
        transform: Slot::Value(Affine::IDENTITY),
        opacity: Slot::Value(opacity),
        clip: clip.map(Slot::Value),
    })
}

fn tree(changes: Vec<Change>) -> Result<SceneTree, TestError> {
    let mut tree = SceneTree::default();
    show_all(&mut tree, changes)?;
    Ok(tree)
}

fn single(commands: Vec<DrawCommand>) -> Result<SceneTree, TestError> {
    tree(vec![insert(1, None, 0, leaf(commands)?)])
}

fn grouped(group: Node, commands: Vec<DrawCommand>) -> Result<SceneTree, TestError> {
    tree(vec![
        insert(1, None, 0, group),
        insert(2, Some(1), 0, leaf(commands)?),
    ])
}

fn filled(clips: &[RoundedRect], color: Color) -> Result<SceneTree, TestError> {
    let ids = (1_u64..).step_by(2);
    let mut changes = Vec::new();
    for (id, (index, clip)) in ids.zip(clips.iter().enumerate()) {
        changes.push(insert(id, None, index, group(1.0, Some(*clip))));
        changes.push(insert(
            id + 1,
            Some(id),
            0,
            leaf(vec![fill(clip.rect, color)])?,
        ));
    }
    tree(changes)
}

fn nested(
    outer: RoundedRect,
    inner: RoundedRect,
    commands: Vec<DrawCommand>,
) -> Result<SceneTree, TestError> {
    tree(vec![
        insert(1, None, 0, group(1.0, Some(outer))),
        insert(2, Some(1), 0, group(1.0, Some(inner))),
        insert(3, Some(2), 0, leaf(commands)?),
    ])
}

fn stretched() -> Vec<DrawCommand> {
    let half = Rect::new(PANEL.x, PANEL.y / 2.0, PANEL.width, PANEL.height / 2.0);
    vec![
        DrawCommand::Save,
        DrawCommand::Transform(Affine::scale(1.0, 2.0)),
        DrawCommand::Fill {
            shape: Shape::rounded(half, 8.dp()),
            brush: Color::rgb(0x00e9_b5a3).into(),
        },
        DrawCommand::Restore,
    ]
}

fn shadow() -> Result<DrawCommand, TestError> {
    Ok(DrawCommand::Shadow {
        shape: Rect::new(150.dp(), 140.dp(), 100.dp(), 60.dp()).into(),
        shadow: Shadow::new(
            Color::rgb(0x0040_607a),
            Dp::ZERO,
            Point::new(Dp::ZERO, Dp::ZERO),
        )?
        .spread(80.dp())?,
    })
}

fn union() -> Result<DrawCommand, TestError> {
    let halves = [
        Rect::new(90.dp(), 90.dp(), 110.dp(), 160.dp()),
        Rect::new(200.dp(), 90.dp(), 110.dp(), 160.dp()),
    ];
    Ok(DrawCommand::Fill {
        shape: crate::field::test_shapes::union(halves.map(UnionMember::from), Dp::ZERO)?,
        brush: Color::rgb(0x00e9_b5a3).into(),
    })
}

fn paint(
    surface: &mut Surface,
    fonts: &Fonts,
    tree: &SceneTree,
    damages: &[Rect],
) -> Result<Pixels<[f32; 4]>, TestError> {
    let mut painter = Painter::new(fonts, crate::UniformRange::VULKAN_REQUIRED)?;
    painter.set_packet_caching(crate::PacketCaching::Disabled);
    for damage in damages {
        let frame = SurfaceFrame {
            surface: TEST_SURFACE,
            tree: tree.at(Duration::ZERO),
            dp_to_output: Affine::IDENTITY,
            damage: Some(*damage),
            clear: skia_safe::Color::WHITE,
            scene: None,
            shape: None,
            streams: &StreamFrames::default(),
        };
        painter.paint_surface(surface.canvas(), frame)?;
    }
    working_pixels(surface)
}

#[test]
fn a_partial_repaint_on_the_gpu_stores_the_pixels_of_a_full_redraw() -> Result<(), TestError> {
    let cases = [
        ("leaf", single(vec![large()])?),
        ("faded group", grouped(group(0.6, None), vec![small()])?),
        (
            "clipped group",
            grouped(
                group(
                    1.0,
                    Some(clip(
                        Rect::new(20.dp(), 20.dp(), 280.dp(), 280.dp()),
                        Dp::ZERO,
                    )),
                ),
                vec![large()],
            )?,
        ),
        (
            "faded packet",
            grouped(
                group(1.0, None),
                vec![DrawCommand::Opacity(0.6), small(), DrawCommand::Restore],
            )?,
        ),
    ];
    assert_partial_repaints_match(&Fonts::new(), &cases)
}

#[test]
fn a_partial_repaint_on_the_gpu_stores_the_exact_color_of_a_cut_fill() -> Result<(), TestError> {
    let cases = [
        (
            "fill that a full redraw clears",
            single(vec![fill(LARGE, BETWEEN_STEPS)])?,
        ),
        (
            "fill next to a bright fill",
            single(vec![fill(CUT, BETWEEN_STEPS), fill(OUTSIDE, BRIGHT)])?,
        ),
    ];
    assert_partial_repaints_match(&Fonts::new(), &cases)
}

#[test]
fn a_partial_repaint_on_the_gpu_stores_the_edges_of_clipped_fills() -> Result<(), TestError> {
    let panel = || vec![fill(PANEL, Color::rgb(0x00e9_b5a3))];
    let rounded = clip(PANEL, 8.dp());
    let cases = [
        (
            "rounded clip of its size",
            grouped(group(1.0, Some(rounded)), panel())?,
        ),
        (
            "clip of its size between pixels",
            grouped(group(1.0, Some(clip(PANEL, Dp::ZERO))), panel())?,
        ),
        (
            "rounded clip cut by a clip on whole pixels",
            nested(clip(WHOLE_PIXEL_CLIP, Dp::ZERO), rounded, panel())?,
        ),
        (
            "rounded clip cut by a clip between pixels",
            nested(clip(HALF_PIXEL_CLIP, Dp::ZERO), rounded, panel())?,
        ),
        (
            "stretched rounded fill cut by a clip on whole pixels",
            grouped(
                group(1.0, Some(clip(WHOLE_PIXEL_CLIP, Dp::ZERO))),
                stretched(),
            )?,
        ),
        (
            "rounded clip around the repaint area",
            grouped(group(1.0, Some(clip(LARGE, 8.dp()))), vec![large()])?,
        ),
        (
            "rounded clip inside the repaint area",
            grouped(
                group(1.0, Some(clip(BADGE, 8.dp()))),
                vec![fill(BADGE, Color::rgb(0x00e9_b5a3))],
            )?,
        ),
        (
            "fill smaller than its rounded clip",
            grouped(
                group(1.0, Some(rounded)),
                vec![fill(
                    Rect::new(100.dp(), 100.dp(), 200.dp(), 20.dp()),
                    Color::rgb(0x00e9_b5a3),
                )],
            )?,
        ),
        (
            "dot cut by the repaint area",
            grouped(
                group(1.0, Some(clip(DOT, 2.dp()))),
                vec![fill(DOT, Color::rgb(0x0000_0000))],
            )?,
        ),
        (
            "two rounded clips with corners of different radii cut by the repaint area",
            filled(&uneven(), Color::rgb(0x00e9_b5a3))?,
        ),
        (
            "sharp shadow under a rounded clip",
            grouped(group(1.0, Some(rounded)), vec![shadow()?])?,
        ),
        (
            "union under a rounded clip",
            grouped(group(1.0, Some(rounded)), vec![union()?])?,
        ),
    ];
    assert_partial_repaints_match(&Fonts::new(), &cases)
}

#[test]
fn a_partial_repaint_on_the_gpu_stores_the_pixels_of_clipped_text() -> Result<(), TestError> {
    let fonts = crate::test_font::test_font()?;
    let mut style = probe_style();
    style.color = Color::rgb(0x0000_0000);
    let text = DrawCommand::Text {
        text: "Lavashell".into(),
        rect: Rect::new(50.dp(), 100.dp(), 100.dp(), 20.dp()),
        style,
    };
    let cases = [(
        "text cut by the repaint area under a clip between pixels",
        grouped(group(1.0, Some(clip(AROUND_INSIDE, Dp::ZERO))), vec![text])?,
    )];
    assert_partial_repaints_match(&fonts, &cases)
}

#[test]
fn a_fill_under_a_rounded_clip_on_the_gpu_paints_only_the_clip_pixels() -> Result<(), TestError> {
    let cases: [&[RoundedRect]; 2] = [&[clip(DOT, 2.dp())], &uneven()];
    let fonts = Fonts::new();
    let mut gpu = crate::VulkanRenderer::headless()?;
    let size = SURFACE.pixel_size();
    let clear = paint(
        &mut *gpu.offscreen(size)?,
        &fonts,
        &tree(vec![])?,
        &[SURFACE.bounds()],
    )?;
    let mut outside = Vec::new();
    for clips in cases {
        let scene = filled(clips, Color::rgb(0x0000_0000))?;
        let mut changed = Vec::new();
        for damages in [&[SURFACE.bounds()][..], &[SURFACE.bounds(), INSIDE]] {
            let painted = paint(&mut *gpu.offscreen(size)?, &fonts, &scene, damages)?;
            changed.push(pixels_outside(&painted, &clear, clips)?);
        }
        outside.push(changed);
    }
    assert_eq!(
        outside,
        [[0, 0], [0, 0]],
        "changed pixels outside the clips of each tree in a full and a partial repaint"
    );
    Ok(())
}

fn pixels_outside(
    painted: &Pixels<[f32; 4]>,
    clear: &Pixels<[f32; 4]>,
    clips: &[RoundedRect],
) -> Result<usize, TestError> {
    let outside_clips = |&(x, y): &(u16, u16)| {
        let at = Point::new(Dp::new(f64::from(x)), Dp::new(f64::from(y)));
        !clips.iter().any(|clip| clip.rect.contains(at))
    };
    let changed = painted.positions_differing_in_bits(clear)?;
    Ok(changed.into_iter().filter(outside_clips).count())
}

fn assert_partial_repaints_match(
    fonts: &Fonts,
    cases: &[(&str, SceneTree)],
) -> Result<(), TestError> {
    let mut gpu = crate::VulkanRenderer::headless()?;
    let size = SURFACE.pixel_size();
    let mut changed = Vec::new();
    for (name, tree) in cases {
        let full = paint(&mut *gpu.offscreen(size)?, fonts, tree, &[SURFACE.bounds()])?;
        let partial = paint(
            &mut *gpu.offscreen(size)?,
            fonts,
            tree,
            &[SURFACE.bounds(), INSIDE],
        )?;
        let pixels = partial.positions_differing_in_bits(&full)?.len();
        if pixels > 0 {
            changed.push((*name, pixels));
        }
    }
    assert_eq!(changed, [], "changed pixels of each tree");
    Ok(())
}
