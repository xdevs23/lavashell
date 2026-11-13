// SPDX-License-Identifier: GPL-3.0-only
mod failures;
mod latch;
mod surfaces;

use super::warnings;
use crate::Error;
use crate::buffer_importer::HardwareBufferImporter;
use crate::platform::{Producer, SIDE, SURFACE_SIZE, TestError, VulkanRenderer};
use crate::streams::AospStreams;
use crate::surface_stream::{BufferSizes, StreamFormat, SurfaceStream};
use lavashell_core::prelude::*;
use lavashell_core::scene::tree::{Change, Content, Node, Packet, SceneNodeId};
use lavashell_renderer::{
    Fonts, FrameDevice, FrameOutcome, Latch, Painter, Stream, StreamBackend, SurfaceFrame, skia,
};
use std::sync::Arc;

const RED: [u8; 4] = [255, 0, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];

/// The middle of the simulated surface along either axis.
const MIDDLE: i32 = SIDE / 2;

/// Buffers of the size of the simulated surface, which is also the largest size drawn.
const SURFACE_SIZES: BufferSizes = BufferSizes {
    default: SURFACE_SIZE,
    max: SURFACE_SIZE,
};

/// The streams of an AOSP renderer, drawn by the simulated renderer over the whole display at one pixel per dp.
struct Display {
    gpu: VulkanRenderer,
    painter: Painter,
    streams: AospStreams,
}

impl Display {
    fn new() -> Result<Self, TestError> {
        let gpu = VulkanRenderer::new()?;
        let painter = gpu.painter(&Fonts::new())?;
        let streams = HardwareBufferImporter::new(&gpu).map(AospStreams::new)?;
        Ok(Self {
            gpu,
            painter,
            streams,
        })
    }

    fn create_stream(
        &mut self,
        sizes: BufferSizes,
        format: StreamFormat,
    ) -> Result<StreamId, Error> {
        SurfaceStream::new(sizes, format).map(|stream| self.streams.attach(stream))
    }

    fn stream(&mut self, id: StreamId) -> Result<&mut Stream<SurfaceStream>, Error> {
        self.streams.stream(id)
    }

    fn source(&mut self, id: StreamId) -> Result<&mut SurfaceStream, Error> {
        self.stream(id).map(Stream::source_mut)
    }

    fn producer(&mut self, id: StreamId) -> Result<Producer, TestError> {
        producer(self.source(id)?)
    }

    fn latch(&mut self, id: StreamId) -> Result<Latch, Error> {
        self.streams.latch(&self.painter, id)
    }

    /// The image of the frame latched from the stream `id`, or an error when the latch finds no new frame.
    fn latched(&mut self, id: StreamId) -> Result<StreamImage, TestError> {
        match self.latch(id)? {
            Latch::Frame(image) => Ok(image),
            other => Err(TestError::NoNewFrame(other)),
        }
    }

    /// Draws `tree`, and returns the outcome of the frame and the failure of the streams.
    fn draw(&mut self, tree: &SceneTree) -> (Result<FrameOutcome, TestError>, Result<(), Error>) {
        let snapshot = tree.snapshot();
        let shown = snapshot.at(FrameTime::default().presentation);
        let draw_frame =
            |device: FrameDevice<'_, VulkanRenderer>| -> Result<FrameOutcome, TestError> {
                let FrameDevice {
                    gpu,
                    painter,
                    streams,
                } = device;
                let mut painted = Ok(());
                let outcome = gpu.present(|canvas| {
                    let whole = SurfaceFrame {
                        tree: shown,
                        dp_to_output: PixelScale::SQUARE.dp_to_output(Density::ONE),
                        damage: Some(Rect::new(0.dp(), 0.dp(), SIDE.dp(), SIDE.dp())),
                        clear: skia::Color::TRANSPARENT,
                        scene: None,
                        shape: None,
                        streams,
                    };
                    painted = painter.paint_surface(canvas, whole);
                })?;
                painted?;
                Ok(outcome)
            };
        (self.streams).draw(&mut self.gpu, &mut self.painter, shown, draw_frame)
    }

    /// Draws `image` over the whole display. Fails when the frame or the streams fail.
    fn render(&mut self, image: StreamImage) -> Result<FrameOutcome, TestError> {
        let (frame, streams) = self.draw(&scene(whole_surface(image))?);
        let outcome = frame?;
        streams?;
        Ok(outcome)
    }

    fn pixel(&self, x: i32, y: i32) -> Result<[u8; 4], TestError> {
        self.gpu.pixel(x, y)
    }
}

fn producer(stream: &SurfaceStream) -> Result<Producer, TestError> {
    Producer::of(stream.native_window())
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
