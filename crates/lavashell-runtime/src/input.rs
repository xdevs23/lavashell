// SPDX-License-Identifier: GPL-3.0-only
use crate::surface_records::SurfaceRecords;
use crate::window_control::begin_window_control;
use lavashell_core::geometry::Point;
use lavashell_core::keyboard::KeyStroke;
use lavashell_core::surface::SurfaceId;
use lavashell_core::ui::InputModifiers;
use lavashell_core::view::InputEvent;
use lavashell_platform::{
    Button, DeviceEvent, InputRequest, InputSerial, KeyboardDevice, PlatformRequest, PointerDevice,
    PointerPress, PressState,
};
use std::collections::BTreeMap;
use touch::TouchPoints;

mod touch;

pub type InputFilter = Box<dyn FnMut(SurfaceId, InputEvent) -> Option<InputEvent>>;

#[derive(Debug, PartialEq)]
pub(crate) enum RoutedInput {
    Input {
        surface: SurfaceId,
        input: InputEvent,
        pointer: Option<PointerDevice>,
    },
    PrimaryPress {
        surface: SurfaceId,
        press: PointerPress,
        at: Point,
    },
    Cancel {
        surface: SurfaceId,
        pointer: PointerDevice,
    },
}

impl RoutedInput {
    pub fn deliver(self, records: &mut SurfaceRecords) -> Option<PlatformRequest> {
        match self {
            Self::Input {
                surface,
                input,
                pointer,
            } => deliver_input(records, surface, input, pointer),
            Self::PrimaryPress { surface, press, at } => {
                match begin_window_control(records, surface, press, at) {
                    Some(request) => {
                        cancel_pointer(records, surface);
                        Some(PlatformRequest::WindowControl(request))
                    }
                    None => {
                        deliver_input(records, surface, InputEvent::Press(at), Some(press.pointer))
                    }
                }
            }
            Self::Cancel { surface, .. } => {
                cancel_pointer(records, surface);
                None
            }
        }
    }
}

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

#[derive(Clone, Copy)]
enum Interaction {
    TakesOver,
    OnlyInteracting,
}

#[derive(Default)]
struct KeyboardState {
    focus: Option<SurfaceId>,
    modifiers: InputModifiers,
}

pub(crate) struct InputRouter {
    pointers: BTreeMap<PointerDevice, SurfaceId>,
    interactions: BTreeMap<SurfaceId, PointerDevice>,
    last_presses: BTreeMap<SurfaceId, PointerPress>,
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
            last_presses: BTreeMap::new(),
            keyboards: BTreeMap::new(),
            touches: TouchPoints::default(),
            filter,
        }
    }

    pub fn route(&mut self, event: DeviceEvent) -> Vec<RoutedInput> {
        match event {
            DeviceEvent::PointerEnter {
                pointer,
                surface,
                at,
            } => {
                let _earlier_surface_of_pointer = self.pointers.insert(pointer, surface);
                self.route_pointer_input(pointer, Interaction::TakesOver, InputEvent::Move(at))
            }
            DeviceEvent::PointerMotion { pointer, at } => {
                self.route_pointer_input(pointer, Interaction::TakesOver, InputEvent::Move(at))
            }
            DeviceEvent::PointerLeave { pointer } => {
                let left = self.route_pointer_input(
                    pointer,
                    Interaction::OnlyInteracting,
                    InputEvent::Leave,
                );
                let _surface_left_by_pointer = self.pointers.remove(&pointer);
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
                self.route_pointer_input(pointer, Interaction::TakesOver, scroll)
            }
            DeviceEvent::Key {
                keyboard,
                stroke,
                action,
                ..
            } => self
                .route_key(keyboard, stroke, action.press_state())
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
                let _state_of_removed_keyboard = self.keyboards.remove(&keyboard);
                left
            }
            DeviceEvent::TouchDown { surface, sample } => vec![self.touches.down(surface, sample)],
            DeviceEvent::TouchMotion(sample) => self.touches.motion(sample).into_iter().collect(),
            DeviceEvent::TouchUp(sample) => self.touches.up(sample).into_iter().collect(),
            DeviceEvent::TouchCancel(touch) => self.touches.cancel(touch).into_iter().collect(),
        }
    }

    #[must_use]
    pub fn keyboard_focus(&self, keyboard: KeyboardDevice) -> Option<SurfaceId> {
        self.keyboards.get(&keyboard)?.focus
    }

    #[must_use]
    pub fn is_keyboard_focused(&self, surface: SurfaceId) -> bool {
        (self.keyboards.values()).any(|keyboard| keyboard.focus == Some(surface))
    }

    #[must_use]
    pub fn pointer_surface(&self, pointer: PointerDevice) -> Option<SurfaceId> {
        self.pointers.get(&pointer).copied()
    }

    #[must_use]
    pub fn interacting_pointer(&self, surface: SurfaceId) -> Option<PointerDevice> {
        self.interactions.get(&surface).copied()
    }

    #[must_use]
    pub fn last_press(&self, surface: SurfaceId) -> Option<PointerPress> {
        self.last_presses.get(&surface).copied()
    }

    pub fn forget_surface(&mut self, surface: SurfaceId) {
        let _pointer_of_ended_interaction = self.interactions.remove(&surface);
        let _press_on_forgotten_surface = self.last_presses.remove(&surface);
        self.pointers.retain(|_, &mut under| under != surface);
        self.touches.forget_surface(surface);
    }

    fn route_pointer_input(
        &mut self,
        pointer: PointerDevice,
        interaction: Interaction,
        input: InputEvent,
    ) -> Vec<RoutedInput> {
        self.route_pointer(pointer, interaction, |surface| {
            Some(input_with_pointer(surface, input, pointer))
        })
    }

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
        let press = PointerPress { pointer, serial };
        if let (PressState::Pressed, Some(&surface)) = (state, self.pointers.get(&pointer)) {
            let _earlier_press_on_surface = self.last_presses.insert(surface, press);
        }
        self.route_pointer(pointer, interaction, |surface| match (button, state) {
            (Button::Primary, PressState::Pressed) => {
                Some(RoutedInput::PrimaryPress { surface, press, at })
            }
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

    fn remove_pointer(&mut self, pointer: PointerDevice) -> Vec<RoutedInput> {
        let _surface_under_removed_pointer = self.pointers.remove(&pointer);
        let cancels = (self.interactions.iter())
            .filter(|&(_, &interacting)| interacting == pointer)
            .map(|(&surface, _)| RoutedInput::Cancel { surface, pointer })
            .collect();
        self.interactions
            .retain(|_, &mut interacting| interacting != pointer);
        self.last_presses
            .retain(|_, press| press.pointer != pointer);
        cancels
    }
}

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

fn input_without_pointer(surface: SurfaceId, input: InputEvent) -> RoutedInput {
    RoutedInput::Input {
        surface,
        input,
        pointer: None,
    }
}

#[cfg(test)]
mod tests;
