// SPDX-License-Identifier: GPL-3.0-only
use super::{Check, Samples, TestResult};
use lavashell_core::accessibility::{
    AccessibilityAction, AccessibleNode, AccessibleState, AccessibleText, AccessibleTextRun,
    AccessibleValue, ActionKind, Checked, CollectionInfo, CollectionItem, FormState, LiveRegion,
    RangeInfo, Relations, Role, SemanticsState, TextDirection, TextSelection,
};
use lavashell_core::geometry::{Axis, Rect};
use lavashell_core::ui::NodeId;
use lavashell_core::units::DpExt;
use lavashell_protocol::AccessibilityUpdate;

fn text() -> AccessibleText {
    let run = AccessibleTextRun {
        start: 2,
        text: "Porto".into(),
        line: 1,
        word_starts: Some(vec![0, 3]),
        character_bounds: vec![Rect::new(1.dp(), 2.dp(), 3.dp(), 4.dp())],
        selectable_lengths: vec![5, 6],
        direction: TextDirection::RightToLeft,
    };
    AccessibleText {
        value: "A Porto".into(),
        selection: Some(TextSelection {
            anchor: 1,
            focus: 4,
        }),
        multiline: true,
        placeholder: Some("City".into()),
        language: Some("pt".into()),
        direction: TextDirection::LeftToRight,
        runs: vec![run],
    }
}

fn node() -> AccessibleNode {
    let state = AccessibleState {
        semantics: SemanticsState {
            disabled: true,
            form: FormState {
                read_only: false,
                required: true,
                invalid: false,
            },
            busy: true,
            modal: false,
            checked: Some(Checked::Mixed),
            expanded: Some(true),
        },
        focusable: true,
        selected: Some(false),
    };
    let relations = Relations {
        labelled_by: vec![NodeId(11)],
        described_by: vec![NodeId(12)],
        controls: vec![NodeId(13)],
        error_message: vec![NodeId(14)],
        member_of: Some(NodeId(15)),
    };
    AccessibleNode {
        id: NodeId(4),
        role: Role::Slider,
        heading_level: Some(2),
        name: Some("Lisbon, 18 °C".into()),
        description: Some("Weather".into()),
        value: Some(AccessibleValue::Number(18.5)),
        bounds: Rect::new(16.dp(), 24.dp(), 240.dp(), 164.dp()),
        visible_bounds: Some(Rect::new(16.dp(), 24.dp(), 200.dp(), 100.dp())),
        children: vec![NodeId(6), NodeId(7)],
        state,
        actions: [ActionKind::Activate, ActionKind::Scroll(Axis::Vertical)].into(),
        relations,
        live: LiveRegion::Polite,
        range: Some(RangeInfo {
            min: -10.0,
            max: 40.0,
            step: Some(0.5),
        }),
        collection: Some(CollectionInfo {
            rows: Some(3),
            columns: None,
            item_count: Some(9),
            multiple_selection: true,
        }),
        item: Some(CollectionItem {
            row: 1,
            column: 2,
            row_span: 3,
            column_span: 4,
            heading: true,
        }),
        text: Some(text()),
    }
}

pub(super) fn update() -> AccessibilityUpdate {
    AccessibilityUpdate {
        revision: 2,
        root: NodeId(1),
        focus: Some(NodeId(4)),
        changed: vec![node()],
        removed: vec![NodeId(5)],
    }
}

fn actions() -> Samples<AccessibilityAction> {
    let value = AccessibleValue::Number(0.25);
    every_variant! {
        AccessibilityAction::Focus => AccessibilityAction::Focus, [0];
        AccessibilityAction::Activate => AccessibilityAction::Activate, [1];
        AccessibilityAction::SetValue(_) => AccessibilityAction::SetValue(value), [
            2, 1, 0, 0, 0, 0, 0, 0, 208, 63,
        ];
        AccessibilityAction::Increment => AccessibilityAction::Increment, [3];
        AccessibilityAction::Decrement => AccessibilityAction::Decrement, [4];
        AccessibilityAction::Scroll(..) => AccessibilityAction::Scroll(Axis::Vertical, 40.dp()), [
            5, 1, 0, 0, 0, 0, 0, 0, 68, 64,
        ];
        AccessibilityAction::ScrollIntoView => AccessibilityAction::ScrollIntoView, [6];
        AccessibilityAction::SetTextSelection(_) =>
            AccessibilityAction::SetTextSelection(TextSelection { anchor: 3, focus: 1 }), [7, 3, 1];
        AccessibilityAction::ReplaceSelectedText(_) =>
            AccessibilityAction::ReplaceSelectedText("Faro".into()), [8, 4, 70, 97, 114, 111];
        AccessibilityAction::RevealCollectionItem { .. } =>
            AccessibilityAction::RevealCollectionItem { row: 2, column: 3 }, [9, 2, 3];
    }
}

fn action_kinds() -> Samples<ActionKind> {
    every_variant! {
        ActionKind::Focus => ActionKind::Focus, [0];
        ActionKind::Activate => ActionKind::Activate, [1];
        ActionKind::SetValue => ActionKind::SetValue, [2];
        ActionKind::Increment => ActionKind::Increment, [3];
        ActionKind::Decrement => ActionKind::Decrement, [4];
        ActionKind::Scroll(_) => ActionKind::Scroll(Axis::Horizontal), [5, 0];
        ActionKind::ScrollIntoView => ActionKind::ScrollIntoView, [6];
        ActionKind::SetTextSelection => ActionKind::SetTextSelection, [7];
        ActionKind::ReplaceSelectedText => ActionKind::ReplaceSelectedText, [8];
        ActionKind::RevealCollectionItem => ActionKind::RevealCollectionItem, [9];
    }
}

fn roles() -> Samples<Role> {
    every_variant! {
        Role::Window => Role::Window, [0]; Role::Group => Role::Group, [1];
        Role::Text => Role::Text, [2]; Role::Button => Role::Button, [3];
        Role::Checkbox => Role::Checkbox, [4]; Role::RadioButton => Role::RadioButton, [5];
        Role::Switch => Role::Switch, [6]; Role::Slider => Role::Slider, [7];
        Role::ProgressIndicator => Role::ProgressIndicator, [8];
        Role::TextInput => Role::TextInput, [9]; Role::PasswordInput => Role::PasswordInput, [10];
        Role::TextArea => Role::TextArea, [11]; Role::Image => Role::Image, [12];
        Role::Link => Role::Link, [13]; Role::List => Role::List, [14];
        Role::ListItem => Role::ListItem, [15]; Role::Menu => Role::Menu, [16];
        Role::MenuItem => Role::MenuItem, [17]; Role::Dialog => Role::Dialog, [18];
        Role::Heading => Role::Heading, [19]; Role::Tab => Role::Tab, [20];
        Role::TabList => Role::TabList, [21]; Role::ScrollView => Role::ScrollView, [22];
        Role::ScrollBar => Role::ScrollBar, [23];
        Role::Custom(_) => Role::Custom("map".into()), [24, 3, 109, 97, 112];
    }
}

fn values() -> Samples<AccessibleValue> {
    every_variant! {
        AccessibleValue::Text(_) => AccessibleValue::Text("18 °C".into()),
            [0, 6, 49, 56, 32, 194, 176, 67];
        AccessibleValue::Number(_) => AccessibleValue::Number(0.25), [
            1, 0, 0, 0, 0, 0, 0, 208, 63,
        ];
    }
}

fn checked() -> Samples<Checked> {
    every_variant! {
        Checked::False => Checked::False, [0]; Checked::True => Checked::True, [1];
        Checked::Mixed => Checked::Mixed, [2];
    }
}

fn live_regions() -> Samples<LiveRegion> {
    every_variant! {
        LiveRegion::Off => LiveRegion::Off, [0]; LiveRegion::Polite => LiveRegion::Polite, [1];
        LiveRegion::Assertive => LiveRegion::Assertive, [2];
    }
}

fn text_directions() -> Samples<TextDirection> {
    every_variant! {
        TextDirection::Auto => TextDirection::Auto, [0];
        TextDirection::LeftToRight => TextDirection::LeftToRight, [1];
        TextDirection::RightToLeft => TextDirection::RightToLeft, [2];
    }
}

pub(super) fn check(check: &mut impl Check) -> TestResult {
    check.samples(&actions())?;
    check.samples(&action_kinds())?;
    check.samples(&roles())?;
    check.samples(&values())?;
    check.samples(&checked())?;
    check.samples(&live_regions())?;
    check.samples(&text_directions())
}
