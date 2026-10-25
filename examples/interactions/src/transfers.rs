// SPDX-License-Identifier: GPL-3.0-only
use crate::{
    Error,
    copy::{CopyKey, copy},
    model::{ClipboardOffer, Demo, Receipt},
    samples::{self, Source},
    ui,
};
use lavashell::prelude::*;
use lavashell::renderer::encoded_image;
use lavashell::wayland;
use lavashell_example_support::report;
use std::{cell::RefCell, collections::HashSet, rc::Rc};

/// The buttons from which a drag of a sample starts: the copy key that labels and tags each button, and the sample
/// dragged from it.
const SOURCES: [(CopyKey, Source); 3] = [
    (CopyKey::CopyImage, Source::Image),
    (CopyKey::CopyDocument, Source::Document),
    (CopyKey::CopyFileUri, Source::FileUri),
];

pub fn install(app: App, demo: &Demo) -> App {
    let dropped = Rc::new(RefCell::new(HashSet::new()));
    let pending = dropped.clone();
    let destination = demo.clone();
    let receiver = demo.clone();
    let source = demo.clone();
    app.on_drag(move |event, view| match event {
        DragEvent::Enter(info) | DragEvent::Motion(info) => {
            let over = view
                .bounds_of("transfer_target")
                .is_some_and(|r| r.contains(info.position));
            destination.drag_over.set_if_changed(over);
            over.then(|| accept(&info)).flatten()
        }
        DragEvent::Drop(info) => {
            if view
                .bounds_of("transfer_target")
                .is_some_and(|r| r.contains(info.position))
            {
                let _first_drop_of_offer = pending.borrow_mut().insert(info.id);
            }
            destination.drag_over.set_if_changed(false);
            None
        }
        DragEvent::Leave { .. } => {
            destination.drag_over.set_if_changed(false);
            None
        }
        DragEvent::Cancelled { id, .. } => {
            let _was_dropped_on_target = pending.borrow_mut().remove(&id);
            destination.drag_over.set_if_changed(false);
            None
        }
    })
    .on_drop(move |event, _| {
        if !dropped.borrow_mut().remove(&event.id) {
            return false;
        }
        let receipt = receipt(event.data);
        let accepted = matches!(receipt, Receipt::Accepted { .. });
        receiver.receipt.set(receipt);
        accepted
    })
    .on_drag_source(move |gesture, view| {
        let (button, kind) = SOURCES.into_iter().find_map(|(tag, kind)| {
            (view.bounds_of(&tag.to_string()))
                .filter(|button| button.contains(gesture.origin))
                .map(|button| (button, kind))
        })?;
        if button.contains(gesture.position) {
            return None;
        }
        match source.samples.offer(kind) {
            Ok(data) => Some(DragRequest::new(
                data,
                DragActions::new([DragAction::Copy]),
                |result, _| {
                    if let Err(error) = result {
                        report("sample drag", error);
                    }
                },
            )),
            Err(error) => {
                report("sample drag offer", error);
                None
            }
        }
    })
}

fn accept(info: &DragOfferInfo) -> Option<DropAcceptance> {
    if !info.actions.contains(DragAction::Copy) {
        return None;
    }
    let mime_type = samples::FORMATS
        .into_iter()
        .find(|mime| info.mime_types.iter().any(|available| available == mime))?
        .into();
    Some(DropAcceptance {
        mime_type,
        action: DragAction::Copy,
    })
}

fn receipt(result: Result<ReceivedData, wayland::Error>) -> Receipt {
    match result {
        Ok(data) => match preview(&data) {
            Ok(preview) => Receipt::Accepted {
                mime: data.mime_type,
                bytes: data.bytes.len(),
                preview,
            },
            Err(error) => {
                report("transfer receive", error);
                Receipt::Rejected
            }
        },
        Err(error) => {
            report("transfer receive", error);
            Receipt::Rejected
        }
    }
}

/// The preview of a received PNG image, or `None` for data in another format.
fn preview(data: &ReceivedData) -> Result<Option<Image>, Error> {
    if data.mime_type != "image/png" {
        return Ok(None);
    }
    encoded_image(data.bytes.as_slice())
        .map(Some)
        .map_err(Error::ReadImage)
}

pub fn compose(ui: &mut Ui, demo: &Demo, clipboard: &Clipboard) {
    ui::section(ui, "03", CopyKey::Transfers, |ui| {
        ui::label(ui, CopyKey::SourceHint, 13.sp(), ui::MUTED);
        ui.row(
            Modifier::new().gap(16.dp()).align_items(Align::Center),
            |ui| {
                let image = demo.samples.image.clone();
                ui.canvas(
                    Size::new(160.dp(), 90.dp()),
                    Modifier::new().clip(10.dp()),
                    move |d| {
                        d.image(
                            &image,
                            Rect::new(0.dp(), 0.dp(), 160.dp(), 90.dp()),
                            ImageFit::Stretch,
                        );
                    },
                );
                ui.column(Modifier::new().gap(7.dp()), |ui| {
                    ui::data(ui, "image/png", 12.sp(), ui::INK);
                    ui::data(ui, format!("{} B", samples::PNG.len()), 11.sp(), ui::MUTED);
                    ui::data(ui, "application/pdf", 12.sp(), ui::INK);
                    ui::data(ui, format!("{} B", samples::PDF.len()), 11.sp(), ui::MUTED);
                });
            },
        );
        ui.row(Modifier::new().gap(8.dp()), |ui| {
            offer_button(ui, demo, clipboard, CopyKey::CopyImage, Source::Image);
            offer_button(ui, demo, clipboard, CopyKey::CopyDocument, Source::Document);
            offer_button(ui, demo, clipboard, CopyKey::CopyFileUri, Source::FileUri);
        });

        target(ui, demo);
        ui.row(Modifier::new().gap(8.dp()), |ui| {
            let clear = demo.receipt.clone();
            let (demo, clipboard) = (demo.clone(), clipboard.clone());
            ui::button(ui, CopyKey::ReceivePreferred, Modifier::new(), move || {
                let Some(window) = demo.window() else {
                    return;
                };
                let demo = demo.clone();
                clipboard.read(window, samples::FORMATS, move |result| {
                    demo.receipt.set(receipt(result));
                });
            });
            ui::button(ui, CopyKey::Clear, Modifier::new(), move || {
                clear.set(Receipt::Empty);
            });
        });
        if let Some(offer) = demo.offered.get() {
            ui::label(ui, offer.status(), 12.sp(), ui::MUTED);
        }
    });
}

fn offer_button(ui: &mut Ui, demo: &Demo, clipboard: &Clipboard, key: CopyKey, source: Source) {
    let (demo, clipboard) = (demo.clone(), clipboard.clone());
    ui::button(ui, key, Modifier::new().weight(1.0), move || {
        match demo.samples.offer(source) {
            Ok(data) => {
                let status = demo.offered.clone();
                clipboard.set_data(data, move |result| {
                    status.set(Some(ClipboardOffer::of(&result)));
                    if let Err(error) = result {
                        report("clipboard offer", error);
                    }
                });
            }
            Err(error) => {
                demo.offered.set(Some(ClipboardOffer::Failed));
                report("clipboard sample", error);
            }
        }
    });
}

fn target(ui: &mut Ui, demo: &Demo) {
    ui.column(
        Modifier::new()
            .tag("transfer_target")
            .semantics_with(|s| {
                s.name = Some(copy(CopyKey::TargetHint).into());
                s.live = LiveRegion::Polite;
            })
            .gap(9.dp())
            .fill_max_width()
            .background(ui::BG)
            .border(
                if demo.drag_over.get() {
                    ui::ACCENT
                } else {
                    ui::LINE
                },
                1.dp(),
                10.dp(),
            )
            .padding(14.dp()),
        |ui| {
            ui::label(ui, CopyKey::TargetHint, 13.sp(), ui::MUTED);
            match demo.receipt.get() {
                Receipt::Empty => ui::label(ui, CopyKey::TransferEmpty, 13.sp(), ui::MUTED),
                Receipt::Rejected => ui::label(ui, CopyKey::TransferRejected, 13.sp(), ui::ACCENT),
                Receipt::Accepted {
                    mime,
                    bytes,
                    preview,
                } => {
                    ui::label(ui, CopyKey::TransferSuccess, 13.sp(), ui::INK);
                    ui::data(ui, format!("{mime} / {bytes} B"), 12.sp(), ui::ACCENT);
                    if let Some(image) = preview
                        && let Some(natural) = natural_size(&image)
                    {
                        let scale = (240.dp() / natural.width).min(100.dp() / natural.height);
                        let size = Size::new(natural.width * scale, natural.height * scale);
                        ui.canvas(size, Modifier::new().clip(8.dp()), move |d| {
                            d.image(
                                &image,
                                Rect::new(0.dp(), 0.dp(), d.size().width, d.size().height),
                                ImageFit::Stretch,
                            );
                        });
                    }
                }
            }
        },
    );
}

/// The size of `image` with one dp for each of its pixels.
fn natural_size(image: &Image) -> Option<Size> {
    let (width, height) = image.dimensions();
    Some(Size::new(
        i32::try_from(width).ok()?.dp(),
        i32::try_from(height).ok()?.dp(),
    ))
}
