// SPDX-License-Identifier: GPL-3.0-only
use super::{Scene, centered};
use crate::board::{CHARACTER, GAP, Key, KeyAt, Page};
use crate::keyboard::Snapshot;
use crate::look::{
    self, ACTION_GAP, ACTION_ICON, ACTION_KEY, FACE_MARGIN, KEY_CORNERS, KEY_ICON, LETTER,
    PAGE_KEY, RING, SPACE_BAR,
};
use crate::shift::Shift;
use lavashell_core::prelude::*;

pub(super) fn compose(ui: &mut Ui, scene: &Scene, at: KeyAt, key: Key) {
    let snapshot = scene.snapshot;
    let (grid, rect) = (scene.grid, scene.grid.rect(&snapshot.board, at));
    let label = snapshot.character(key);
    let (keyboard, typist) = (scene.keyboard.clone(), scene.keyboard.clone());
    let mut modifier = Modifier::new().size(rect.width, rect.height);
    if label.is_some() {
        modifier = modifier.island(KEY_CORNERS);
    }
    let highlight = snapshot.highlight;
    if highlight.shown && snapshot.board.nearest(highlight.at) == Some(at) {
        modifier = modifier.outline(scene.look.accent, RING, KEY_CORNERS, RING);
    }
    let modifier = modifier.padding_xy(FACE_MARGIN, Dp::ZERO);
    // Each side of a key grows by half a gap, so an enlarged key meets its neighbours.
    let enlarged = match ScaleFactor::try_new(1.0 + GAP / scene.grid.width(CHARACTER)) {
        Ok(enlarged) => enlarged,
        Err(error) => {
            ui.fail(error);
            return;
        }
    };
    let modifier = modifier
        .focusable(false)
        .state_styles(styles(scene, key))
        .semantics(semantics(scene, key, label.clone()))
        .on_pointer(move |input| keyboard.pointer(key, at, grid, input))
        .on_accessibility_action(ActionKind::Activate, move |_| {
            typist.tap(key);
            Ok(())
        })
        .transition(
            Transition::new(look::hide_spring())
                .reflow()
                .enter(Presence::default().scale(enlarged)),
        );
    centered(ui, modifier, |ui| face(ui, scene, key, label.as_deref()));
}

/// The icon, the word and the style of the action key. `tonal` is true where typing goes on after
/// the action: in the same field for Enter in a field with several lines, or in the next field for
/// Next.
struct ActionFace<'a> {
    icon: &'static Icon,
    word: &'a str,
    tonal: bool,
}

fn action(snapshot: &Snapshot) -> ActionFace<'_> {
    let (text, icons) = (&snapshot.settings.text, &snapshot.settings.icons);
    let (icon, word, tonal) = match snapshot.session.hints.action {
        Some(TextAction::Search) => (icons.search, &text.search, false),
        Some(TextAction::Done) => (icons.done, &text.done, false),
        Some(TextAction::Next) => (icons.next, &text.next, true),
        Some(TextAction::Go) => (icons.go, &text.go, false),
        Some(TextAction::Send) => (icons.send, &text.send, false),
        None => (icons.enter, &text.enter, snapshot.session.multiline),
    };
    ActionFace { icon, word, tonal }
}

fn styles(scene: &Scene, key: Key) -> StateStyles {
    let look = scene.look;
    match key {
        Key::Char(_) | Key::Currency => look.letter.clone(),
        Key::Shift => match scene.snapshot.shift {
            Shift::Off => look.function.clone(),
            Shift::Once => look.shifted.clone(),
            Shift::Locked => look.caps.clone(),
        },
        Key::Action if action(scene.snapshot).tonal => look.tonal.clone(),
        Key::Action => look.action.clone(),
        _ => look.function.clone(),
    }
}

fn face(ui: &mut Ui, scene: &Scene, key: Key, label: Option<&str>) {
    let (snapshot, look) = (scene.snapshot, scene.look);
    let (text, icons) = (&snapshot.settings.text, &snapshot.settings.icons);
    let outlined = |ui: &mut Ui, icon: &'static Icon| {
        ui.icon(
            icon,
            IconOptions {
                size: Some(KEY_ICON),
                ..IconOptions::default()
            },
            Modifier::new().color(look.ink),
        );
    };
    let filled = |ui: &mut Ui, icon: &'static Icon| {
        ui.icon(
            icon,
            IconOptions {
                size: Some(KEY_ICON),
                style: IconStyle::Filled,
            },
            Modifier::new().color(look.ink),
        );
    };
    match key {
        Key::Char(_) | Key::Currency => {
            let style = scene.text(&LETTER, look.ink);
            ui.text(label.unwrap_or_default(), Modifier::new().text_style(style));
        }
        Key::Shift => match snapshot.shift {
            Shift::Off => outlined(ui, icons.shift),
            Shift::Once => filled(ui, icons.shift),
            Shift::Locked => filled(ui, icons.caps_lock),
        },
        Key::Delete => outlined(ui, icons.delete),
        Key::Globe => outlined(ui, icons.globe),
        Key::Hide => outlined(ui, icons.hide),
        Key::Page(page) => {
            let label = match page {
                Page::Letters => &text.letters_label,
                Page::Symbols => &text.symbols_label,
                Page::MoreSymbols => &text.more_symbols_label,
            };
            ui.text(
                label,
                Modifier::new().text_style(scene.text(&PAGE_KEY, look.ink)),
            );
        }
        Key::Space => {
            let style = scene.text(&SPACE_BAR, look.muted);
            ui.text(snapshot.layout.name(), Modifier::new().text_style(style));
        }
        Key::Action => {
            let face = action(snapshot);
            ui.layout(
                Axis::Horizontal,
                Modifier::new().gap(ACTION_GAP).align_items(Align::Center),
                |ui| {
                    ui.icon(
                        face.icon,
                        IconOptions {
                            size: Some(ACTION_ICON),
                            ..IconOptions::default()
                        },
                        Modifier::new().color(look.ink),
                    );
                    ui.text(
                        face.word,
                        Modifier::new().text_style(scene.text(&ACTION_KEY, look.ink)),
                    );
                },
            );
        }
    }
}

fn semantics(scene: &Scene, key: Key, label: Option<String>) -> Semantics {
    let snapshot = scene.snapshot;
    let text = &snapshot.settings.text;
    let name = match key {
        Key::Char(_) | Key::Currency => label.unwrap_or_default(),
        Key::Shift => text.shift.clone(),
        Key::Delete => text.delete.clone(),
        Key::Globe => text.globe.clone(),
        Key::Hide => text.hide.clone(),
        Key::Space => text.space.clone(),
        Key::Page(Page::Letters) => text.letters.clone(),
        Key::Page(Page::Symbols) => text.symbols.clone(),
        Key::Page(Page::MoreSymbols) => text.more_symbols.clone(),
        Key::Action => action(snapshot).word.to_string(),
    };
    let mut semantics = Semantics {
        role: Some(Role::Button),
        mode: SemanticsMode::MergeDescendants,
        name: Some(name),
        ..Semantics::default()
    };
    match key {
        Key::Shift => {
            semantics.state.checked = Some(match snapshot.shift {
                Shift::Off => Checked::False,
                Shift::Once => Checked::True,
                Shift::Locked => Checked::Mixed,
            });
        }
        Key::Space => {
            semantics.value = Some(AccessibleValue::Text(snapshot.layout.name().to_string()));
            semantics.live = LiveRegion::Polite;
        }
        _ => {}
    }
    semantics
}
