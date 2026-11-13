// SPDX-License-Identifier: GPL-3.0-only
//! Conversion of a Java android.view.MotionEvent into a motion event on the Surface shown by the host.

use crate::Error;
use crate::input::{Motion, MotionEvent, MouseAction, MousePosition, TouchAction, TouchPointer};
use jni::objects::JObject;
use jni::{Env, JValueOwned, jni_sig, jni_str};
use lavashell_core::ui::PointerKind;
use lavashell_core::units::Px;
use lavashell_platform::{Button, PressState};
use ndk::event::{MotionAction as ActionCode, Pointer, SourceClass, ToolType};
use std::ptr::NonNull;

/// `android.view.MotionEvent.FLAG_CANCELED`, which the NDK headers do not define:
/// <https://developer.android.com/reference/android/view/MotionEvent#FLAG_CANCELED>.
const FLAG_CANCELED: u32 = 0x20;

/// Why an android.view.MotionEvent cannot become a motion event.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum MalformedMotion {
    #[error("the action index {index} is outside the {count} pointers of the event")]
    PointerIndex { index: usize, count: usize },
    #[error("the action button {0:#x} of a button action is none of the mouse buttons")]
    ActionButton(i32),
}

/// The motion event read from `event`, an android.view.MotionEvent, or `None` for an action or a source that
/// produces no device events.
///
/// # Errors
/// Fails when a Java exception is pending, when `event` is null or not an android.view.MotionEvent, when AOSP returns
/// no native copy of it, when its action button cannot be read, and when it is malformed.
pub fn motion_event_of(
    env: &mut Env<'_>,
    event: &JObject<'_>,
) -> Result<Option<MotionEvent>, Error> {
    let copy = NativeMotionEvent::copy(env, event)?;
    let native = &copy.0;
    if !native.source().class().contains(SourceClass::POINTER) {
        return Ok(None);
    }
    let pointers: Vec<Pointer<'_>> = native.pointers().collect();
    let index = native.pointer_index();
    let malformed = MalformedMotion::PointerIndex {
        index,
        count: pointers.len(),
    };
    let changed = pointers.get(index).ok_or(malformed)?;
    let motion = match kind_of_tool(changed.tool_type()) {
        PointerKind::Mouse => {
            let action = mouse_action(env, event, native, changed)?;
            action.map(Motion::Mouse)
        }
        PointerKind::Finger | PointerKind::Pen => {
            let seat = native.device_id().cast_unsigned();
            let action = touch_action(native, &pointers, changed);
            action.map(|action| Motion::Touch { seat, action })
        }
    };
    Ok(motion.map(MotionEvent))
}

/// The native copy of a Java android.view.MotionEvent. Dropping it releases the copy.
struct NativeMotionEvent(ndk::event::MotionEvent);

impl NativeMotionEvent {
    fn copy(env: &mut Env<'_>, event: &JObject<'_>) -> Result<Self, Error> {
        if env.exception_check() {
            return Err(Error::PendingException);
        }
        if event.as_raw().is_null() {
            return Err(Error::NullMotionEvent);
        }
        let class = jni_str!("android/view/MotionEvent");
        if !env
            .is_instance_of(event, class)
            .map_err(Error::MotionEventClass)?
        {
            return Err(Error::NotAMotionEvent);
        }
        let raw_env = env.get_raw().cast();
        // SAFETY: ndk 0.9.0 returns the copy of `AMotionEvent_fromJava` in a type without accessors, so the copy is
        // made through ndk-sys, which offers it only as an unsafe fn. Its conditions are a valid JNIEnv and a live
        // reference to an android.view.MotionEvent. The JNIEnv pointer comes from the `Env` of the calling thread, with
        // no exception pending. `event` is a live reference, not null, to an instance of android.view.MotionEvent. The
        // checks above confirm both conditions.
        let copy = unsafe { ndk_sys::AMotionEvent_fromJava(raw_env, event.as_raw().cast()) };
        let copy = NonNull::new(copy.cast_mut()).ok_or(Error::MotionEventCopy)?;
        // SAFETY: ndk 0.9.0 wraps a native motion event only through this unsafe constructor. `copy` is the non-null
        // motion event returned by `AMotionEvent_fromJava`, and it lives until `drop` releases it.
        Ok(Self(unsafe { ndk::event::MotionEvent::from_ptr(copy) }))
    }
}

impl Drop for NativeMotionEvent {
    fn drop(&mut self) {
        // SAFETY: ndk-sys offers the release only as an unsafe fn. The pointer is the copy returned by
        // `AMotionEvent_fromJava`, and only this drop releases it.
        unsafe { ndk_sys::AInputEvent_release(self.0.ptr().as_ptr()) }
    }
}

/// The touch action of `native`, made by its pointer `changed`, or `None` for an action that moves no touch point.
fn touch_action(
    native: &ndk::event::MotionEvent,
    pointers: &[Pointer<'_>],
    changed: &Pointer<'_>,
) -> Option<TouchAction> {
    let canceled = native.flags().0 & FLAG_CANCELED != 0;
    let action = match native.action() {
        ActionCode::Down | ActionCode::PointerDown => TouchAction::Down(touch_pointer(changed)),
        ActionCode::PointerUp if canceled => TouchAction::Cancel(vec![changed.pointer_id()]),
        ActionCode::Up | ActionCode::PointerUp => TouchAction::Up(touch_pointer(changed)),
        ActionCode::Move => TouchAction::Move(pointers.iter().map(touch_pointer).collect()),
        ActionCode::Cancel => {
            TouchAction::Cancel(pointers.iter().map(Pointer::pointer_id).collect())
        }
        _ => return None,
    };
    Some(action)
}

fn touch_pointer(pointer: &Pointer<'_>) -> TouchPointer {
    TouchPointer {
        id: pointer.pointer_id(),
        kind: kind_of_tool(pointer.tool_type()),
        x: to_px(pointer.x()),
        y: to_px(pointer.y()),
    }
}

/// The mouse action of `native`, made by its pointer `changed`. The action button is read from `event`, because the
/// NDK has it only from API 33.
fn mouse_action(
    env: &mut Env<'_>,
    event: &JObject<'_>,
    native: &ndk::event::MotionEvent,
    changed: &Pointer<'_>,
) -> Result<Option<MouseAction>, Error> {
    let position = MousePosition {
        x: to_px(changed.x()),
        y: to_px(changed.y()),
        held: native.button_state().0 != 0,
    };
    Ok(match native.action() {
        ActionCode::HoverEnter | ActionCode::HoverMove | ActionCode::Move => {
            Some(MouseAction::Move(position))
        }
        // A hover exit while a button is down starts a press, so the mouse stays over the surface.
        ActionCode::HoverExit if position.held => None,
        ActionCode::HoverExit => Some(MouseAction::Exit),
        ActionCode::ButtonPress => Some(MouseAction::Button {
            position,
            button: action_button(env, event)?,
            state: PressState::Pressed,
        }),
        ActionCode::ButtonRelease => Some(MouseAction::Button {
            position,
            button: action_button(env, event)?,
            state: PressState::Released,
        }),
        ActionCode::Cancel => Some(MouseAction::Cancel),
        // The button actions contain every press, so the down and the up of a mouse produce no device events.
        _ => None,
    })
}

/// The button that a button action of `event` pressed or released.
fn action_button(env: &mut Env<'_>, event: &JObject<'_>) -> Result<Button, Error> {
    let action_button = (env.call_method(event, jni_str!("getActionButton"), jni_sig!("()I"), &[]))
        .and_then(JValueOwned::i)
        .map_err(Error::ActionButton)?;
    match action_button.cast_unsigned() {
        ndk_sys::AMOTION_EVENT_BUTTON_PRIMARY => Ok(Button::Primary),
        ndk_sys::AMOTION_EVENT_BUTTON_SECONDARY => Ok(Button::Secondary),
        ndk_sys::AMOTION_EVENT_BUTTON_TERTIARY
        | ndk_sys::AMOTION_EVENT_BUTTON_BACK
        | ndk_sys::AMOTION_EVENT_BUTTON_FORWARD => Ok(Button::Other),
        _ => Err(MalformedMotion::ActionButton(action_button).into()),
    }
}

/// The kind of pointer of `tool`. A palm, and a tool that the device does not report, touch like a finger.
fn kind_of_tool(tool: ToolType) -> PointerKind {
    match tool {
        ToolType::Stylus | ToolType::Eraser => PointerKind::Pen,
        ToolType::Mouse => PointerKind::Mouse,
        _ => PointerKind::Finger,
    }
}

fn to_px(pixels: f32) -> Px {
    Px::new(f64::from(pixels))
}
