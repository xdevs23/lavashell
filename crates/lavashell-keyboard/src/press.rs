// SPDX-License-Identifier: GPL-3.0-only
use crate::board::{Grid, Key, KeyAt, Typed};
use crate::keyboard::{ComposingView, Keyboard};
use crate::popup::{Entries, Offer, Open, Placed, Popup};
use lavashell_core::platform::KeyRepeat;
use lavashell_core::prelude::{
    Point, PointerId, PointerInput, PointerPhase, SessionId, State, Timer,
};
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::time::Duration;

pub(crate) const LINGER: Duration = Duration::from_millis(80);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct PressSerial(u64);

impl PressSerial {
    fn following(self) -> Self {
        Self(self.0 + 1)
    }
}

struct Press {
    serial: PressSerial,
    pointer: PointerId,
    key: Key,
    session: SessionId,
    view: ComposingView,
    timer: Option<Timer>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Preview {
    pub serial: PressSerial,
    pub label: String,
    pub key: Key,
    pub at: KeyAt,
}

pub(crate) struct Presses {
    held: RefCell<Vec<Press>>,
    previews: State<Vec<Preview>>,
    closing: RefCell<BTreeMap<PressSerial, Timer>>,
    serials: Cell<PressSerial>,
}

impl Presses {
    pub fn new() -> Self {
        Self {
            held: RefCell::default(),
            previews: State::new(Vec::new()),
            closing: RefCell::default(),
            serials: Cell::new(PressSerial::default()),
        }
    }

    pub fn previews(&self) -> Vec<Preview> {
        self.previews.get()
    }

    pub fn clear(&self) {
        self.held.borrow_mut().clear();
        self.previews.set_if_changed(Vec::new());
        self.closing.borrow_mut().clear();
    }

    fn next_serial(&self) -> PressSerial {
        let serial = self.serials.get().following();
        self.serials.set(serial);
        serial
    }
}

impl Keyboard {
    pub(crate) fn pointer(&self, key: Key, at: KeyAt, grid: Grid, input: PointerInput) {
        match input.phase {
            PointerPhase::Pressed => self.press(key, at, input.pointer),
            PointerPhase::Moved => self.slide(input.pointer, at, grid, input.position),
            PointerPhase::Released => self.release(input.pointer),
            PointerPhase::Cancelled => self.cancel(input.pointer),
        }
    }

    pub(crate) fn begin_press(&self) {
        if let Some(snapshot) = self.snapshot()
            && (snapshot.popup.as_ref()).is_some_and(|popup| popup.open == Open::LayoutsLeft)
        {
            self.set_popup(&snapshot, None);
        }
    }

    pub(crate) fn type_held(&self) {
        let held = std::mem::take(&mut *self.presses().held.borrow_mut());
        for press in held {
            self.finish(&press);
        }
    }

    fn press(&self, key: Key, at: KeyAt, pointer: PointerId) {
        self.begin_press();
        self.hide_ring();
        let Some(snapshot) = self.snapshot() else {
            return;
        };
        let serial = self.presses().next_serial();
        let (platform, session) = (&snapshot.platform, snapshot.session.id);
        if key.acts_on_press() {
            self.type_held();
            self.act(key);
        }
        let long_press = |offer| {
            let popup = Popup::new(serial, at, offer);
            let timer = self.timer(&snapshot.view, move |keyboard| {
                keyboard.open(session, popup.clone());
            });
            timer.start(platform.long_press);
            timer
        };
        let timer = match key {
            Key::Delete => {
                (platform.key_repeat).map(|repeat| self.repeat(&snapshot.view, session, repeat))
            }
            Key::Typed(Typed::Char(letter)) if snapshot.listed_letters(letter).next().is_some() => {
                Some(long_press(Offer::FurtherLetters(letter)))
            }
            Key::Globe => Some(long_press(Offer::Layouts)),
            _ => None,
        };
        let before: Vec<Key> = (self.presses().held.borrow().iter())
            .map(|press| press.key)
            .collect();
        if let Some(label) = snapshot.character_after(&before, key) {
            let preview = Preview {
                serial,
                label,
                key,
                at,
            };
            (self.presses().previews).update(|previews| previews.push(preview));
        }
        let mut held = self.presses().held.borrow_mut();
        for press in held.iter_mut() {
            let _stopped_timer = press.timer.take();
        }
        held.push(Press {
            serial,
            pointer,
            key,
            session,
            view: snapshot.view.clone(),
            timer,
        });
    }

    fn repeat(&self, view: &ComposingView, session: SessionId, repeat: KeyRepeat) -> Timer {
        let timer = self.timer(view, move |keyboard| {
            if keyboard.in_session(session) {
                keyboard.act(Key::Delete);
            }
        });
        timer.repeat(repeat.delay(), repeat.interval());
        timer
    }

    fn in_session(&self, session: SessionId) -> bool {
        self.session().is_some_and(|current| current.id == session)
    }

    fn open(&self, session: SessionId, popup: Popup) {
        let Some(snapshot) = self
            .snapshot()
            .filter(|snapshot| snapshot.session.id == session)
        else {
            return;
        };
        (self.presses().previews).update(|previews| {
            previews.retain(|preview| !popup.held_by(preview.serial));
        });
        self.set_popup(&snapshot, Some(popup));
    }

    fn slide(&self, pointer: PointerId, at: KeyAt, grid: Grid, position: Point) {
        let serial = (self.presses().held.borrow().iter())
            .find(|press| press.pointer == pointer)
            .map(|press| press.serial);
        let Some(serial) = serial else {
            return;
        };
        let Some(snapshot) = self.snapshot() else {
            return;
        };
        let Some(popup) = snapshot.popup_held_by(serial).cloned() else {
            return;
        };
        let Some(placed) = Placed::new(&popup, &snapshot, grid) else {
            return;
        };
        let key = grid.rect(&snapshot.board, at);
        let point = Point::new(key.x + position.x, key.y + position.y);
        let chosen = placed.entry_at(point);
        self.set_popup(&snapshot, Some(Popup { chosen, ..popup }));
    }

    fn take_press(&self, pointer: PointerId) -> Option<Press> {
        let mut held = self.presses().held.borrow_mut();
        let index = held.iter().position(|press| press.pointer == pointer)?;
        Some(held.remove(index))
    }

    fn release(&self, pointer: PointerId) {
        let ended: Vec<Press> = {
            let mut held = self.presses().held.borrow_mut();
            let Some(index) = held.iter().position(|press| press.pointer == pointer) else {
                return;
            };
            held.drain(..=index).collect()
        };
        for press in ended {
            self.finish(&press);
        }
    }

    fn cancel(&self, pointer: PointerId) {
        let Some(press) = self.take_press(pointer) else {
            return;
        };
        if let Some(snapshot) = self.snapshot()
            && snapshot.popup_held_by(press.serial).is_some()
        {
            self.set_popup(&snapshot, None);
        }
        self.dismiss(&press);
    }

    fn finish(&self, press: &Press) {
        self.dismiss(press);
        let Some(snapshot) =
            (self.snapshot()).filter(|snapshot| snapshot.session.id == press.session)
        else {
            return;
        };
        let Some(popup) = snapshot.popup_held_by(press.serial).cloned() else {
            if !press.key.acts_on_press() {
                self.act(press.key);
            }
            return;
        };
        match (popup.chosen, popup.offer()) {
            (Some(index), _) => self.choose(&popup, index),
            (None, Offer::Layouts) => {
                let left = Popup {
                    open: Open::LayoutsLeft,
                    ..popup
                };
                self.set_popup(&snapshot, Some(left));
            }
            (None, Offer::FurtherLetters(_)) => self.set_popup(&snapshot, None),
        }
    }

    pub(crate) fn pick(&self, index: usize) {
        let Some(popup) = self.snapshot().and_then(|snapshot| snapshot.popup) else {
            return;
        };
        (self.presses().held.borrow_mut()).retain(|press| !popup.held_by(press.serial));
        self.type_held();
        self.choose(&popup, index);
    }

    fn choose(&self, popup: &Popup, index: usize) {
        let Some(snapshot) = self.snapshot() else {
            return;
        };
        self.set_popup(&snapshot, None);
        match Entries::of(popup, &snapshot) {
            Some(Entries::FurtherLetters(letters)) => {
                if let Some(letter) = letters.get(index) {
                    self.act(Key::char(letter));
                }
            }
            Some(Entries::Layouts(layouts)) => {
                if let Some(layout) = layouts.get(index) {
                    self.switch_layout(layout);
                }
            }
            None => {}
        }
    }

    fn dismiss(&self, press: &Press) {
        let serial = press.serial;
        let presses = self.presses();
        let shown = (presses.previews.peek().iter()).any(|preview| preview.serial == serial);
        if !shown || presses.closing.borrow().contains_key(&serial) {
            return;
        }
        let timer = self.timer(&press.view, move |keyboard| {
            let presses = keyboard.presses();
            (presses.previews)
                .update(|previews| previews.retain(|preview| preview.serial != serial));
            let _expired_timer = presses.closing.borrow_mut().remove(&serial);
        });
        timer.start(LINGER);
        let _replaced_timer = presses.closing.borrow_mut().insert(serial, timer);
    }
}
