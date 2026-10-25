// SPDX-License-Identifier: GPL-3.0-only
use crate::board::{Board, Key, KeyAt, Page};
use crate::facts::Facts;
use crate::layouts::{self, Layout};
use crate::popup::Popup;
use crate::press::{Presses, Preview};
use crate::settings::KeyboardSettings;
use crate::shift::{self, Shift, ShiftChoice};
use crate::stage::{Stage, Stages};
use lavashell_core::prelude::{
    AnimationError, Dp, FieldPresses, Key as KeyCode, Motion, Selector, SessionId, State,
    TextCommand, TextPurpose, TextSender, TextSession, Timer, Ui,
};
use std::cell::RefCell;
use std::rc::Rc;

/// A function called with each text command that the keyboard sends to a field edited by the host.
pub type TextSink = Rc<dyn Fn(TextCommand)>;

#[derive(Clone)]
pub enum Target {
    /// The focused field of the view where the keyboard is composed.
    View,
    /// A field edited by the host, such as one in another view or another process. The host
    /// updates `session` after every change and applies every command passed to `sink`.
    Host {
        session: State<Option<TextSession>>,
        sink: TextSink,
    },
}

/// An on-screen keyboard. The keyboard is on the screen while it is composed in a view and the
/// target given to it has a field that accepts text. Clones share one keyboard. Compose it with
/// `UiKeyboard::keyboard`.
#[derive(Clone)]
pub struct Keyboard(Rc<Inner>);

#[derive(Clone)]
pub(crate) struct ComposingView {
    clock: Motion,
    sender: TextSender,
}

/// The clock and the sender of a `ComposingView` come from the same view. Comparing the senders is
/// therefore enough, since two senders are equal when they send to the same view.
impl PartialEq for ComposingView {
    fn eq(&self, other: &Self) -> bool {
        self.sender == other.sender
    }
}

struct Inner {
    target: Target,
    view: State<Option<ComposingView>>,
    settings: Selector<KeyboardSettings, Rc<KeyboardSettings>>,
    /// The platform facts read during the last composition, or `None` before the first composition.
    facts: State<Option<Rc<Facts>>>,
    layout: State<&'static Layout>,
    choices: State<Choices>,
    highlight: State<Highlight>,
    presses: Presses,
    stages: Stages,
    /// Runs while a second tap on shift counts as a double tap.
    double_tap: RefCell<Option<Timer>>,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct Choices {
    session: SessionId,
    page: Option<Page>,
    shift: Option<ShiftChoice>,
    /// The number of presses on the field at the time the keyboard was hidden. The keyboard is
    /// shown again once the field has a different number of presses.
    hidden: Option<FieldPresses>,
    popup: Option<Popup>,
}

impl Choices {
    /// These choices when they were made in `session`, otherwise empty choices for `session`.
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

/// The key to which the arrow keys moved the focus ring, and whether the focus ring is shown on
/// that key.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Highlight {
    pub at: KeyAt,
    pub shown: bool,
}

/// The state of a shown keyboard, read all at once so that one composition or one input uses
/// consistent values.
pub(crate) struct Snapshot {
    pub view: ComposingView,
    pub session: TextSession,
    pub settings: Rc<KeyboardSettings>,
    pub facts: Rc<Facts>,
    /// The layouts that the globe key switches between, in order.
    pub layouts: Vec<&'static Layout>,
    pub layout: &'static Layout,
    pub page: Page,
    pub shift: Shift,
    pub board: Board,
    pub highlight: Highlight,
    pub popup: Option<Popup>,
    pub previews: Vec<Preview>,
}

impl Snapshot {
    /// `letter` as a capital while shift is on, otherwise `letter` unchanged. Shift applies only on
    /// the page of letters, and only to a letter whose capital is one character. A letter such as
    /// ß, whose capital is SS, does not change.
    pub fn cased(&self, letter: char) -> char {
        if self.page != Page::Letters || self.shift == Shift::Off {
            return letter;
        }
        let mut capital = letter.to_uppercase();
        match (capital.next(), capital.next()) {
            (Some(capital), None) => capital,
            _ => letter,
        }
    }

    pub fn character(&self, key: Key) -> Option<String> {
        match key {
            Key::Char(letter) => Some(self.cased(letter).to_string()),
            Key::Currency => Some(self.layout.currency().to_string()),
            _ => None,
        }
    }

    pub fn popup_held_by(&self, serial: u64) -> Option<&Popup> {
        (self.popup.as_ref()).filter(|popup| popup.held_by(serial))
    }

    /// The further letters listed for `letter` by the shown layout and then by each configured
    /// layout, without the letters that have a key on the shown layout. The list is empty outside
    /// the page of letters. A letter listed by several layouts comes once for each of them.
    pub fn listed_letters(&self, letter: char) -> impl Iterator<Item = char> + '_ {
        let layouts = std::iter::once(self.layout).chain(self.layouts.iter().copied());
        layouts
            .filter(|_| self.page == Page::Letters)
            .filter_map(move |layout| layout.further_letters_of(letter))
            .flat_map(str::chars)
            .filter(|listed| !self.layout.has_key(*listed))
    }

    /// The letters offered by a long press on `letter`: the listed letters, each once.
    pub fn further_letters(&self, letter: char) -> Vec<char> {
        let mut letters = Vec::new();
        for listed in self.listed_letters(letter) {
            if !letters.contains(&listed) {
                letters.push(listed);
            }
        }
        letters
    }
}

impl Keyboard {
    /// A keyboard that types into `target`, uses `settings` and shows `layout`. The globe key
    /// changes `layout`. The timers of the keyboard run on the motion clock of the view in which
    /// the keyboard is composed.
    ///
    /// # Errors
    ///
    /// Fails when the animation of the keyboard panel cannot be created.
    pub fn new(
        target: Target,
        settings: &State<KeyboardSettings>,
        layout: State<&'static Layout>,
    ) -> Result<Self, AnimationError> {
        Ok(Self(Rc::new(Inner {
            target,
            view: State::new(None),
            settings: settings.select(|settings| Rc::new(settings.clone())),
            facts: State::new(None),
            layout,
            choices: State::new(Choices::default()),
            highlight: State::new(Highlight::default()),
            presses: Presses::new(),
            stages: Stages::new()?,
            double_tap: RefCell::new(None),
        })))
    }

    /// Whether the keyboard panel is on the screen. This is true from the first composition of the
    /// keyboard that finds a field accepting text, until the panel has finished leaving after no
    /// field accepts text any more or the keyboard was hidden. Reading this value during
    /// composition makes the composition run again when the value changes.
    #[must_use]
    pub fn shown(&self) -> bool {
        self.stages().get() != Stage::Hidden
    }

    /// The height of the keyboard at rest at the bottom of the view: the height of the panel set
    /// in the settings while the keyboard is shown, or zero while it is hidden.
    #[must_use]
    pub fn height(&self) -> Dp {
        if self.shown() {
            self.settings().panel_height.get()
        } else {
            Dp::ZERO
        }
    }

    /// Hides the keyboard until another field gains focus or the field is pressed again.
    pub fn hide(&self) {
        if let Some(session) = (self.0.view.get()).and_then(|view| self.target_session(&view)) {
            self.update_choices(&session, |choices| choices.hidden = Some(session.presses));
        }
    }

    /// Moves the focus ring over the keys for an arrow key, and types the key in the focus ring for
    /// Enter. The first arrow key only shows the ring on the key where it was last. Returns whether
    /// the keyboard used `key`, which happens only while the keyboard is shown.
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

    /// The choices made in `session`. Reading them during composition makes the composition run
    /// again when they change.
    fn choices(&self, session: &TextSession) -> Choices {
        self.0.choices.get().of(session.id)
    }

    fn update_choices(&self, session: &TextSession, change: impl FnOnce(&mut Choices)) {
        let mut choices = self.0.choices.peek().of(session.id);
        change(&mut choices);
        self.0.choices.set_if_changed(choices);
    }

    /// Opens `popup` in the session of `snapshot`, or closes the open popup when `popup` is `None`.
    pub(crate) fn set_popup(&self, snapshot: &Snapshot, popup: Option<Popup>) {
        self.update_choices(&snapshot.session, |choices| choices.popup = popup);
    }

    /// Reading the settings during composition makes the composition run again when they change.
    pub(crate) fn settings(&self) -> Rc<KeyboardSettings> {
        self.0.settings.get()
    }

    pub(crate) fn presses(&self) -> &Presses {
        &self.0.presses
    }

    /// Makes the view composed by `ui` the view that runs the timers of the keyboard and that the
    /// text commands of the keyboard go to. When that view differs from the previous one, the
    /// keyboard clears its choices, held keys and previews. Fails when the animation of the panel
    /// cannot be created on the clock of that view.
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

    /// Reads the platform facts used by the keyboard from the view composed by `ui`. Presses and
    /// taps after this composition use those facts. Reading them makes the composition run again
    /// when a fact changes.
    pub(crate) fn follow_platform(&self, ui: &Ui) {
        self.0.facts.set_if_changed(Some(Rc::new(Facts::read(ui))));
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

    /// The text session of the field that the keyboard types into: the focused field of `view` for
    /// `Target::View`, or the session set by the host for `Target::Host`.
    fn target_session(&self, view: &ComposingView) -> Option<TextSession> {
        match &self.0.target {
            Target::View => view.sender.session(),
            Target::Host { session, .. } => session.get(),
        }
    }

    /// A timer on the clock of `view` that runs `fire` while the keyboard exists.
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

    /// Ends the window in which a second tap on shift counts as a double tap.
    fn close_double_tap_window(&self) {
        *self.0.double_tap.borrow_mut() = None;
    }

    /// Hides the focus ring, as a touch on a key does.
    pub(crate) fn hide_ring(&self) {
        let highlight = self.0.highlight.peek();
        if highlight.shown {
            self.0.highlight.set(Highlight {
                shown: false,
                ..highlight
            });
        }
    }

    pub(crate) fn session(&self) -> Option<TextSession> {
        self.shown_session(&self.0.view.get()?)
    }

    fn shown_session(&self, view: &ComposingView) -> Option<TextSession> {
        let session = self.target_session(view)?;
        (self.choices(&session).hidden != Some(session.presses)).then_some(session)
    }

    pub(crate) fn snapshot(&self) -> Option<Snapshot> {
        let view = self.0.view.get()?;
        let session = self.shown_session(&view)?;
        let facts = self.0.facts.get()?;
        let choices = self.choices(&session);
        let settings = self.0.settings.get();
        let layouts = layouts::offered(&settings.layouts, &facts.locales);
        let layout = shown_layout(&layouts, self.0.layout.get());
        let page = choices.page.unwrap_or(match session.hints.purpose {
            TextPurpose::Number | TextPurpose::Phone => Page::Symbols,
            _ => Page::Letters,
        });
        let shift = shift::resolve(
            choices.shift.as_ref(),
            &session,
            facts.preferences.automatic_capitals,
        );
        let board = Board::new(layout, page, &layouts);
        Some(Snapshot {
            view,
            session,
            settings,
            facts,
            layouts,
            layout,
            page,
            shift,
            board,
            highlight: self.0.highlight.get(),
            popup: choices.popup,
            previews: self.0.presses.previews(),
        })
    }

    /// Types the keys held so far, then does what a release of `key` does.
    pub(crate) fn tap(&self, key: Key) {
        self.begin_press();
        self.act(key);
    }

    /// Does what releasing `key` does.
    pub(crate) fn act(&self, key: Key) {
        let Some(snapshot) = self.snapshot() else {
            return;
        };
        let session = &snapshot.session;
        let double = self.0.double_tap.take().is_some();
        match key {
            Key::Char(_) | Key::Currency => {
                if let Some(text) = snapshot.character(key) {
                    self.type_text(&snapshot, text);
                }
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
            Key::Shift => {
                let shift = shift::tapped(snapshot.shift, double);
                let choice = ShiftChoice::new(shift, session);
                self.update_choices(session, |choices| choices.shift = Some(choice));
                if shift != Shift::Locked {
                    let window = self.timer(&snapshot.view, Keyboard::close_double_tap_window);
                    window.start(snapshot.facts.double_tap);
                    *self.0.double_tap.borrow_mut() = Some(window);
                }
            }
            Key::Page(page) => self.update_choices(session, |choices| choices.page = Some(page)),
            Key::Globe => {
                let after = layout_after(&snapshot.layouts, snapshot.layout);
                self.0.layout.set(after);
            }
            Key::Hide => self.hide(),
        }
    }

    pub(crate) fn switch_layout(&self, layout: &'static Layout) {
        self.0.layout.set(layout);
    }

    /// Commits `text`. On the page of letters, typing a letter ends shift for one letter.
    fn type_text(&self, snapshot: &Snapshot, text: String) {
        let letter = text.chars().any(char::is_alphabetic);
        self.send(&snapshot.view, TextCommand::Commit(text));
        if snapshot.page == Page::Letters && snapshot.shift != Shift::Locked && letter {
            let typed = ShiftChoice::new(Shift::Off, &snapshot.session);
            self.update_choices(&snapshot.session, |choices| choices.shift = Some(typed));
        }
    }
}

/// The layout to show: `layout`, or the first of `layouts` when `layouts` does not contain
/// `layout`.
fn shown_layout(layouts: &[&'static Layout], layout: &'static Layout) -> &'static Layout {
    match layouts.first() {
        Some(first) if !layouts.iter().any(|listed| listed.id() == layout.id()) => first,
        _ => layout,
    }
}

fn layout_after(layouts: &[&'static Layout], layout: &'static Layout) -> &'static Layout {
    let index = layouts.iter().position(|listed| listed.id() == layout.id());
    index
        .and_then(|index| layouts.get((index + 1) % layouts.len()).copied())
        .unwrap_or(layout)
}
