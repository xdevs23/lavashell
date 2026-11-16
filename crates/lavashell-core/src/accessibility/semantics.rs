// SPDX-License-Identifier: GPL-3.0-only
use crate::geometry::Rect;
use crate::ui::NodeId;
use crate::units::NumberError;
use std::fmt;
use std::ops::RangeInclusive;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Role {
    Window,
    #[default]
    Group,
    Text,
    Button,
    Checkbox,
    RadioButton,
    Switch,
    Slider,
    ProgressIndicator,
    TextInput,
    PasswordInput,
    TextArea,
    Image,
    Link,
    List,
    ListItem,
    Menu,
    MenuItem,
    Dialog,
    Heading,
    Tab,
    TabList,
    ScrollView,
    ScrollBar,
    Custom(String),
}

impl fmt::Display for Role {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Window => "window",
            Self::Group => "group",
            Self::Text => "text",
            Self::Button => "button",
            Self::Checkbox => "checkbox",
            Self::RadioButton => "radio button",
            Self::Switch => "switch",
            Self::Slider => "slider",
            Self::ProgressIndicator => "progress indicator",
            Self::TextInput => "text input",
            Self::PasswordInput => "password input",
            Self::TextArea => "text area",
            Self::Image => "image",
            Self::Link => "link",
            Self::List => "list",
            Self::ListItem => "list item",
            Self::Menu => "menu",
            Self::MenuItem => "menu item",
            Self::Dialog => "dialog",
            Self::Heading => "heading",
            Self::Tab => "tab",
            Self::TabList => "tab list",
            Self::ScrollView => "scroll view",
            Self::ScrollBar => "scroll bar",
            Self::Custom(name) => name,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SemanticsMode {
    #[default]
    Normal,
    MergeDescendants,
    ClearDescendants,
    Decorative,
    Hidden,
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum AccessibleValue {
    Text(String),
    Number(f64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Checked {
    False,
    True,
    Mixed,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FormState {
    pub read_only: bool,
    pub required: bool,
    pub invalid: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SemanticsState {
    pub disabled: bool,
    pub form: FormState,
    pub busy: bool,
    pub modal: bool,
    pub checked: Option<Checked>,
    pub expanded: Option<bool>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AccessibleState {
    pub semantics: SemanticsState,
    pub focusable: bool,
    pub selected: Option<bool>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum LiveRegion {
    #[default]
    Off,
    Polite,
    Assertive,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TextDirection {
    #[default]
    Auto,
    LeftToRight,
    RightToLeft,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Relations {
    #[cfg_attr(
        feature = "serde",
        serde(deserialize_with = "crate::encoding::sequence")
    )]
    pub labelled_by: Vec<NodeId>,
    #[cfg_attr(
        feature = "serde",
        serde(deserialize_with = "crate::encoding::sequence")
    )]
    pub described_by: Vec<NodeId>,
    #[cfg_attr(
        feature = "serde",
        serde(deserialize_with = "crate::encoding::sequence")
    )]
    pub controls: Vec<NodeId>,
    #[cfg_attr(
        feature = "serde",
        serde(deserialize_with = "crate::encoding::sequence")
    )]
    pub error_message: Vec<NodeId>,
    pub member_of: Option<NodeId>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RangeInfo {
    pub min: f64,
    pub max: f64,
    pub step: Option<f64>,
}

impl RangeInfo {
    #[must_use]
    pub fn valid(&self) -> bool {
        self.min.is_finite()
            && self.max.is_finite()
            && self.min <= self.max
            && self.step.is_none_or(|step| step.is_finite() && step > 0.0)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CollectionInfo {
    pub rows: Option<usize>,
    pub columns: Option<usize>,
    pub item_count: Option<usize>,
    pub multiple_selection: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CollectionItem {
    pub row: usize,
    pub column: usize,
    pub row_span: usize,
    pub column_span: usize,
    pub heading: bool,
}

impl Default for CollectionItem {
    fn default() -> Self {
        Self {
            row: 0,
            column: 0,
            row_span: 1,
            column_span: 1,
            heading: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TextSelection {
    pub anchor: usize,
    pub focus: usize,
}

#[derive(Clone, Debug, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AccessibleTextRun {
    pub start: usize,
    pub text: String,
    pub line: usize,
    #[cfg_attr(
        feature = "serde",
        serde(deserialize_with = "crate::encoding::optional_sequence")
    )]
    pub word_starts: Option<Vec<usize>>,
    #[cfg_attr(
        feature = "serde",
        serde(deserialize_with = "crate::encoding::sequence")
    )]
    pub character_bounds: Vec<Rect>,
    #[cfg_attr(
        feature = "serde",
        serde(deserialize_with = "crate::encoding::sequence")
    )]
    pub selectable_lengths: Vec<usize>,
    pub direction: TextDirection,
}

#[derive(Clone, Debug, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AccessibleText {
    pub value: String,
    pub selection: Option<TextSelection>,
    pub multiline: bool,
    pub placeholder: Option<String>,
    pub language: Option<String>,
    pub direction: TextDirection,
    #[cfg_attr(
        feature = "serde",
        serde(deserialize_with = "crate::encoding::sequence")
    )]
    pub runs: Vec<AccessibleTextRun>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(try_from = "u8", into = "u8")
)]
pub struct HeadingLevel(u8);

impl HeadingLevel {
    // https://html.spec.whatwg.org/multipage/sections.html#the-h1,-h2,-h3,-h4,-h5,-and-h6-elements
    const LEVELS: RangeInclusive<u8> = 1..=6;

    pub const fn try_new(level: u8) -> Result<Self, NumberError> {
        if level >= *Self::LEVELS.start() && level <= *Self::LEVELS.end() {
            Ok(Self(level))
        } else {
            Err(NumberError::OutOfRange)
        }
    }

    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

#[cfg(feature = "serde")]
impl TryFrom<u8> for HeadingLevel {
    type Error = NumberError;

    fn try_from(level: u8) -> Result<Self, NumberError> {
        Self::try_new(level)
    }
}

#[cfg(feature = "serde")]
impl From<HeadingLevel> for u8 {
    fn from(level: HeadingLevel) -> Self {
        level.0
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Semantics {
    pub mode: SemanticsMode,
    pub role: Option<Role>,
    pub heading_level: Option<HeadingLevel>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub value: Option<AccessibleValue>,
    pub state: SemanticsState,
    pub relations: Relations,
    pub live: LiveRegion,
    pub range: Option<RangeInfo>,
    pub collection: Option<CollectionInfo>,
    pub item: Option<CollectionItem>,
    pub text: Option<AccessibleText>,
}
