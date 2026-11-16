// SPDX-License-Identifier: GPL-3.0-only
mod root;
mod snapshot;

use crate::board::{Board, Key, KeyAt, Page};
use crate::layouts::{self, Layout};
use crate::placement::Placement;
use crate::platform_typing_settings::PlatformTypingSettings;
use crate::popup::Popup;
use crate::press::Presses;
use crate::settings::KeyboardSettings;
use crate::shift::{self, ShiftChoice};
use crate::stage::{Stage, Stages};
use lavashell_core::prelude::{
    AnimationError, Dp, FieldPresses, Key as KeyCode, Motion, Selector, SessionId, State,
    TextCommand, TextPurpose, TextSender, TextSession, Timer, TransformInput, Ui,
};
use lavashell_core::ui::TransformHandler;
use root::RootNumber;
pub(crate) use snapshot::Snapshot;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

pub type TextSink = Rc<dyn Fn(TextCommand)>;

#[derive(Clone)]
pub enum Target {
    View,
    Host {
        session: State<Option<TextSession>>,
        sink: TextSink,
    },
}

#[derive(Clone)]
pub struct Keyboard(Rc<Inner>);

#[derive(Clone)]
pub(crate) struct ComposingView {
    clock: Motion,
    sender: TextSender,
}

impl PartialEq for ComposingView {
    fn eq(&self, other: &Self) -> bool {
        self.sender == other.sender
    }
}

struct Inner {
    target: Target,
    view: State<Option<ComposingView>>,
    settings: Selector<KeyboardSettings, Rc<KeyboardSettings>>,
    platform: State<Option<Rc<PlatformTypingSettings>>>,
    layout: State<&'static Layout>,
    placement: State<Placement>,
    choices: State<Choices>,
    highlight: State<Highlight>,
    presses: Presses,
    stages: Stages,
    height: State<Dp>,
    roots: Cell<RootNumber>,
    transform: State<Option<TransformHandler>>,
    double_tap: RefCell<Option<Timer>>,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct Choices {
    session: SessionId,
    page: Option<Page>,
    shift: Option<ShiftChoice>,
    hidden: Option<FieldPresses>,
    popup: Option<Popup>,
}

impl Choices {
    fn of(self, session: SessionId) -> Self {
        if self.session == session {
            self
        } else {
            Self {
                session,
                ..Self::default()
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Highlight {
    pub at: KeyAt,
    pub shown: bool,
}

impl Keyboard {
    pub fn new(
        target: Target,
        settings: &State<KeyboardSettings>,
        layout: State<&'static Layout>,
        placement: State<Placement>,
    ) -> Result<Self, AnimationError> {
        Ok(Self(Rc::new(Inner {
            target,
            view: State::new(None),
            settings: settings.select(|settings| Rc::new(settings.clone())),
            platform: State::new(None),
            layout,
            placement,
            choices: State::new(Choices::default()),
            highlight: State::new(Highlight::default()),
            presses: Presses::new(),
            stages: Stages::new()?,
            height: State::new(Dp::ZERO),
            roots: Cell::new(RootNumber::default()),
            transform: State::new(None),
            double_tap: RefCell::new(None),
        })))
    }

    pub fn set_on_transform(&self, action: impl FnMut(TransformInput) + 'static) {
        let action: TransformHandler = Rc::new(RefCell::new(action));
        self.0.transform.set(Some(action));
    }

    pub(crate) fn transform_handler(&self) -> Option<TransformHandler> {
        self.0.transform.get()
    }

    #[must_use]
    pub fn shown(&self) -> bool {
        self.stages().get() != Stage::Hidden
    }

    #[must_use]
    pub fn height(&self) -> Dp {
        self.0.height.get()
    }

    #[must_use]
    pub fn covered(&self) -> Dp {
        self.stages().height(Dp::ZERO)
    }

    pub fn hide(&self) {
        if let Some(session) = (self.0.view.get()).and_then(|view| self.target_session(&view)) {
            self.update_choices(&session, |choices| choices.hidden = Some(session.presses));
        }
    }

    #[must_use]
    pub fn navigate(&self, key: KeyCode) -> bool {
        let Some(snapshot) = self.snapshot() else {
            return false;
        };
        let highlight = snapshot.highlight;
        let Some(at) = snapshot.board.nearest(highlight.at) else {
            return false;
        };
        match key {
            KeyCode::Enter if highlight.shown => {
                if let Some(key) = snapshot.board.key(at) {
                    self.tap(key);
                }
                true
            }
            KeyCode::Left | KeyCode::Right | KeyCode::Up | KeyCode::Down => {
                let moved = highlight
                    .shown
                    .then(|| snapshot.board.step(at, key))
                    .flatten();
                self.0.highlight.set(Highlight {
                    at: moved.unwrap_or(at),
                    shown: true,
                });
                true
            }
            _ => false,
        }
    }

    fn choices(&self, session: &TextSession) -> Choices {
        self.0.choices.get().of(session.id)
    }

    fn update_choices(&self, session: &TextSession, change: impl FnOnce(&mut Choices)) {
        let mut choices = self.0.choices.peek().of(session.id);
        change(&mut choices);
        self.0.choices.set_if_changed(choices);
    }

    pub(crate) fn set_popup(&self, snapshot: &Snapshot, popup: Option<Popup>) {
        self.update_choices(&snapshot.session, |choices| choices.popup = popup);
    }

    pub(crate) fn settings(&self) -> Rc<KeyboardSettings> {
        self.0.settings.get()
    }

    pub(crate) fn presses(&self) -> &Presses {
        &self.0.presses
    }

    pub(crate) fn compose_in(&self, ui: &Ui) -> Result<(), AnimationError> {
        let view = ComposingView {
            clock: ui.motion().clone(),
            sender: ui.text_sender(),
        };
        if self.0.view.peek().as_ref() == Some(&view) {
            return Ok(());
        }
        self.reset_stage(ui.motion())?;
        self.0.view.set(Some(view));
        self.0.choices.set_if_changed(Choices::default());
        self.0.presses.clear();
        self.close_double_tap_window();
        Ok(())
    }

    pub(crate) fn follow_platform(&self, ui: &Ui) {
        (self.0.platform).set_if_changed(Some(Rc::new(PlatformTypingSettings::read(ui))));
    }

    pub(crate) fn stages(&self) -> &Stages {
        &self.0.stages
    }

    fn send(&self, view: &ComposingView, command: TextCommand) {
        match &self.0.target {
            Target::View => view.sender.send(command),
            Target::Host { sink, .. } => sink(command),
        }
    }

    fn target_session(&self, view: &ComposingView) -> Option<TextSession> {
        match &self.0.target {
            Target::View => view.sender.session(),
            Target::Host { session, .. } => session.get(),
        }
    }

    pub(crate) fn timer(
        &self,
        view: &ComposingView,
        mut fire: impl FnMut(&Keyboard) + 'static,
    ) -> Timer {
        let keyboard = Rc::downgrade(&self.0);
        view.clock.timer(move || {
            if let Some(inner) = keyboard.upgrade() {
                fire(&Keyboard(inner));
            }
        })
    }

    fn close_double_tap_window(&self) {
        *self.0.double_tap.borrow_mut() = None;
    }

    pub(crate) fn hide_ring(&self) {
        let highlight = self.0.highlight.peek();
        if highlight.shown {
            self.0.highlight.set(Highlight {
                shown: false,
                ..highlight
            });
        }
    }

    #[must_use]
    pub fn session(&self) -> Option<TextSession> {
        self.shown_session(&self.0.view.get()?)
    }

    fn shown_session(&self, view: &ComposingView) -> Option<TextSession> {
        let session = self.target_session(view)?;
        (self.choices(&session).hidden != Some(session.presses)).then_some(session)
    }

    pub(crate) fn snapshot(&self) -> Option<Snapshot> {
        let view = self.0.view.get()?;
        let session = self.shown_session(&view)?;
        let platform = self.0.platform.get()?;
        let choices = self.choices(&session);
        let settings = self.0.settings.get();
        let layouts = layouts::offered(&settings.layouts, &platform.locales);
        let layout = shown_layout(&layouts, self.0.layout.get());
        let page = choices.page.unwrap_or(match session.hints.purpose {
            TextPurpose::Number | TextPurpose::Phone => Page::Symbols,
            _ => Page::Letters,
        });
        let shift = shift::resolve(
            choices.shift.as_ref(),
            &session,
            platform.preferences.automatic_capitals,
        );
        let board = Board::new(layout, page, &layouts);
        Some(Snapshot {
            view,
            session,
            settings,
            platform,
            layouts,
            layout,
            page,
            shift,
            board,
            highlight: self.0.highlight.get(),
            popup: choices.popup,
            previews: self.0.presses.previews(),
            double_tap: self.0.double_tap.borrow().is_some(),
        })
    }

    pub(crate) fn tap(&self, key: Key) {
        self.type_held();
        self.begin_press();
        self.act(key);
    }

    pub(crate) fn act(&self, key: Key) {
        let Some(snapshot) = self.snapshot() else {
            return;
        };
        let session = &snapshot.session;
        self.close_double_tap_window();
        let (character, before) = (snapshot.character(key), snapshot.typing());
        let after = before.after_release(key, character.as_deref());
        let chosen = before.chosen_shift(key, character.as_deref());
        match key {
            Key::Typed(typed) => {
                self.send(&snapshot.view, TextCommand::Commit(snapshot.typed(typed)));
            }
            Key::Space => self.send(&snapshot.view, TextCommand::Commit(" ".into())),
            Key::Delete => self.send(&snapshot.view, TextCommand::DeleteBackward),
            Key::Action => self.send(
                &snapshot.view,
                match session.hints.action {
                    Some(action) => TextCommand::Action(action),
                    None => TextCommand::Key(KeyCode::Enter),
                },
            ),
            Key::Shift | Key::Page(_) => {}
            Key::Globe => self
                .0
                .layout
                .set(layout_after(&snapshot.layouts, snapshot.layout)),
            Key::Hide => self.hide(),
        }
        self.update_choices(session, |choices| {
            if after.page != before.page {
                choices.page = Some(after.page);
            }
            if let Some(shift) = chosen {
                choices.shift = Some(ShiftChoice::new(shift, session));
            }
        });
        if after.double_tap {
            let window = self.timer(&snapshot.view, Keyboard::close_double_tap_window);
            window.start(snapshot.platform.double_tap);
            *self.0.double_tap.borrow_mut() = Some(window);
        }
    }

    pub(crate) fn switch_layout(&self, layout: &'static Layout) {
        self.0.layout.set(layout);
    }

    pub(crate) fn placement(&self) -> Placement {
        self.0.placement.get()
    }
}

fn shown_layout(layouts: &[&'static Layout], layout: &'static Layout) -> &'static Layout {
    match layouts.first() {
        Some(first) if !layouts.contains(&layout) => first,
        _ => layout,
    }
}

fn layout_after(layouts: &[&'static Layout], layout: &'static Layout) -> &'static Layout {
    let index = layouts.iter().position(|listed| *listed == layout);
    index
        .and_then(|index| layouts.get((index + 1) % layouts.len()).copied())
        .unwrap_or(layout)
}
