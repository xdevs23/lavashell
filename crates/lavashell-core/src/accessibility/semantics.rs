// SPDX-License-Identifier: GPL-3.0-only
use crate::geometry::Rect;
use crate::ui::NodeId;
use std::fmt;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
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
    /// A role with a custom name, understood only by some platform adapters. The other adapters export the node
    /// as `Group`.
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
    /// Names this node from the visible text of its descendants, and leaves the descendants that are not
    /// interactive out of the tree.
    MergeDescendants,
    /// Leaves every descendant out of the tree, also the descendants that offer actions.
    ClearDescendants,
    /// Leaves this node out of the tree and exports its descendants in its place.
    Decorative,
    /// Leaves this node and every descendant out of the tree.
    Hidden,
}

#[derive(Clone, Debug, PartialEq)]
pub enum AccessibleValue {
    Text(String),
    Number(f64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Checked {
    False,
    True,
    Mixed,
}

/// Whether the value of a form field is read-only, required or invalid.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FormState {
    pub read_only: bool,
    pub required: bool,
    pub invalid: bool,
}

/// The states that a component or an application sets in `Semantics`.
/// The view derives the remaining states of `AccessibleState`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SemanticsState {
    pub disabled: bool,
    pub form: FormState,
    pub busy: bool,
    pub modal: bool,
    pub checked: Option<Checked>,
    pub expanded: Option<bool>,
}

/// The state of a node in the accessibility tree. The view computes `focusable` and `selected` from the node. It
/// also adds to the states set in `Semantics`: `disabled` for a disabled node and for every node below a disabled
/// ancestor, `modal` for a modal node, and `read_only` for selectable text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AccessibleState {
    pub semantics: SemanticsState,
    pub focusable: bool,
    pub selected: Option<bool>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LiveRegion {
    #[default]
    Off,
    Polite,
    Assertive,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextDirection {
    #[default]
    Auto,
    LeftToRight,
    RightToLeft,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Relations {
    pub labelled_by: Vec<NodeId>,
    pub described_by: Vec<NodeId>,
    pub controls: Vec<NodeId>,
    pub error_message: Vec<NodeId>,
    pub member_of: Option<NodeId>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
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
pub struct CollectionInfo {
    pub rows: Option<usize>,
    pub columns: Option<usize>,
    pub item_count: Option<usize>,
    pub multiple_selection: bool,
}

/// The zero-based row and column of an item in the data of a collection, with its spans and whether it is a heading.
/// The position does not depend on the holder that a recycled list binds the item to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

/// A text selection in Unicode scalar offsets, from `anchor`, where the selection started, to `focus`, the end that
/// moves. `focus` can be before `anchor`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextSelection {
    pub anchor: usize,
    pub focus: usize,
}

/// A run of shaped text with its line, word starts and character bounds, filled in by the view.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AccessibleTextRun {
    pub start: usize,
    pub text: String,
    /// The zero-based index of the visual line of the run. Runs of different text directions on one line have the
    /// same index.
    pub line: usize,
    /// The offsets of the word starts in the run, in Unicode scalar values from the start of the run. `None` means
    /// that the word starts are unknown. An empty list means that the run has no word start, as in a run that begins
    /// in the middle of a word.
    pub word_starts: Option<Vec<usize>>,
    /// The bounds of each Unicode scalar value of the run, relative to the surface.
    pub character_bounds: Vec<Rect>,
    /// The length in Unicode scalar values of each unit that the keyboard caret moves over in the shaped text.
    /// Platform adapters convert each length to the text encoding of their platform.
    pub selectable_lengths: Vec<usize>,
    pub direction: TextDirection,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AccessibleText {
    pub value: String,
    pub selection: Option<TextSelection>,
    pub multiline: bool,
    pub placeholder: Option<String>,
    pub language: Option<String>,
    pub direction: TextDirection,
    pub runs: Vec<AccessibleTextRun>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Semantics {
    pub mode: SemanticsMode,
    pub role: Option<Role>,
    pub heading_level: Option<u8>,
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
