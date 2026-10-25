// SPDX-License-Identifier: GPL-3.0-only
use crate::board::{Grid, Key, KeyAt};
use crate::keyboard::{ComposingView, Keyboard};
use crate::popup::{Entries, Offer, Open, Placed, Popup};
use lavashell_core::platform::KeyRepeat;
use lavashell_core::prelude::{
    Point, PointerId, PointerInput, PointerPhase, SessionId, State, Timer,
};
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::time::Duration;

/// How long the preview of a released key is still fully open before it starts to close.
const LINGER: Duration = Duration::from_millis(80);

/// A key held down by a pointer, together with the text session in which it was pressed. A press
/// in a text session that has ended types nothing.
struct Press {
    serial: u64,
    pointer: PointerId,
    key: Key,
    session: SessionId,
    /// The view in which the key was pressed. The timer that closes the preview of the key runs on
    /// the clock of that view.
    view: ComposingView,
    /// The timer that opens a popup after a long press or repeats a held delete key. Dropping the
    /// press cancels it.
    _timer: Option<Timer>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Preview {
    pub serial: u64,
    pub label: String,
    pub key: Key,
    pub at: KeyAt,
    pub released: bool,
}

pub(crate) struct Presses {
    held: RefCell<Vec<Press>>,
    previews: State<Vec<Preview>>,
    /// The timers that close the previews of released keys, by the serial number of the press.
    closing: RefCell<BTreeMap<u64, Timer>>,
    serials: Cell<u64>,
}

impl Presses {
    pub fn new() -> Self {
        Self {
            held: RefCell::default(),
            previews: State::new(Vec::new()),
            closing: RefCell::default(),
            serials: Cell::new(0),
        }
    }

    /// The open key previews. Reading them during composition makes the composition run again when
    /// the list changes.
    pub fn previews(&self) -> Vec<Preview> {
        self.previews.get()
    }

    pub fn clear(&self) {
        self.held.borrow_mut().clear();
        self.previews.set_if_changed(Vec::new());
        self.closing.borrow_mut().clear();
    }

    fn next_serial(&self) -> u64 {
        let serial = self.serials.get() + 1;
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

    /// Types the keys held so far and closes a list of layouts that is still open after its press
    /// ended. Every press outside a popup starts with this.
    pub(crate) fn begin_press(&self) {
        self.type_held();
        if let Some(snapshot) = self.snapshot()
            && (snapshot.popup.as_ref()).is_some_and(|popup| popup.open == Open::LayoutsLeft)
        {
            self.set_popup(&snapshot, None);
        }
    }

    /// Finishes every held press as a release would, so that a key pressed before another key types
    /// first.
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
        let (facts, session) = (&snapshot.facts, snapshot.session.id);
        if key.acts_on_press() {
            self.act(key);
        }
        let long_press = |offer| {
            let popup = Popup::new(serial, at, offer);
            let timer = self.timer(&snapshot.view, move |keyboard| {
                keyboard.open(session, popup.clone());
            });
            timer.start(facts.long_press);
            timer
        };
        let timer = match key {
            Key::Delete => {
                (facts.key_repeat).map(|repeat| self.repeat(&snapshot.view, session, repeat))
            }
            Key::Char(letter) if snapshot.listed_letters(letter).next().is_some() => {
                Some(long_press(Offer::FurtherLetters(letter)))
            }
            Key::Globe => Some(long_press(Offer::Layouts)),
            _ => None,
        };
        if let Some(label) = snapshot.character(key) {
            let preview = Preview {
                serial,
                label,
                key,
                at,
                released: false,
            };
            (self.presses().previews).update(|previews| previews.push(preview));
        }
        self.presses().held.borrow_mut().push(Press {
            serial,
            pointer,
            key,
            session,
            view: snapshot.view.clone(),
            _timer: timer,
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

    /// Replaces the preview of the press that opened `popup` with the popup, if the text session
    /// `session` is still the current one.
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

    /// Chooses the entry under the pointer of the press that opened the popup. The pointer is at
    /// `position` on the key at `at`.
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
        if let Some(press) = self.take_press(pointer) {
            self.finish(&press);
        }
    }

    /// Ends the press of `pointer` without typing and closes the popup opened by that press.
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

    /// Releases `press`. When the press opened a popup, the chosen letter is typed or the chosen
    /// layout is shown, and a list of layouts with nothing chosen is left open. Otherwise the key
    /// does its action, unless it already acted when it was pressed. A press in a text session that
    /// has ended does nothing.
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

    /// Chooses the entry at `index` of the open popup, as a tap on that entry does. The press that
    /// opened the popup ends without typing.
    pub(crate) fn pick(&self, index: usize) {
        let Some(popup) = self.snapshot().and_then(|snapshot| snapshot.popup) else {
            return;
        };
        (self.presses().held.borrow_mut()).retain(|press| !popup.held_by(press.serial));
        self.type_held();
        self.choose(&popup, index);
    }

    /// Closes `popup` and types the letter or switches to the layout at `index`. An index beyond
    /// the entries shown now chooses nothing.
    fn choose(&self, popup: &Popup, index: usize) {
        let Some(snapshot) = self.snapshot() else {
            return;
        };
        self.set_popup(&snapshot, None);
        match Entries::of(popup, &snapshot) {
            Some(Entries::FurtherLetters(letters)) => {
                if let Some(letter) = letters.get(index) {
                    self.act(Key::Char(*letter));
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

    /// Marks the preview of `press` as released and closes it after `LINGER`.
    fn dismiss(&self, press: &Press) {
        let serial = press.serial;
        let presses = self.presses();
        let open = (presses.previews.peek().iter())
            .any(|preview| preview.serial == serial && !preview.released);
        if !open {
            return;
        }
        presses.previews.update(|previews| {
            for preview in (previews.iter_mut()).filter(|preview| preview.serial == serial) {
                preview.released = true;
            }
        });
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
