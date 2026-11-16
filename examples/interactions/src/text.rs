// SPDX-License-Identifier: GPL-3.0-only
use crate::{
    Error,
    copy::{CopyKey, copy},
    model::{ClipboardOffer, Demo, Receipt},
    palette::PALETTE,
    tag::Tag,
    ui,
};
use lavashell::core::transfer::TextFormat;
use lavashell::prelude::*;
use lavashell_example_support::report;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

#[derive(Clone)]
pub struct TextActions(Rc<Controls>);

struct Controls {
    fields: RefCell<Vec<NodeId>>,
    selected: Cell<Option<NodeId>>,
    copy: State<bool>,
    cut: State<bool>,
    paste: State<bool>,
}

impl Default for TextActions {
    fn default() -> Self {
        Self(Rc::new(Controls {
            fields: RefCell::new(Vec::new()),
            selected: Cell::new(None),
            copy: State::new(false),
            cut: State::new(false),
            paste: State::new(false),
        }))
    }
}

impl TextActions {
    pub fn observe(&self, view: &View) {
        if let Some(id) = view.focus()
            && self.0.fields.borrow().contains(&id)
        {
            self.0.selected.set(Some(id));
        }
        let source = self
            .0
            .selected
            .get()
            .and_then(|id| view.text_selection_source(id));
        self.0.copy.set_if_changed(source.is_some());
        self.0
            .cut
            .set_if_changed(source.is_some_and(|source| source.can_move()));
        self.0.paste.set_if_changed(
            self.0
                .selected
                .get()
                .and_then(|id| view.text_paste_target(id))
                .is_some(),
        );
    }

    fn remember(&self, id: NodeId) {
        self.0.fields.borrow_mut().push(id);
    }
}

pub fn compose(ui: &mut Ui, demo: &Demo, clipboard: &Clipboard, actions: &ViewActions) {
    let colors = ui.local(&PALETTE);
    demo.text.0.fields.borrow_mut().clear();
    ui::section(ui, "01", CopyKey::Text, |ui| {
        ui::label(ui, CopyKey::SelectableLabel, 12.sp(), colors.muted);
        let id = ui.next_id();
        ui.text(
            copy(CopyKey::SelectableSample),
            Modifier::new()
                .font_family(lavashell_example_support::BODY)
                .font_size(16.sp())
                .color(colors.ink)
                .max_lines(3)
                .user_select(true)
                .tag(Tag::SelectableText)
                .accessible_name(copy(CopyKey::SelectableLabel))
                .fill_max_width()
                .background(colors.background)
                .padding(12.dp()),
        );
        demo.text.remember(id);
        field(ui, demo, FieldKind::Singleline);
        field(ui, demo, FieldKind::Multiline);
        ui.row(Modifier::new().gap(8.dp()), |ui| {
            command(
                ui,
                CopyKey::Copy,
                demo.text.0.copy.get(),
                copy_action(demo, clipboard, actions, false),
            );
            command(
                ui,
                CopyKey::Cut,
                demo.text.0.cut.get(),
                copy_action(demo, clipboard, actions, true),
            );
            command(
                ui,
                CopyKey::Paste,
                demo.text.0.paste.get(),
                paste_action(demo, clipboard, actions),
            );
            let reset = demo.clone();
            ui::button(ui, CopyKey::Reset, Modifier::new().weight(1.0), move || {
                reset.reset_text();
            });
        });
    });
}

#[derive(Clone, Copy)]
enum FieldKind {
    Singleline,
    Multiline,
}

fn field(ui: &mut Ui, demo: &Demo, kind: FieldKind) {
    let colors = ui.local(&PALETTE);
    let (key, state, min_height) = match kind {
        FieldKind::Singleline => (CopyKey::SinglelineLabel, &demo.single, 45.dp()),
        FieldKind::Multiline => (CopyKey::MultilineLabel, &demo.multiline, 82.dp()),
    };
    ui::label(ui, key, 12.sp(), colors.muted);
    let change = state.clone();
    let modifier = Modifier::new()
        .font_family(lavashell_example_support::BODY)
        .font_size(15.sp())
        .color(colors.ink)
        .tag(Tag::Control(key))
        .accessible_name(copy(key))
        .fill_max_width()
        .min_size(0.dp(), min_height)
        .border(colors.line, 1.dp(), 8.dp())
        .background(colors.card)
        .clip(8.dp())
        .padding(12.dp());
    let id = ui.next_id();
    match kind {
        FieldKind::Singleline => ui.text_input(
            state.get(),
            TextFieldOptions::default(),
            modifier,
            move |text| change.set(text),
        ),
        FieldKind::Multiline => ui.text_area(
            state.get(),
            TextFieldOptions::default(),
            modifier,
            move |text| change.set(text),
        ),
    }
    demo.text.remember(id);
}

fn command(ui: &mut Ui, key: CopyKey, enabled: bool, action: impl FnMut() + 'static) {
    let colors = ui.local(&PALETTE);
    ui.button(
        Modifier::new()
            .tag(Tag::Control(key))
            .accessible_name(copy(key))
            .enabled(enabled)
            .weight(1.0)
            .border(colors.line, 1.dp(), 9.dp())
            .padding_xy(11.dp(), 11.dp())
            .opacity(if enabled { 1.0 } else { 0.4 }),
        action,
        |ui| ui::label(ui, key, 13.sp(), colors.ink),
    );
}

fn copy_action(
    demo: &Demo,
    clipboard: &Clipboard,
    actions: &ViewActions,
    cut: bool,
) -> impl FnMut() + 'static {
    let (demo, clipboard, actions) = (demo.clone(), clipboard.clone(), actions.clone());
    move || {
        let Some(window) = demo.window() else {
            return;
        };
        let (demo, clipboard, completed) = (demo.clone(), clipboard.clone(), actions.clone());
        actions.dispatch(window, move |view| {
            let Some(source) = demo
                .text
                .0
                .selected
                .get()
                .and_then(|id| view.text_selection_source(id))
            else {
                return;
            };
            clipboard.set_data(TransferData::text(source.text()), move |result| {
                demo.offered.set(Some(ClipboardOffer::of(&result)));
                if result.is_ok() && cut {
                    completed.dispatch(window, move |view| {
                        if !view.complete_text_move(&source) {
                            report("text clipboard", Error::CutTextChanged);
                        }
                    });
                }
                if let Err(error) = result {
                    report("text clipboard", error);
                }
            });
        });
    }
}

fn paste_action(
    demo: &Demo,
    clipboard: &Clipboard,
    actions: &ViewActions,
) -> impl FnMut() + 'static {
    let (demo, clipboard, actions) = (demo.clone(), clipboard.clone(), actions.clone());
    move || {
        let Some(window) = demo.window() else {
            return;
        };
        let (demo, clipboard, completed) = (demo.clone(), clipboard.clone(), actions.clone());
        actions.dispatch(window, move |view| {
            let Some(target) = demo
                .text
                .0
                .selected
                .get()
                .and_then(|id| view.text_paste_target(id))
            else {
                return;
            };
            let formats = TextFormat::by_preference().map(TextFormat::mime_type);
            clipboard.read(window, formats, move |data| {
                completed.dispatch(window, move |view| {
                    match data.and_then(|data| {
                        let size = data.bytes.len();
                        let text =
                            String::from_utf8(data.bytes).map_err(ClipboardError::DecodeText)?;
                        Ok((data.mime_type, size, text))
                    }) {
                        Ok((mime, bytes, text)) => {
                            demo.receipt.set(if view.paste_text_at(&target, &text) {
                                Receipt::Accepted {
                                    mime,
                                    bytes,
                                    preview: None,
                                }
                            } else {
                                Receipt::Rejected
                            });
                        }
                        Err(error) => {
                            demo.receipt.set(Receipt::Rejected);
                            report("text paste", error);
                        }
                    }
                });
            });
        });
    }
}
