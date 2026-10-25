// SPDX-License-Identifier: GPL-3.0-only
mod failures;
mod latch;
mod surfaces;

use super::{warnings, window};
use crate::platform::{NativeSurface, Producer, SIDE, SURFACE_SIZE, TestError};
use crate::renderer::AndroidRenderer;
use crate::surface_stream::{BufferSizes, SurfaceStream};
use crate::{Error, Latch};
use lavashell_core::prelude::*;
use lavashell_core::scene::tree::{Change, Content, Node, Packet, SceneNodeId};
use lavashell_renderer::{
    Fonts, FrameOutcome, PresentationPolicy, SurfaceAlpha, SurfaceTiming, skia,
};
use std::sync::Arc;
use std::time::Duration;

const RED: [u8; 4] = [255, 0, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];

/// The middle of the simulated surface along either axis.
const MIDDLE: i32 = SIDE / 2;

/// Buffers of the size of the simulated surface, which is also the largest size drawn.
const SURFACE_SIZES: BufferSizes = BufferSizes {
    default: SURFACE_SIZE,
    max: SURFACE_SIZE,
};

fn attached() -> Result<(AndroidRenderer, NativeSurface), Error> {
    let surface = NativeSurface::new();
    let mut renderer = AndroidRenderer::new(&Fonts::new())?;
    let timing = SurfaceTiming::new(PresentationPolicy::Automatic);
    renderer.attach(window(&surface, timing), SurfaceAlpha::Opaque)?;
    Ok((renderer, surface))
}

fn whole_surface(image: StreamImage) -> [DrawCommand; 1] {
    let whole = Rect::new(0.dp(), 0.dp(), SIDE.dp(), SIDE.dp());
    [DrawCommand::StreamImage {
        image,
        destination: whole,
        frame: whole,
    }]
}

/// A tree whose only node is a leaf that shows `commands`.
fn scene(commands: impl Into<Arc<[DrawCommand]>>) -> Result<SceneTree, TestError> {
    let leaf = Node::Leaf(Content::Packet(Packet::new(commands.into())));
    let insert = Change::Insert {
        id: SceneNodeId(1),
        parent: None,
        index: 0,
        node: leaf,
    };
    let mut tree = SceneTree::default();
    tree.show(SceneUpdate {
        changes: vec![insert],
    })?;
    Ok(tree)
}

fn render(renderer: &mut AndroidRenderer, image: StreamImage) -> Result<FrameOutcome, TestError> {
    let tree = scene(whole_surface(image))?;
    renderer
        .render(
            tree.at(Duration::ZERO),
            Density::ONE,
            skia::Color::TRANSPARENT,
        )
        .map_err(TestError::from)
}

fn producer(stream: &SurfaceStream) -> Result<Producer, TestError> {
    Producer::of(stream.native_window())
}

/// The image of the frame latched by `renderer` from `stream`, or an error when the latch finds no new frame.
fn latched(
    renderer: &mut AndroidRenderer,
    stream: &mut SurfaceStream,
) -> Result<StreamImage, TestError> {
    match renderer.latch(stream)? {
        Latch::Frame(image) => Ok(image),
        other => Err(TestError::NoNewFrame(other)),
    }
}
