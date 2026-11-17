// SPDX-License-Identifier: GPL-3.0-only
use crate::failure::DrawFailure;
use crate::fault::{DefectNotice, Fault};
use crate::fonts::RegisteredFont;
use ash::vk;
use lavashell_core::error::message_chain;
use lavashell_core::prelude::{NumberError, OutputProfile, PixelSize};
use lavashell_core::scene::FontWeight;
use lavashell_effect_quality::EffectId;
use skia_safe::gpu::DirectContextId;
use std::collections::BTreeMap;
use std::ffi::CStr;
use std::fmt;
use std::num::TryFromIntError;
use std::path::PathBuf;
use std::str::Utf8Error;

const FONT_NEXT_STEP: &str = "register a different TrueType or OpenType font file under that alias";

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct SkslDiagnostic(pub(crate) String);

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
    #[error("cannot decode the {font}; {}", FONT_NEXT_STEP)]
    DecodeFont { font: RegisteredFont },
    #[error("cannot read the variation axes of the {font}; {}", FONT_NEXT_STEP)]
    ReadFontAxes { font: RegisteredFont },
    #[error("cannot instantiate the {font} at weight {}; {}", .weight.get(), FONT_NEXT_STEP)]
    InstantiateFont {
        font: RegisteredFont,
        weight: FontWeight,
    },
    #[error("a font is registered twice under the alias {0}")]
    DuplicateFontAlias(String),
    #[error("the font alias {alias} is registered with an empty list of font files")]
    NoFontFiles { alias: String },
    #[error("cannot compile the {program} program. {DefectNotice}")]
    CompileProgram {
        program: String,
        #[source]
        source: SkslDiagnostic,
    },
    #[error(
        "the {program} program declares the uniforms {declared:?} where the renderer writes {expected:?}. {DefectNotice}"
    )]
    ProgramUniforms {
        program: String,
        declared: BTreeMap<String, usize>,
        expected: BTreeMap<String, usize>,
    },
    #[error(
        "the {program} program declares the children {declared:?} where the renderer passes {expected:?}. {DefectNotice}"
    )]
    ProgramChildren {
        program: String,
        declared: Vec<String>,
        expected: Vec<String>,
    },
    #[error(
        "Skia finds no uniform {name} in the {program} program although the program declares it. {DefectNotice}"
    )]
    MissingUniform { program: String, name: String },
    #[error(
        "Skia finds no child {name} in the {program} program although the program declares it. {DefectNotice}"
    )]
    MissingChild { program: String, name: &'static str },
    #[error(
        "the uniform range of the device leaves the {program} program no room for its geometry; use a device with \
         a larger maxUniformBufferRange"
    )]
    NoUniformRoom { program: String },
    #[error("the uniform range of the device lies beyond the address space of this platform")]
    UniformRangeSize(#[source] TryFromIntError),
    #[error("Skia reports a negative count for a uniform of the {program} program")]
    UniformCount {
        program: String,
        #[source]
        source: TryFromIntError,
    },
    #[error("the {effect} effect of subpixel text does not compile. {DefectNotice}")]
    CompileSubpixelEffect {
        effect: &'static str,
        #[source]
        source: SkslDiagnostic,
    },
    #[error("cannot allocate the coverage tile of subpixel text")]
    SubpixelTile,
    #[error(
        "the coverage tile of subpixel text is empty or has more bytes than the resource cache allows, although \
         the renderer sized it to fit. {DefectNotice}"
    )]
    SubpixelTileSize,
    #[error("Skia cannot sample the coverage mask of subpixel text")]
    SubpixelMask,
    #[error("Skia rejects the blender of subpixel text")]
    SubpixelBlender,
    #[error("subpixel text is drawn onto a surface whose color type Skia does not know")]
    SubpixelColorType,
    #[error("the blend tolerance of subpixel text lies beyond the range of a shader float")]
    SubpixelBlendTolerance(#[source] DrawFailure),
    #[error(
        "the scene image was imported by renderer {importer:?}, but the canvas belongs to {canvas_owner:?}"
    )]
    SceneOfAnotherRenderer {
        importer: DirectContextId,
        canvas_owner: Option<DirectContextId>,
    },
    #[error("the effect {0:?} is registered twice")]
    DuplicateEffect(EffectId),
    #[error("two text rasterizers are registered for {0:?} antialiasing")]
    DuplicateTextRasterizer(crate::text::TextAntialiasing),
    #[error("Skia cannot decode the image")]
    DecodeImage,
    #[error("the surface of a PNG image could not get an id")]
    NumberImageSurface(#[source] lavashell_core::id_counter::IdsExhausted),
    #[error("the decoded image has a negative size")]
    DecodedImageSize(#[source] TryFromIntError),
    #[error("Skia cannot read the header of the encoded image")]
    ReadImageHeader,
    #[error("the header of the encoded image states a negative size")]
    ImageHeaderSize(#[source] TryFromIntError),
    #[error("a raster export of {width}x{height} pixels is too large for Skia")]
    RasterSize { width: u32, height: u32 },
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
    EmbeddedImageSize(PixelSize),
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
        size: PixelSize,
        context: ContextState,
    },
    #[error("Skia cannot hand the host image back in {0:?}")]
    EmbeddedReturnLayout(vk::ImageLayout),
    #[error("Skia could not add a wait on the semaphore that the embedded draw waits for")]
    WaitSemaphore,
    #[error(
        "cannot wait on the ready semaphore of a failed embedded draw, so the semaphore is still signaled"
    )]
    ConsumeReadySignal(#[source] lavashell_vulkan::Error),
    #[error("Skia cannot submit the embedded draw")]
    EmbeddedSubmit,
    #[error("cannot create a Vulkan surface for the window")]
    CreateSurface(#[source] lavashell_vulkan::Error),
    #[error("cannot ask whether the graphics queue can present to the Vulkan surface")]
    QuerySurfacePresentation(#[source] lavashell_vulkan::Error),
    #[error("cannot read the capabilities of the Vulkan surface")]
    QuerySurfaceCapabilities(#[source] lavashell_vulkan::Error),
    #[error("cannot list the formats of the Vulkan surface")]
    QuerySurfaceFormats(#[source] lavashell_vulkan::Error),
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
    #[error("the HDR mastering metadata lies beyond the range of a Vulkan float")]
    HdrMetadataRange(#[source] DrawFailure),
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
    #[error(
        "the index of the acquired swapchain image lies beyond the address space of this platform"
    )]
    SwapchainImageIndex(#[source] TryFromIntError),
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
    #[error("the map from dp to output pixels lies beyond the range of Skia scalars")]
    OutputTransform(#[source] DrawFailure),
    #[error("cannot allocate the floating-point working surface")]
    WorkingSurface,
    #[error("an offscreen surface of {0:?} is too large for Skia")]
    OffscreenSize(PixelSize),
    #[error("Skia cannot allocate an offscreen surface of {0:?}")]
    OffscreenSurface(PixelSize),
    #[error("Skia cannot allocate the surface of a PNG export")]
    PngSurface,
    #[error("Skia cannot encode a PNG export")]
    EncodePng,
    #[error("Skia cannot read the pixels of a painted frame")]
    ReadPaintedFrame,
    #[error("Skia did not submit the semaphore that presentation waits for")]
    PresentationSemaphore,
    #[error("Skia cannot submit the frame")]
    SubmitFrame,
    #[error("cannot submit the completion of a frame")]
    SignalFrame(#[source] lavashell_vulkan::Error),
    #[error("cannot present a swapchain image")]
    Present(#[source] lavashell_vulkan::Error),
    #[error("the Vulkan device does not support VK_GOOGLE_display_timing")]
    NoDisplayTiming,
    #[error("the presentation time {0:?} is outside 1..=i64::MAX nanoseconds")]
    PresentationTimeRange(std::time::Duration),
    #[error("the surface requires an explicit timestamp on every frame after its first one")]
    ExplicitTimestampRequired,
    #[error("{}", InOrder(.0))]
    Several(Vec<Error>),
}

impl Error {
    pub(crate) fn fault(&self) -> Fault {
        match self {
            Self::CompileProgram { .. }
            | Self::ProgramUniforms { .. }
            | Self::ProgramChildren { .. }
            | Self::MissingUniform { .. }
            | Self::MissingChild { .. }
            | Self::CompileSubpixelEffect { .. }
            | Self::SubpixelTileSize => Fault::DefectInThisBuild,
            Self::Several(failures) => {
                if failures
                    .iter()
                    .any(|failure| failure.fault() == Fault::DefectInThisBuild)
                {
                    Fault::DefectInThisBuild
                } else {
                    Fault::OutsideThisBuild
                }
            }
            _ => Fault::OutsideThisBuild,
        }
    }
}

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

pub trait MultipleFailures: Sized {
    fn several(failures: Vec<Self>) -> Self;

    fn into_failures(self) -> Vec<Self>;
}

impl MultipleFailures for Error {
    fn several(failures: Vec<Self>) -> Self {
        Self::Several(failures)
    }

    fn into_failures(self) -> Vec<Self> {
        match self {
            Self::Several(failures) => failures,
            failure => vec![failure],
        }
    }
}

pub struct FailureList<E>(Vec<E>);

impl<E> Default for FailureList<E> {
    fn default() -> Self {
        Self(Vec::new())
    }
}

impl<E: MultipleFailures> FailureList<E> {
    pub fn add(&mut self, result: Result<(), E>) {
        if let Err(error) = result {
            self.push(error);
        }
    }

    pub fn push(&mut self, error: E) {
        self.0.extend(error.into_failures());
    }

    pub fn into_result(mut self) -> Result<(), E> {
        if self.0.len() > 1 {
            return Err(E::several(self.0));
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
        let mut none = FailureList::<Error>::default();
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
