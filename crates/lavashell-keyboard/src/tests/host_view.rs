// SPDX-License-Identifier: GPL-3.0-only
use super::*;

/// A view with only a keyboard tagged `keyboard`, which types into a field that the host edits. The
/// host collects the commands sent by the keyboard, and the transforms passed on by it once
/// `Host::pass_transforms` has set the transform action of the keyboard.
pub(super) struct Host {
    pub(super) view: View,
    pub(super) scene: Scene,
    pub(super) session: State<Option<TextSession>>,
    pub(super) sent: Rc<RefCell<Vec<TextCommand>>>,
    pub(super) transforms: Rc<RefCell<Vec<TransformInput>>>,
    pub(super) settings: State<KeyboardSettings>,
    pub(super) layout: State<&'static Layout>,
    pub(super) placement: State<Placement>,
    pub(super) keyboard: Keyboard,
    pub(super) time: u64,
}

impl Host {
    pub(super) fn new(settings: KeyboardSettings) -> Result<Self, TestError> {
        Self::composed(settings, None, lavashell_test_fixtures::facts())
    }

    /// A host on a platform with `facts`.
    pub(super) fn on(settings: KeyboardSettings, facts: PlatformFacts) -> Result<Self, TestError> {
        Self::composed(settings, None, facts)
    }

    pub(super) fn styled(style: KeyboardStyle) -> Result<Self, TestError> {
        Self::composed(settings(), Some(style), lavashell_test_fixtures::facts())
    }

    fn composed(
        settings: KeyboardSettings,
        style: Option<KeyboardStyle>,
        facts: PlatformFacts,
    ) -> Result<Self, TestError> {
        let (session, sent) = (State::new(None), Rc::new(RefCell::new(Vec::new())));
        let (receiver, transforms) = (sent.clone(), Rc::new(RefCell::new(Vec::new())));
        let target = Target::Host {
            session: session.clone(),
            sink: Rc::new(move |command| receiver.borrow_mut().push(command)),
        };
        let (settings, layout) = (State::new(settings), State::new(&ENGLISH));
        let placement = State::new(Placement::FullWidth);
        let keyboard = Keyboard::new(target, &settings, layout.clone(), placement.clone())?;
        let composed = keyboard.clone();
        let view = View::new(facts, move |ui| {
            let compose = |ui: &mut Ui| {
                above_keyboard(
                    ui,
                    |_| {},
                    |ui| {
                        ui.keyboard(&composed, Modifier::new().tag("keyboard"));
                    },
                );
            };
            match &style {
                Some(style) => ui.provide(&KEYBOARD_STYLE, style.clone(), compose),
                None => compose(ui),
            }
        });
        let mut host = Self {
            view,
            scene: Scene::default(),
            session,
            sent,
            transforms,
            settings,
            layout,
            placement,
            keyboard,
            time: 0,
        };
        host.frame()?;
        Ok(host)
    }

    /// Applies `change` to the platform values of the view, as a host does when the platform
    /// reports new values.
    pub(super) fn platform(&mut self, change: impl FnOnce(&mut PlatformFacts)) {
        let mut facts = self.view.platform();
        change(&mut facts);
        self.view.provide_platform(facts);
    }

    /// Starts a new text session with `hints` and with `text` before the cursor, then waits until
    /// the keyboard is at rest.
    pub(super) fn start(&mut self, hints: TextHints, text: &str) -> Result<(), TestError> {
        let id = (self.session.peek()).map_or(SessionId(1), |session| session.id.next());
        let text = FieldText::BeforeCursor(text.into());
        self.session.set(Some(session(id, hints, text)));
        self.settle()
    }

    /// Sets the text before the cursor of the session, as the host does after an edit.
    pub(super) fn before_cursor(&mut self, text: &str) -> Result<(), TestError> {
        let session = self.session.peek().ok_or(TestError::NoSession)?;
        self.session.set(Some(TextSession {
            text: FieldText::BeforeCursor(text.into()),
            ..session
        }));
        self.frame()
    }

    /// Sets the transform action of the keyboard, which collects each transform in `transforms`.
    pub(super) fn pass_transforms(&self) {
        let reported = self.transforms.clone();
        (self.keyboard).set_on_transform(move |input| reported.borrow_mut().push(input));
    }

    pub(super) fn sent(&self) -> Vec<TextCommand> {
        self.sent.borrow().clone()
    }

    pub(super) fn commit(text: &str) -> TextCommand {
        TextCommand::Commit(text.to_string())
    }
}

impl Clocked for Host {
    fn clock(&mut self) -> (&mut View, &mut Scene, &mut u64) {
        (&mut self.view, &mut self.scene, &mut self.time)
    }
}
