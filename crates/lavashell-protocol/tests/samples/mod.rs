// SPDX-License-Identifier: GPL-3.0-only
//! Stored bytes of every message of version 1 and of each variant of the enums inside them. postcard writes a value
//! inside a message as the bytes that it has alone. A change that alters any of them needs a new version. The models
//! of a color are a private enum of core, which stores their bytes.

/// One sample of each variant of an enum and its stored bytes. The patterns form an exhaustive match, so an enum with
/// a variant that has no sample fails to compile, and each sample has to match its pattern. A variant that is valid
/// only in the process that made it has no sample.
macro_rules! every_variant {
    (@samples [$($every:pat),+] $($pattern:pat => $sample:expr, $stored:expr;)+) => {{
        let samples: Vec<(_, &'static [u8])> = vec![$({
            let sample = $sample;
            let pattern = stringify!($pattern);
            assert!(matches!(sample, $pattern), "{} is not {pattern}", stringify!($sample));
            (sample, &$stored[..])
        }),+];
        for (sample, _) in &samples {
            match sample {
                $($every)|+ => {}
            }
        }
        samples
    }};
    (#[process_local($($local:pat),+)] $($pattern:pat => $sample:expr, $stored:expr;)+) => {
        every_variant!(@samples [$($pattern),+, $($local),+] $($pattern => $sample, $stored;)+)
    };
    ($($pattern:pat => $sample:expr, $stored:expr;)+) => {
        every_variant!(@samples [$($pattern),+] $($pattern => $sample, $stored;)+)
    };
}

mod accessibility;
mod color;
mod effects;
mod input;
mod motion;
mod scene;
mod shape;
mod tree;

use lavashell_core::accessibility::{AccessibilityAction, ActionRequest};
use lavashell_core::animation::{EasingError, KeyframesError, MotionError, SpringError};
use lavashell_core::effects::{EffectId, SourceFilterId};
use lavashell_core::geometry::Point;
use lavashell_core::input_method::{
    Capitalization, FieldPresses, FieldText, SessionId, TextAction, TextCommand, TextHints,
    TextPurpose, TextSession,
};
use lavashell_core::preferences::{ContrastPreference, PreferenceValues, TextScaling};
use lavashell_core::scene::tree::SceneUpdate;
use lavashell_core::shape::PointsError;
use lavashell_core::ui::NodeId;
use lavashell_core::units::{Density, DpExt, NonNegativeDp, NumberError};
use lavashell_core::view::{Cursor, InputEvent, Proposal};
use lavashell_protocol::{
    Account, FrameSequence, GuestMessage, Hello, HostMessage, NonNegativeSize, ProposalSequence,
    ProtocolError, ResizeSequence, VERSION, Versions, decode,
};
use serde::{Serialize, de::DeserializeOwned};
use std::num::TryFromIntError;
use std::time::Duration;

/// Values, each with the bytes stored for it.
pub type Samples<T> = Vec<(T, &'static [u8])>;

/// The reason a test of the samples stops: an error from the code that makes or reads a sample, or a value that a
/// sample needs and did not get.
#[derive(Debug, thiserror::Error)]
pub enum TestError {
    #[error("a message could not be encoded or decoded")]
    Protocol(#[from] ProtocolError),
    #[error("a number could not be checked")]
    Number(#[from] NumberError),
    #[error("a spring could not be made")]
    Spring(#[from] SpringError),
    #[error("an easing curve could not be made")]
    Easing(#[from] EasingError),
    #[error("keyframes could not be made")]
    Keyframes(#[from] KeyframesError),
    #[error("a motion could not be made")]
    Motion(#[from] MotionError),
    #[error("a shape could not be made from its points")]
    Points(#[from] PointsError),
    #[error("a count does not fit the type it is converted to")]
    Integer(#[from] TryFromIntError),
    #[error("the sample needs {0}, which it does not get")]
    Absent(&'static str),
}

pub type TestResult = Result<(), TestError>;

/// What a test checks of the samples of one type and their stored bytes.
pub trait Check {
    /// # Errors
    ///
    /// The error of a sample that fails to encode or decode.
    fn samples<T: Serialize + DeserializeOwned>(&mut self, samples: &[(T, &[u8])]) -> TestResult;
}

/// Hands the samples of each type to `check`.
///
/// # Errors
///
/// The error of a sample that cannot be made, or that `check` returns.
pub fn check_every_sample(check: &mut impl Check) -> TestResult {
    check.samples(&[hello()?])?;
    check.samples(&host_messages()?)?;
    check.samples(&guest_messages()?)?;
    accessibility::check(check)?;
    color::check(check)?;
    effects::check(check)?;
    input::check(check)?;
    motion::check(check)?;
    scene::check(check)?;
    shape::check(check)?;
    tree::check(check)
}

/// Reads a value that has no public constructor from its bytes.
///
/// # Errors
///
/// The error of a decode of `bytes` as a `T`.
pub fn read<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, ProtocolError> {
    Ok(decode(bytes, &mut Account::new(usize::MAX))?.value)
}

/// # Errors
///
/// `TestError::Absent` when the versions of the hello are out of order.
pub fn hello() -> Result<(Hello, &'static [u8]), TestError> {
    let versions = Versions::new(VERSION, VERSION).ok_or(TestError::Absent("ordered versions"))?;
    Ok((Hello { versions }, &[1, 1]))
}

fn size(width: i32, height: i32) -> Result<NonNegativeSize, NumberError> {
    Ok(NonNegativeSize {
        width: NonNegativeDp::try_new(width.dp())?,
        height: NonNegativeDp::try_new(height.dp())?,
    })
}

fn host_messages() -> Result<Samples<HostMessage>, TestError> {
    let preferences = PreferenceValues {
        text_scaling: TextScaling::linear(1.25)
            .ok_or(TestError::Absent("a text scaling of 1.25"))?,
        reduced_motion: true,
        reduced_transparency: false,
        contrast: ContrastPreference::More,
    };
    let proposal = Proposal {
        width: Some(320.dp()),
        height: None,
    };
    let request = ActionRequest {
        target: NodeId(7),
        action: AccessibilityAction::Activate,
    };
    let density = Density::from_scale(2.75).ok_or(TestError::Absent("a density of 2.75"))?;
    Ok(every_variant! {
        HostMessage::Setup { .. } => HostMessage::Setup {
            effects: vec![EffectId(0x6261_636b_6472_6f70)],
            filters: vec![SourceFilterId(7)],
        }, [0, 1, 240, 222, 201, 163, 182, 237, 216, 176, 98, 1, 7];
        HostMessage::Preferences(_) => HostMessage::Preferences(preferences),
            [1, 0, 0, 0, 0, 0, 0, 0, 244, 63, 1, 0, 1];
        HostMessage::Resize { .. } => HostMessage::Resize {
            sequence: ResizeSequence(3),
            size: size(320, 180)?,
            density,
        }, [
            2, 3, 0, 0, 0, 0, 0, 0, 116, 64, 0, 0, 0, 0, 0, 128, 102, 64, 0, 0, 0, 0, 0, 0, 6, 64,
        ];
        HostMessage::Propose { .. } => HostMessage::Propose {
            sequence: ProposalSequence(4),
            proposal,
        }, [3, 4, 1, 0, 0, 0, 0, 0, 0, 116, 64, 0];
        HostMessage::Frame { .. } => HostMessage::Frame {
            sequence: FrameSequence(9),
            presentation: Duration::from_nanos(1_000_016_667),
            deadline: Duration::from_micros(1_000_012),
        }, [4, 9, 1, 155, 130, 1, 1, 224, 93];
        HostMessage::Input { .. } => HostMessage::Input {
            shown: FrameSequence(8),
            input: InputEvent::Press(Point::new(12.dp(), 40.5.dp())),
        }, [5, 8, 7, 0, 0, 0, 0, 0, 0, 40, 64, 0, 0, 0, 0, 0, 64, 68, 64];
        HostMessage::Text(_) => HostMessage::Text(TextCommand::Commit("ok".into())),
            [6, 0, 2, 111, 107];
        HostMessage::Copy => HostMessage::Copy, [7]; HostMessage::Cut => HostMessage::Cut, [8];
        HostMessage::Paste(_) => HostMessage::Paste("Lisbon".into()),
            [9, 6, 76, 105, 115, 98, 111, 110];
        HostMessage::Accessibility(_) => HostMessage::Accessibility(request), [10, 7, 1];
        HostMessage::AccessibilityActive(_) => HostMessage::AccessibilityActive(true), [11, 1];
        HostMessage::Shown => HostMessage::Shown, [12];
        HostMessage::Hidden => HostMessage::Hidden, [13];
        HostMessage::Throttled => HostMessage::Throttled, [14];
    })
}

fn session() -> TextSession {
    TextSession {
        id: SessionId(2),
        presses: FieldPresses(1),
        hints: TextHints {
            action: Some(TextAction::Go),
            purpose: TextPurpose::Email,
            capitalization: Capitalization::Words,
        },
        multiline: true,
        text: FieldText::BeforeCursor("Lis".into()),
    }
}

fn guest_messages() -> Result<Samples<GuestMessage>, TestError> {
    let update = SceneUpdate {
        changes: tree::changes()
            .into_iter()
            .map(|(change, _)| change)
            .collect(),
    };
    Ok(every_variant! {
        GuestMessage::RequestFrame => GuestMessage::RequestFrame, [0];
        GuestMessage::Scene { .. } => GuestMessage::Scene {
            frame: FrameSequence(9),
            layout: ResizeSequence(3),
            update,
        }, [
            1, 9, 3, 5, 0, 1, 1, 2, 3, 0, 0, 0, 0, 0, 0, 0, 0, 248, 63, 0, 0, 0, 0, 0, 0, 208, 63,
            0, 0, 0, 0, 0, 0, 224, 191, 0, 0, 0, 0, 0, 0, 0, 64, 0, 0, 0, 0, 0, 0, 8, 64, 0, 0, 0,
            0, 0, 0, 28, 64, 0, 0, 0, 0, 0, 0, 0, 232, 63, 1, 0, 0, 0, 0, 0, 0, 0, 48, 64, 0, 0, 0,
            0, 0, 0, 56, 64, 0, 0, 0, 0, 0, 0, 110, 64, 0, 0, 0, 0, 0, 128, 100, 64, 0, 0, 0, 0, 0,
            0, 0, 64, 0, 0, 0, 0, 0, 0, 16, 64, 0, 0, 0, 0, 0, 0, 24, 64, 0, 0, 0, 0, 0, 0, 32, 64,
            1, 4, 2, 5, 0, 6, 3, 7, 0, 3, 0, 10, 0, 0, 0, 0, 0, 0, 0, 48, 64, 0, 0, 0, 0, 0, 0, 56,
            64, 0, 0, 0, 0, 0, 0, 110, 64, 0, 0, 0, 0, 0, 128, 100, 64, 0, 0, 0, 0, 0, 0, 0, 64, 0,
            0, 0, 0, 0, 0, 16, 64, 0, 0, 0, 0, 0, 0, 24, 64, 0, 0, 0, 0, 0, 0, 32, 64, 0, 0, 0, 0,
            0, 0, 0, 0, 240, 63, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 236, 81, 184, 30,
            133, 235, 193, 63, 1, 4, 8, 1, 0, 0, 0, 0, 0, 0, 0, 224, 63,
        ];
        GuestMessage::Cursor(_) => GuestMessage::Cursor(Cursor::Pointer), [2, 1];
        GuestMessage::FocusVisibleInside(_) => GuestMessage::FocusVisibleInside(true), [3, 1];
        GuestMessage::Session(_) => GuestMessage::Session(Some(session())),
            [4, 1, 2, 1, 1, 3, 3, 2, 1, 2, 3, 76, 105, 115];
        GuestMessage::Copied(_) => GuestMessage::Copied("18 °C".into()),
            [5, 6, 49, 56, 32, 194, 176, 67];
        GuestMessage::Accessibility(_) => GuestMessage::Accessibility(accessibility::update()), [
            6, 2, 1, 1, 4, 1, 4, 7, 1, 2, 1, 14, 76, 105, 115, 98, 111, 110, 44, 32, 49, 56, 32,
            194, 176, 67, 1, 7, 87, 101, 97, 116, 104, 101, 114, 1, 1, 0, 0, 0, 0, 0, 128, 50, 64,
            0, 0, 0, 0, 0, 0, 48, 64, 0, 0, 0, 0, 0, 0, 56, 64, 0, 0, 0, 0, 0, 0, 110, 64, 0, 0, 0,
            0, 0, 128, 100, 64, 1, 0, 0, 0, 0, 0, 0, 48, 64, 0, 0, 0, 0, 0, 0, 56, 64, 0, 0, 0, 0,
            0, 0, 105, 64, 0, 0, 0, 0, 0, 0, 89, 64, 2, 6, 7, 1, 0, 1, 0, 1, 0, 1, 2, 1, 1, 1, 1, 0,
            2, 1, 5, 1, 1, 11, 1, 12, 1, 13, 1, 14, 1, 15, 1, 1, 0, 0, 0, 0, 0, 0, 36, 192, 0, 0, 0,
            0, 0, 0, 68, 64, 1, 0, 0, 0, 0, 0, 0, 224, 63, 1, 1, 3, 0, 1, 9, 1, 1, 1, 2, 3, 4, 1, 1,
            7, 65, 32, 80, 111, 114, 116, 111, 1, 1, 4, 1, 1, 4, 67, 105, 116, 121, 1, 2, 112, 116,
            1, 1, 2, 5, 80, 111, 114, 116, 111, 1, 1, 2, 0, 3, 1, 0, 0, 0, 0, 0, 0, 240, 63, 0, 0,
            0, 0, 0, 0, 0, 64, 0, 0, 0, 0, 0, 0, 8, 64, 0, 0, 0, 0, 0, 0, 16, 64, 2, 5, 6, 2, 1, 5,
        ];
        GuestMessage::Size { .. } => GuestMessage::Size {
            sequence: ProposalSequence(4),
            size: size(320, 96)?,
        }, [7, 4, 0, 0, 0, 0, 0, 0, 116, 64, 0, 0, 0, 0, 0, 0, 88, 64];
    })
}
