// SPDX-License-Identifier: GPL-3.0-only
use crate::SurfaceRecords;
use lavashell_core::keyboard::KeyStroke;
use lavashell_core::surface::SurfaceId;
use lavashell_core::ui::InputModifiers;
use lavashell_core::view::InputEvent;
use lavashell_platform::{
    Button, DeviceEvent, InputRequest, KeyboardDevice, PointerDevice, PressState,
};
use std::collections::BTreeMap;

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
    /// The interaction of `pointer` with `surface` ends, because another pointer took the surface over or `pointer`
    /// was removed. The view receives a cancel.
    Cancel {
        surface: SurfaceId,
        pointer: PointerDevice,
    },
}

impl RoutedInput {
    /// Hands the input to the view of the routed surface while that surface is open. Returns the request that sets the
    /// cursor of the pointer to the cursor shown by the view.
    pub fn deliver(self, records: &mut SurfaceRecords) -> Option<InputRequest> {
        match self {
            Self::Input {
                surface,
                input,
                pointer,
            } => {
                let view = records.open_view_mut(surface)?;
                view.input(input);
                pointer.map(|pointer| InputRequest::SetCursor(pointer, view.cursor()))
            }
            Self::Cancel { surface, .. } => {
                records
                    .open_view_mut(surface)?
                    .input(InputEvent::CancelPointer);
                None
            }
        }
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

/// Routes the input of each pointer to the view under it, and the input of each keyboard to the view focused by it.
pub struct InputRouter {
    /// The surface under each pointer.
    pointers: BTreeMap<PointerDevice, SurfaceId>,
    /// The pointer that interacts with each surface: the last pointer whose input the surface received.
    interactions: BTreeMap<SurfaceId, PointerDevice>,
    keyboards: BTreeMap<KeyboardDevice, KeyboardState>,
    filter: Option<InputFilter>,
}

impl InputRouter {
    #[must_use]
    pub fn new(filter: Option<InputFilter>) -> Self {
        Self {
            pointers: BTreeMap::new(),
            interactions: BTreeMap::new(),
            keyboards: BTreeMap::new(),
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
            } => {
                let input = match (button, state) {
                    (Button::Primary, PressState::Pressed) => Some(InputEvent::Press(at)),
                    (Button::Primary, PressState::Released) => Some(InputEvent::Release(at)),
                    (Button::Secondary, PressState::Pressed) => Some(InputEvent::Context(at)),
                    (Button::Secondary | Button::Other, PressState::Released) => return Vec::new(),
                    (Button::Other, PressState::Pressed) => None,
                };
                let interaction = match state {
                    PressState::Pressed => Interaction::TakesOver,
                    PressState::Released => Interaction::OnlyInteracting,
                };
                self.route_pointer_input(pointer, interaction, input)
            }
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
                self.keyboards.entry(keyboard).or_default().focus = surface;
                Vec::new()
            }
            DeviceEvent::PointerRemoved(pointer) => self.remove_pointer(pointer),
            DeviceEvent::KeyboardRemoved(keyboard) => {
                self.keyboards.retain(|&known, _| known != keyboard);
                Vec::new()
            }
        }
    }

    /// The surface focused by `keyboard`.
    #[must_use]
    pub fn keyboard_focus(&self, keyboard: KeyboardDevice) -> Option<SurfaceId> {
        self.keyboards.get(&keyboard)?.focus
    }

    /// The pointer that interacts with `surface`.
    #[must_use]
    pub fn interacting_pointer(&self, surface: SurfaceId) -> Option<PointerDevice> {
        self.interactions.get(&surface).copied()
    }

    /// Removes the record of the pointers that are over `surface` or interacted with it, once the surface retires.
    pub fn forget_surface(&mut self, surface: SurfaceId) {
        self.interactions.retain(|&known, _| known != surface);
        self.pointers.retain(|_, &mut under| under != surface);
    }

    /// Routes `input` made by `pointer` to the surface under it. When `pointer` takes the surface over from another
    /// pointer, the surface first receives a cancel for the other pointer.
    fn route_pointer_input(
        &mut self,
        pointer: PointerDevice,
        interaction: Interaction,
        input: Option<InputEvent>,
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
        let input = input.map(|input| RoutedInput::Input {
            surface,
            input,
            pointer: Some(pointer),
        });
        cancel.into_iter().chain(input).collect()
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
