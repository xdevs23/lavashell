// SPDX-License-Identifier: GPL-3.0-only
use crate::{
    copy::{CopyKey, copy},
    model::{ClipboardOffer, Demo, Receipt},
    ui,
};
use lavashell::prelude::*;
use lavashell::wayland::Error;
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
    demo.text.0.fields.borrow_mut().clear();
    ui::section(ui, "01", CopyKey::Text, |ui| {
        ui::label(ui, CopyKey::SelectableLabel, 12.sp(), ui::MUTED);
        let id = ui.next_id();
        ui.text(
            copy(CopyKey::SelectableSample),
            Modifier::new()
                .font_family(lavashell_example_support::BODY)
                .font_size(16.sp())
                .color(ui::INK)
                .max_lines(3)
                .user_select(true)
                .tag("selectable_text")
                .accessible_name(copy(CopyKey::SelectableLabel))
                .fill_max_width()
                .background(ui::BG)
                .padding(12.dp()),
        );
        demo.text.remember(id);
        field(ui, demo, false);
        field(ui, demo, true);
        ui::label(ui, CopyKey::SelectionDrag, 13.sp(), ui::MUTED);
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

fn field(ui: &mut Ui, demo: &Demo, multiline: bool) {
    let key = if multiline {
        CopyKey::MultilineLabel
    } else {
        CopyKey::SinglelineLabel
    };
    ui::label(ui, key, 12.sp(), ui::MUTED);
    let state = if multiline {
        &demo.multiline
    } else {
        &demo.single
    };
    let change = state.clone();
    let modifier = Modifier::new()
        .font_family(lavashell_example_support::BODY)
        .font_size(15.sp())
        .color(ui::INK)
        .tag(key.to_string())
        .accessible_name(copy(key))
        .fill_max_width()
        .min_size(0.dp(), if multiline { 82.dp() } else { 45.dp() })
        .border(ui::LINE, 1.dp(), 8.dp())
        .background(ui::CARD)
        .clip(8.dp())
        .padding(12.dp());
    let id = ui.next_id();
    if multiline {
        ui.text_area(
            state.get(),
            TextFieldOptions::default(),
            modifier,
            move |text| change.set(text),
        );
    } else {
        ui.text_input(
            state.get(),
            TextFieldOptions::default(),
            modifier,
            move |text| change.set(text),
        );
    }
    demo.text.remember(id);
}

fn command(ui: &mut Ui, key: CopyKey, enabled: bool, action: impl FnMut() + 'static) {
    ui.button(
        Modifier::new()
            .tag(key.to_string())
            .accessible_name(copy(key))
            .enabled(enabled)
            .weight(1.0)
            .border(ui::LINE, 1.dp(), 9.dp())
            .padding_xy(11.dp(), 11.dp())
            .opacity(if enabled { 1.0 } else { 0.4 }),
        action,
        |ui| ui::label(ui, key, 13.sp(), ui::INK),
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
            let data =
                match TransferData::single("text/plain;charset=utf-8", source.text().as_bytes()) {
                    Ok(data) => data,
                    Err(error) => {
                        demo.offered.set(Some(ClipboardOffer::Failed));
                        report("text clipboard", error);
                        return;
                    }
                };
            clipboard.set_data(data, move |result| {
                demo.offered.set(Some(ClipboardOffer::of(&result)));
                if result.is_ok() && cut {
                    completed.dispatch(window, move |view| {
                        if !view.complete_text_move(&source) {
                            eprintln!("text clipboard: the cut text changed before the move ended and stays");
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
            clipboard.read(
                window,
                ["text/plain;charset=utf-8", "text/plain"],
                move |data| {
                    completed.dispatch(window, move |view| {
                        match data.and_then(|data| {
                            let size = data.bytes.len();
                            let text = String::from_utf8(data.bytes).map_err(Error::DecodeText)?;
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
                },
            );
        });
    }
}
