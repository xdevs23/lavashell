// SPDX-License-Identifier: GPL-3.0-only
use crate::failure::DrawFailure;
use ash::vk;
use lavashell_core::error::message_chain;
use lavashell_core::prelude::{NumberError, OutputProfile, PixelSize};
use lavashell_core::scene::FontWeight;
use lavashell_effect_quality::EffectId;
use skia_safe::gpu::DirectContextId;
use std::ffi::CStr;
use std::fmt;
use std::num::TryFromIntError;
use std::path::PathBuf;
use std::str::Utf8Error;

/// The message returned by Skia when it rejects `SkSL` shader source. Skia returns the message only as text.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct SkslDiagnostic(pub(crate) String);

/// The state of a Skia GPU context after a call on it failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContextState {
    Usable,
    DeviceLost,
    OutOfMemory,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("cannot read the font file {}", path.display())]
    ReadFont {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error(
        "cannot decode the font registered as {alias}; register a TrueType or OpenType font file under that alias"
    )]
    DecodeFont { alias: String },
    #[error("cannot read the variation axes of the font registered as {alias}")]
    ReadFontAxes { alias: String },
    #[error("cannot instantiate the font registered as {alias} at weight {}", .weight.get())]
    InstantiateFont { alias: String, weight: FontWeight },
    #[error(
        "the weight {} of the font registered as {alias} lies beyond the range of a Skia scalar",
        .weight.get()
    )]
    FontWeightRange {
        alias: String,
        weight: FontWeight,
        #[source]
        source: DrawFailure,
    },
    #[error("a font is registered twice under the alias {0}")]
    DuplicateFontAlias(String),
    #[error(
        "cannot compile the {program} program, which is a defect in this build of the renderer"
    )]
    CompileProgram {
        program: String,
        #[source]
        source: SkslDiagnostic,
    },
    #[error(
        "the {program} program declares the uniforms {declared:?} where the renderer writes {expected:?}, which is \
         a defect in this build of the renderer"
    )]
    ProgramUniforms {
        program: String,
        declared: Vec<(String, usize)>,
        expected: Vec<(String, usize)>,
    },
    #[error(
        "the {program} program declares the children {declared:?} where the renderer passes {expected:?}, which is \
         a defect in this build of the renderer"
    )]
    ProgramChildren {
        program: String,
        declared: Vec<String>,
        expected: Vec<String>,
    },
    #[error(
        "Skia finds no uniform {name} in the {program} program although the program declares it, which is a defect \
         in this build of the renderer"
    )]
    MissingUniform { program: String, name: String },
    #[error(
        "Skia finds no child {name} in the {program} program although the program declares it, which is a defect in \
         this build of the renderer"
    )]
    MissingChild { program: String, name: &'static str },
    #[error(
        "the uniform range of the device leaves the {program} program no room for its geometry; use a device with \
         a larger maxUniformBufferRange"
    )]
    NoUniformRoom { program: String },
    #[error("Skia reports a negative count for a uniform of the {program} program")]
    UniformCount {
        program: String,
        #[source]
        source: TryFromIntError,
    },
    #[error(
        "the {effect} effect of subpixel text does not compile, which is a defect in this build of the renderer"
    )]
    CompileSubpixelEffect {
        effect: &'static str,
        #[source]
        source: SkslDiagnostic,
    },
    #[error("cannot allocate the coverage tile of subpixel text")]
    SubpixelTile,
    #[error(
        "the coverage tile of subpixel text has no pixels or more bytes than the resource cache holds, although it \
         was sized to fit, which is a defect in this build of the renderer"
    )]
    SubpixelTileSize,
    #[error("Skia cannot sample the coverage mask of subpixel text")]
    SubpixelMask,
    #[error("Skia rejects the blender of subpixel text")]
    SubpixelBlender,
    #[error("the blend tolerance of subpixel text lies beyond the range of a shader float")]
    SubpixelBlendTolerance(#[source] DrawFailure),
    /// `canvas_owner` is `None` when the canvas has no GPU context.
    #[error(
        "the scene image was imported by renderer {importer:?}, but the canvas belongs to {canvas_owner:?}"
    )]
    SceneOfAnotherRenderer {
        importer: DirectContextId,
        canvas_owner: Option<DirectContextId>,
    },
    #[error("the effect {0:?} is registered twice")]
    DuplicateEffect(EffectId),
    #[error("Skia cannot decode the image")]
    DecodeImage,
    #[error("the decoded image has a negative size")]
    DecodedImageSize(#[source] TryFromIntError),
    #[error("Skia cannot read the header of the encoded image")]
    ReadImageHeader,
    #[error("the header of the encoded image states a negative size")]
    ImageHeaderSize(#[source] TryFromIntError),
    #[error("a raster export of {0:?} is too large for Skia")]
    RasterSize((u32, u32)),
    #[error("Skia cannot allocate the surface of a raster export")]
    RasterSurface,
    #[error("the texels along a side of a field texture are no count")]
    TexelCount(#[source] NumberError),
    #[error("a field texture has more texels along a side than a Skia image holds")]
    TexelSide(#[source] TryFromIntError),
    #[error("cannot convert the count {count} for a shader uniform")]
    ShaderCount {
        count: usize,
        #[source]
        source: NumberError,
    },
    #[error("the count {count} for a shader uniform lies beyond the range of a shader float")]
    ShaderCountRange {
        count: usize,
        #[source]
        source: DrawFailure,
    },
    #[error("a shader float does not hold the count {0} exactly")]
    InexactShaderCount(usize),
    #[cfg(target_os = "linux")]
    #[error("the display offers no handle")]
    DisplayHandle(#[source] raw_window_handle::HandleError),
    #[cfg(target_os = "linux")]
    #[error("the display does not belong to Wayland")]
    DisplayNotWayland,
    #[error("cannot list the Vulkan instance extensions")]
    InstanceExtensions(#[source] lavashell_vulkan::Error),
    #[error("cannot create the Vulkan instance")]
    Instance(#[source] lavashell_vulkan::Error),
    #[error("cannot list the Vulkan physical devices")]
    PhysicalDevices(#[source] lavashell_vulkan::Error),
    #[error("cannot query a Vulkan physical device")]
    PhysicalDevice(#[source] lavashell_vulkan::Error),
    #[error("no Vulkan 1.1 device with a graphics queue suits the renderer")]
    NoDevice,
    #[error("cannot create the Vulkan device")]
    Device(#[source] lavashell_vulkan::Error),
    #[error("the Vulkan device reports no device-local memory heap")]
    NoDeviceLocalHeap,
    #[error("the Vulkan extension name {0:?} is not UTF-8")]
    ExtensionName(&'static CStr, #[source] Utf8Error),
    #[error("Skia cannot create a context on the Vulkan device")]
    SkiaContext,
    #[error("a Vulkan handle, version or queue family does not fit its Skia type")]
    SkiaHandoff(#[source] TryFromIntError),
    #[error("Skia abandoned its context after the loss of the Vulkan device and submits nothing")]
    SkiaAbandoned,
    #[error("cannot allocate the surface that tests the mask coverage of the device")]
    CoverageSurface,
    #[error("cannot read back the surface that tests the mask coverage of the device")]
    CoverageReadback,
    #[error("cannot create a GPU timer")]
    CreateTimer(#[source] lavashell_vulkan::Error),
    #[error("cannot start a GPU timer")]
    StartTimer(#[source] lavashell_vulkan::Error),
    #[error("cannot stop a GPU timer")]
    StopTimer(#[source] lavashell_vulkan::Error),
    #[error("cannot read a GPU timer")]
    ReadTimer(#[source] lavashell_vulkan::Error),
    #[error("cannot reset a GPU timer")]
    ResetTimer(#[source] lavashell_vulkan::Error),
    #[error("cannot submit the completion of GPU work")]
    SubmitCompletion(#[source] lavashell_vulkan::Error),
    #[error("cannot read the completion of GPU work")]
    CompletionStatus(#[source] lavashell_vulkan::Error),
    #[error("the Vulkan device cannot import foreign memory")]
    ForeignExtensions(#[source] lavashell_vulkan::Error),
    #[error("cannot export a scene image")]
    ExportScene(#[source] lavashell_vulkan::Error),
    #[error("an imported scene image of {0:?} cannot be sampled as sRGB RGBA")]
    SceneFormat(vk::Format),
    #[error("an imported scene image of {0:?} is too large for Skia")]
    SceneSize(vk::Extent2D),
    #[error("cannot import a scene image")]
    ImportScene(#[source] lavashell_vulkan::Error),
    #[error("cannot take a scene image over from its producer")]
    AcquireScene(#[source] lavashell_vulkan::Error),
    #[error("the scene image is imported again before the renderer handed it back")]
    SceneInUse,
    #[error("Skia cannot wrap an imported scene image")]
    WrapScene,
    #[error(
        "Skia cannot move imported scene images into the layout of their release, which happens after the loss of \
         the Vulkan device"
    )]
    HandBackScenes,
    #[error("cannot hand a scene image back to its producer")]
    ReleaseScene(#[source] lavashell_vulkan::Error),
    #[error("cannot read the hand-back of a scene image to its producer")]
    ReleaseSceneStatus(#[source] lavashell_vulkan::Error),
    #[error("the scene mailbox has no room for another pending scene")]
    SceneMailboxFull,
    #[error("scene generation {generation} is not newer than the published generation {published}")]
    StaleScene { generation: u64, published: u64 },
    #[error("foreign memory of {0:?} is too large for Skia")]
    ForeignSize(PixelSize),
    #[error("cannot import foreign memory")]
    ImportForeign(#[source] lavashell_vulkan::Error),
    #[error("Skia cannot wrap imported foreign memory")]
    WrapForeign,
    #[error(
        "Skia did not hand the frames of streams back to their producers, which happens after the loss of the \
         Vulkan device"
    )]
    HandBackForeign,
    #[error("cannot signal the release fence of the frames of streams")]
    SignalRelease(#[source] lavashell_vulkan::Error),
    #[error("embedded rendering needs Vulkan 1.1, and the host instance has {0:#x}")]
    HostApiVersion(u32),
    #[error("cannot create the semaphore that orders the host before the embedded renderer")]
    ReadySemaphore(#[source] lavashell_vulkan::Error),
    #[error("the embedded renderer cannot draw into {0:?}")]
    EmbeddedFormat(vk::Format),
    #[error("the embedded renderer cannot draw into an image of {0:?}")]
    EmbeddedImageSize((u32, u32)),
    #[error("the host image to draw into is null")]
    EmbeddedImageHandle,
    #[error(
        "the host image has {0:?}, and the embedded renderer needs color attachment and both transfer usages"
    )]
    EmbeddedImageUsage(vk::ImageUsageFlags),
    #[error("Skia cannot take a host image in {0:?}")]
    EmbeddedLayout(vk::ImageLayout),
    #[error(
        "Skia cannot wrap the host image of {format:?} with {usage:?} at {size:?}; its context is {context:?}"
    )]
    WrapHostImage {
        format: vk::Format,
        usage: vk::ImageUsageFlags,
        size: (u32, u32),
        context: ContextState,
    },
    #[error("Skia cannot hand the host image back in {0:?}")]
    EmbeddedReturnLayout(vk::ImageLayout),
    #[error("Skia refused the semaphore that the embedded draw waits for")]
    WaitSemaphore,
    #[error("Skia cannot submit the embedded draw")]
    EmbeddedSubmit,
    #[error("cannot create a Vulkan surface for the window")]
    CreateSurface(#[source] lavashell_vulkan::Error),
    #[error("cannot query the Vulkan surface")]
    SurfaceQuery(#[source] lavashell_vulkan::Error),
    #[error("the graphics queue cannot present to the surface")]
    SurfacePresentation,
    #[error("the surface offers no SDR sRGB format")]
    NoSurfaceFormat,
    #[error("the surface offers no composite alpha mode that the target can use")]
    NoCompositeAlpha,
    #[error("cannot create the swapchain")]
    CreateSwapchain(#[source] lavashell_vulkan::Error),
    #[error("cannot create a semaphore of the swapchain")]
    CreateSemaphore(#[source] lavashell_vulkan::Error),
    #[error("the HDR mastering metadata is invalid")]
    InvalidHdrMetadata,
    #[error("cannot set the HDR mastering metadata of the swapchain")]
    SetHdrMetadata(#[source] lavashell_vulkan::Error),
    #[error("the surface target belongs to another renderer")]
    ForeignTarget,
    #[error("cannot wait for the previous frame of a swapchain slot")]
    WaitFrame(#[source] lavashell_vulkan::Error),
    #[error("cannot acquire a swapchain image")]
    Acquire(#[source] lavashell_vulkan::Error),
    #[error("Skia cannot draw into the swapchain format {0:?}")]
    SwapchainFormat(vk::Format),
    #[error("the swapchain extent is too large for Skia")]
    SwapchainExtent(#[source] std::num::TryFromIntError),
    #[error("Skia cannot wrap the swapchain image; its context is {0:?}")]
    WrapSwapchainImage(ContextState),
    #[error("Skia cannot wait for the acquisition of the swapchain image")]
    WaitAcquisition,
    #[error("Skia does not provide the working color space")]
    WorkingColorSpace,
    #[error("Skia does not provide the color space of {0:?}")]
    OutputColorSpace(OutputProfile),
    #[error("the gain that maps the scene to scRGB lies beyond the range of a Skia scalar")]
    OutputColorFilter(#[source] DrawFailure),
    #[error("cannot allocate the floating-point working surface")]
    WorkingSurface,
    #[error("an offscreen surface of {0:?} is too large for Skia")]
    OffscreenSize((u32, u32)),
    #[error("Skia cannot allocate an offscreen surface of {0:?}")]
    OffscreenSurface((u32, u32)),
    #[error("Skia cannot allocate the surface of a PNG export")]
    PngSurface,
    #[error("Skia cannot encode a PNG export")]
    EncodePng,
    #[error("Skia did not submit the semaphore that presentation waits for")]
    PresentationSemaphore,
    #[error("Skia cannot submit the frame")]
    SubmitFrame,
    #[error("cannot submit the completion of a frame")]
    SignalFrame(#[source] lavashell_vulkan::Error),
    #[error("cannot present a swapchain image")]
    Present(#[source] lavashell_vulkan::Error),
    #[cfg(any(target_os = "android", test))]
    #[error("the Vulkan device does not support VK_GOOGLE_display_timing")]
    NoDisplayTiming,
    #[error("the presentation time {0} is outside 1..=i64::MAX nanoseconds")]
    PresentationTimeRange(u64),
    #[error("the surface requires an explicit timestamp on every frame after its first one")]
    ExplicitTimestampRequired,
    /// Several failures of one call, in the order they happened. After each failure, the call continued with the
    /// work that did not depend on the failed work.
    #[error("{}", InOrder(.0))]
    Several(Vec<Error>),
}

/// Writes failures with their causes, joined in the order they happened.
struct InOrder<'a>(&'a [Error]);

impl fmt::Display for InOrder<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, failure) in self.0.iter().enumerate() {
            if index > 0 {
                formatter.write_str("; after that, ")?;
            }
            formatter.write_str(&message_chain(failure))?;
        }
        Ok(())
    }
}

/// Collects the failures of a call that continues its other work after a failure.
#[derive(Default)]
pub(crate) struct FailureList(Vec<Error>);

impl FailureList {
    pub fn add(&mut self, result: Result<(), Error>) {
        if let Err(error) = result {
            self.push(error);
        }
    }

    pub fn push(&mut self, error: Error) {
        match error {
            Error::Several(errors) => self.0.extend(errors),
            error => self.0.push(error),
        }
    }

    /// `Ok` when no failure was added, the failure itself when one was added, and `Error::Several` when more were
    /// added.
    pub fn into_result(mut self) -> Result<(), Error> {
        if self.0.len() > 1 {
            return Err(Error::Several(self.0));
        }
        self.0.pop().map_or(Ok(()), Err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_error::TestError;

    #[test]
    fn collected_failures_keep_their_order_and_a_single_one_stays_itself() -> Result<(), TestError>
    {
        let mut none = FailureList::default();
        none.add(Ok(()));
        assert!(none.into_result().is_ok());
        let mut one = FailureList::default();
        one.add(Err(Error::WrapScene));
        assert!(matches!(one.into_result(), Err(Error::WrapScene)));
        let mut several = FailureList::default();
        several.push(Error::WrapScene);
        several.push(Error::Several(vec![Error::EncodePng, Error::PngSurface]));
        let Err(Error::Several(errors)) = several.into_result() else {
            return Err(TestError::Unexpected("several failures"));
        };
        assert!(matches!(
            errors.as_slice(),
            [Error::WrapScene, Error::EncodePng, Error::PngSurface]
        ));
        Ok(())
    }
}
