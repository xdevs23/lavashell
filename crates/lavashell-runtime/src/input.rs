// SPDX-License-Identifier: GPL-3.0-only
use crate::SurfaceRecords;
use crate::window_move::begin_move;
use lavashell_core::geometry::Point;
use lavashell_core::keyboard::KeyStroke;
use lavashell_core::surface::SurfaceId;
use lavashell_core::ui::InputModifiers;
use lavashell_core::view::InputEvent;
use lavashell_platform::{
    Button, DeviceEvent, InputRequest, InputSerial, KeyboardDevice, PlatformRequest, PointerDevice,
    PressState,
};
use std::collections::BTreeMap;
use touch::TouchPoints;

mod touch;

/// Receives each keyboard input for a surface once the key is resolved, and returns the input to pass on, or `None` to
/// consume it.
pub type InputFilter = Box<dyn FnMut(SurfaceId, InputEvent) -> Option<InputEvent>>;

/// What the router passes on to the view of a surface.
#[derive(Debug, PartialEq)]
pub enum RoutedInput {
    Input {
        surface: SurfaceId,
        input: InputEvent,
        /// The pointer that gets the cursor shown by the view after the input.
        pointer: Option<PointerDevice>,
    },
    /// A press of the primary button of `pointer` at `at`. In the drag region of `surface`, the press starts a move
    /// of the window by the host, and the view receives a cancel. Elsewhere, the view receives the press.
    PrimaryPress {
        surface: SurfaceId,
        pointer: PointerDevice,
        at: Point,
        serial: InputSerial,
    },
    /// The interaction of `pointer` with `surface` ends, because another pointer took the surface over or `pointer`
    /// was removed. The view receives a cancel.
    Cancel {
        surface: SurfaceId,
        pointer: PointerDevice,
    },
}

impl RoutedInput {
    /// Sends the input to the view of the routed surface while that surface is open. Returns the request that sets the
    /// cursor of the pointer to the cursor shown by the view, or the request that starts a move of the window.
    pub fn deliver(self, records: &mut SurfaceRecords) -> Option<PlatformRequest> {
        match self {
            Self::Input {
                surface,
                input,
                pointer,
            } => deliver_input(records, surface, input, pointer),
            Self::PrimaryPress {
                surface,
                pointer,
                at,
                serial,
            } => match begin_move(records, surface, pointer, at, serial) {
                Some(request) => {
                    cancel_pointer(records, surface);
                    Some(PlatformRequest::WindowControl(request))
                }
                None => deliver_input(records, surface, InputEvent::Press(at), Some(pointer)),
            },
            Self::Cancel { surface, .. } => {
                cancel_pointer(records, surface);
                None
            }
        }
    }
}

/// Sends `input` to the view of `surface` while that surface is open. Returns the request that sets the cursor of
/// `pointer` to the cursor shown by the view.
fn deliver_input(
    records: &mut SurfaceRecords,
    surface: SurfaceId,
    input: InputEvent,
    pointer: Option<PointerDevice>,
) -> Option<PlatformRequest> {
    let view = records.open_view_mut(surface)?;
    view.input(input);
    pointer
        .map(|pointer| InputRequest::SetCursor(pointer, view.cursor()))
        .map(PlatformRequest::Input)
}

fn cancel_pointer(records: &mut SurfaceRecords, surface: SurfaceId) {
    if let Some(view) = records.open_view_mut(surface) {
        view.input(InputEvent::CancelPointer);
    }
}

/// What an input does to the interaction of a pointer with the surface under it.
#[derive(Clone, Copy)]
enum Interaction {
    /// The pointer takes the surface over and becomes the one that interacts with it.
    TakesOver,
    /// The surface receives the input only from the pointer that interacts with it.
    OnlyInteracting,
}

#[derive(Default)]
struct KeyboardState {
    focus: Option<SurfaceId>,
    modifiers: InputModifiers,
}

/// Routes the input of each pointer to the view under it, the input of each keyboard to the view focused by it, and the
/// input of each touch point to the view on which it went down.
pub struct InputRouter {
    /// The surface under each pointer.
    pointers: BTreeMap<PointerDevice, SurfaceId>,
    /// The pointer that interacts with each surface: the last pointer whose input the surface received.
    interactions: BTreeMap<SurfaceId, PointerDevice>,
    keyboards: BTreeMap<KeyboardDevice, KeyboardState>,
    touches: TouchPoints,
    filter: Option<InputFilter>,
}

impl InputRouter {
    #[must_use]
    pub fn new(filter: Option<InputFilter>) -> Self {
        Self {
            pointers: BTreeMap::new(),
            interactions: BTreeMap::new(),
            keyboards: BTreeMap::new(),
            touches: TouchPoints::default(),
            filter,
        }
    }

    /// Routes `event` reported by a platform and returns each resulting input for a view, in order.
    pub fn route(&mut self, event: DeviceEvent) -> Vec<RoutedInput> {
        match event {
            DeviceEvent::PointerEnter {
                pointer,
                surface,
                at,
            } => {
                self.pointers.extend([(pointer, surface)]);
                self.route_pointer_input(
                    pointer,
                    Interaction::TakesOver,
                    Some(InputEvent::Move(at)),
                )
            }
            DeviceEvent::PointerMotion { pointer, at } => self.route_pointer_input(
                pointer,
                Interaction::TakesOver,
                Some(InputEvent::Move(at)),
            ),
            DeviceEvent::PointerLeave { pointer } => {
                let left = self.route_pointer_input(
                    pointer,
                    Interaction::OnlyInteracting,
                    Some(InputEvent::Leave),
                );
                self.pointers.retain(|&known, _| known != pointer);
                left
            }
            DeviceEvent::Button {
                pointer,
                button,
                state,
                at,
                serial,
            } => self.route_button(pointer, button, state, at, serial),
            DeviceEvent::Axis { pointer, at, delta } => {
                let scroll = InputEvent::Scroll { at, delta };
                self.route_pointer_input(pointer, Interaction::TakesOver, Some(scroll))
            }
            DeviceEvent::Key {
                keyboard,
                stroke,
                state,
            } => self
                .route_key(keyboard, stroke, state)
                .into_iter()
                .collect(),
            DeviceEvent::Modifiers {
                keyboard,
                modifiers,
            } => {
                let keyboard_state = self.keyboards.entry(keyboard).or_default();
                keyboard_state.modifiers = modifiers;
                let changed = InputEvent::Modifiers(modifiers);
                (keyboard_state.focus)
                    .map(|surface| input_without_pointer(surface, changed))
                    .into_iter()
                    .collect()
            }
            DeviceEvent::KeyboardFocus { keyboard, surface } => {
                self.focus_keyboard(keyboard, surface)
            }
            DeviceEvent::PointerRemoved(pointer) => self.remove_pointer(pointer),
            DeviceEvent::KeyboardRemoved(keyboard) => {
                let left = self.focus_keyboard(keyboard, None);
                self.keyboards.retain(|&known, _| known != keyboard);
                left
            }
            DeviceEvent::TouchDown { surface, sample } => vec![self.touches.down(surface, sample)],
            DeviceEvent::TouchMotion(sample) => self.touches.motion(sample).into_iter().collect(),
            DeviceEvent::TouchUp(sample) => self.touches.up(sample).into_iter().collect(),
            DeviceEvent::TouchCancel(touch) => self.touches.cancel(touch).into_iter().collect(),
        }
    }

    /// The surface focused by `keyboard`.
    #[must_use]
    pub fn keyboard_focus(&self, keyboard: KeyboardDevice) -> Option<SurfaceId> {
        self.keyboards.get(&keyboard)?.focus
    }

    /// Whether a keyboard focuses `surface`.
    #[must_use]
    pub fn is_keyboard_focused(&self, surface: SurfaceId) -> bool {
        (self.keyboards.values()).any(|keyboard| keyboard.focus == Some(surface))
    }

    /// The pointer that interacts with `surface`.
    #[must_use]
    pub fn interacting_pointer(&self, surface: SurfaceId) -> Option<PointerDevice> {
        self.interactions.get(&surface).copied()
    }

    /// Removes the record of the pointers that are over `surface` or interacted with it and of the touch points that
    /// went down on it, once the surface retires.
    pub fn forget_surface(&mut self, surface: SurfaceId) {
        self.interactions.retain(|&known, _| known != surface);
        self.pointers.retain(|_, &mut under| under != surface);
        self.touches.forget_surface(surface);
    }

    /// Routes `input` made by `pointer` to the surface under it.
    fn route_pointer_input(
        &mut self,
        pointer: PointerDevice,
        interaction: Interaction,
        input: Option<InputEvent>,
    ) -> Vec<RoutedInput> {
        self.route_pointer(pointer, interaction, |surface| {
            input.map(|input| input_with_pointer(surface, input, pointer))
        })
    }

    /// Routes the input that `routed` returns for the surface under `pointer`. When `pointer` becomes the pointer of
    /// the surface in place of another pointer, the surface first receives a cancel for the other pointer.
    fn route_pointer(
        &mut self,
        pointer: PointerDevice,
        interaction: Interaction,
        routed: impl FnOnce(SurfaceId) -> Option<RoutedInput>,
    ) -> Vec<RoutedInput> {
        let Some(&surface) = self.pointers.get(&pointer) else {
            return Vec::new();
        };
        let cancel = match interaction {
            Interaction::OnlyInteracting if self.interacting_pointer(surface) != Some(pointer) => {
                return Vec::new();
            }
            Interaction::OnlyInteracting => None,
            Interaction::TakesOver => (self.interactions.insert(surface, pointer))
                .filter(|&previous| previous != pointer)
                .map(|previous| RoutedInput::Cancel {
                    surface,
                    pointer: previous,
                }),
        };
        cancel.into_iter().chain(routed(surface)).collect()
    }

    /// Routes a press or release of `button` of `pointer` at `at`.
    fn route_button(
        &mut self,
        pointer: PointerDevice,
        button: Button,
        state: PressState,
        at: Point,
        serial: InputSerial,
    ) -> Vec<RoutedInput> {
        let interaction = match state {
            PressState::Pressed => Interaction::TakesOver,
            PressState::Released => Interaction::OnlyInteracting,
        };
        self.route_pointer(pointer, interaction, |surface| match (button, state) {
            (Button::Primary, PressState::Pressed) => Some(RoutedInput::PrimaryPress {
                surface,
                pointer,
                at,
                serial,
            }),
            (Button::Primary, PressState::Released) => Some(input_with_pointer(
                surface,
                InputEvent::Release(at),
                pointer,
            )),
            (Button::Secondary, PressState::Pressed) => Some(input_with_pointer(
                surface,
                InputEvent::Context(at),
                pointer,
            )),
            (Button::Secondary | Button::Other, PressState::Released)
            | (Button::Other, PressState::Pressed) => None,
        })
    }

    /// Moves the focus of `keyboard` to `surface`. Returns the window focus of the surface that `keyboard` left and of
    /// `surface`, for each of them that gained the focus of a keyboard or lost the focus of the last keyboard, the left
    /// surface first.
    fn focus_keyboard(
        &mut self,
        keyboard: KeyboardDevice,
        surface: Option<SurfaceId>,
    ) -> Vec<RoutedInput> {
        let before: Vec<_> = (self.keyboard_focus(keyboard).into_iter().chain(surface))
            .map(|known| (known, self.is_keyboard_focused(known)))
            .collect();
        self.keyboards.entry(keyboard).or_default().focus = surface;
        (before.into_iter())
            .filter(|&(known, focused)| self.is_keyboard_focused(known) != focused)
            .map(|(known, lost)| input_without_pointer(known, InputEvent::WindowFocus(!lost)))
            .collect()
    }

    /// The input of a key of `keyboard` for the surface focused by it. The filter receives each pressed key.
    fn route_key(
        &mut self,
        keyboard: KeyboardDevice,
        stroke: KeyStroke,
        state: PressState,
    ) -> Option<RoutedInput> {
        let &KeyboardState { focus, modifiers } = self.keyboards.get(&keyboard)?;
        let surface = focus?;
        let input = match state {
            PressState::Pressed => {
                let input = InputEvent::from_key_press(stroke, modifiers)?;
                match &mut self.filter {
                    Some(filter) => filter(surface, input)?,
                    None => input,
                }
            }
            PressState::Released => InputEvent::from_key_release(&stroke)?,
        };
        Some(input_without_pointer(surface, input))
    }

    /// Removes `pointer` from the router. Each surface with which the pointer interacted receives a cancel.
    fn remove_pointer(&mut self, pointer: PointerDevice) -> Vec<RoutedInput> {
        self.pointers.retain(|&known, _| known != pointer);
        let cancels = (self.interactions.iter())
            .filter(|&(_, &interacting)| interacting == pointer)
            .map(|(&surface, _)| RoutedInput::Cancel { surface, pointer })
            .collect();
        self.interactions
            .retain(|_, &mut interacting| interacting != pointer);
        cancels
    }
}

/// An input for `surface` after which `pointer` gets the cursor shown by the view.
fn input_with_pointer(
    surface: SurfaceId,
    input: InputEvent,
    pointer: PointerDevice,
) -> RoutedInput {
    RoutedInput::Input {
        surface,
        input,
        pointer: Some(pointer),
    }
}

/// An input for `surface` that leaves the cursor of every pointer unchanged.
fn input_without_pointer(surface: SurfaceId, input: InputEvent) -> RoutedInput {
    RoutedInput::Input {
        surface,
        input,
        pointer: None,
    }
}

#[cfg(test)]
mod tests;
