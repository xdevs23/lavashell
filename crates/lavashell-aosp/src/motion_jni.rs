// SPDX-License-Identifier: GPL-3.0-only
use crate::Error;
use crate::error::ensure_no_exception;
use crate::input::{Motion, MotionEvent, MouseAction, MousePosition, TouchAction, TouchPointer};
use jni::objects::JObject;
use jni::{Env, JValueOwned, jni_sig, jni_str};
use lavashell_core::ui::PointerKind;
use lavashell_core::units::Px;
use lavashell_platform::{Button, PressState};
use ndk::event::{MotionAction as ActionCode, Pointer, SourceClass, ToolType};
use std::ptr::NonNull;

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum MalformedMotion {
    #[error("the action index {index} is outside the {count} pointers of the event")]
    PointerIndex { index: usize, count: usize },
    #[error("the action button {0:#x} of a button action is not one of the mouse buttons")]
    ActionButton(i32),
}

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
            action.map(|action| MotionEvent(Motion::Mouse(action)))
        }
        PointerKind::Finger | PointerKind::Pen => {
            let seat = i64::from(native.device_id());
            let action = touch_action(env, native, &pointers, changed)?;
            action.map(|action| MotionEvent::touch(seat, action))
        }
    };
    Ok(motion)
}

struct NativeMotionEvent(ndk::event::MotionEvent);

impl NativeMotionEvent {
    fn copy(env: &mut Env<'_>, event: &JObject<'_>) -> Result<Self, Error> {
        ensure_no_exception(env)?;
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
        // SAFETY: ndk 0.9.0 returns the copy of `AMotionEvent_fromJava` in a type without accessors, so the code calls
        // ndk-sys to make the copy, which ndk-sys offers only as an unsafe fn. Its conditions are a valid JNIEnv and a
        // live reference to a Java MotionEvent. The JNIEnv pointer comes from the `Env` of the calling thread, and that
        // thread has not left an exception pending. `event` is a live reference, not null, to an instance of the Java
        // MotionEvent class. The checks above confirm both conditions.
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

fn touch_action(
    env: &mut Env<'_>,
    native: &ndk::event::MotionEvent,
    pointers: &[Pointer<'_>],
    changed: &Pointer<'_>,
) -> Result<Option<TouchAction>, Error> {
    let action = match native.action() {
        ActionCode::Down | ActionCode::PointerDown => TouchAction::Down(touch_pointer(changed)),
        ActionCode::PointerUp => match pointer_ending(env, native)? {
            PointerEnding::Canceled => TouchAction::Cancel(vec![changed.pointer_id()]),
            PointerEnding::Lifted => TouchAction::Up(touch_pointer(changed)),
        },
        ActionCode::Up => TouchAction::Up(touch_pointer(changed)),
        ActionCode::Move => TouchAction::Move(pointers.iter().map(touch_pointer).collect()),
        ActionCode::Cancel => {
            TouchAction::Cancel(pointers.iter().map(Pointer::pointer_id).collect())
        }
        _ => return Ok(None),
    };
    Ok(Some(action))
}

enum PointerEnding {
    Lifted,
    Canceled,
}

fn pointer_ending(
    env: &mut Env<'_>,
    native: &ndk::event::MotionEvent,
) -> Result<PointerEnding, Error> {
    let class = jni_str!("android/view/MotionEvent");
    let canceled = (env.get_static_field(class, jni_str!("FLAG_CANCELED"), jni_sig!("I")))
        .and_then(JValueOwned::i)
        .map_err(Error::CanceledFlag)?;
    Ok(if i64::from(native.flags().0) & i64::from(canceled) == 0 {
        PointerEnding::Lifted
    } else {
        PointerEnding::Canceled
    })
}

fn touch_pointer(pointer: &Pointer<'_>) -> TouchPointer {
    TouchPointer {
        id: pointer.pointer_id(),
        kind: kind_of_tool(pointer.tool_type()),
        x: to_px(pointer.x()),
        y: to_px(pointer.y()),
    }
}

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
        _ => None,
    })
}

fn action_button(env: &mut Env<'_>, event: &JObject<'_>) -> Result<Button, Error> {
    let action_button = (env.call_method(event, jni_str!("getActionButton"), jni_sig!("()I"), &[]))
        .and_then(JValueOwned::i)
        .map_err(Error::ActionButton)?;
    match u32::try_from(action_button) {
        Ok(ndk_sys::AMOTION_EVENT_BUTTON_PRIMARY) => Ok(Button::Primary),
        Ok(ndk_sys::AMOTION_EVENT_BUTTON_SECONDARY) => Ok(Button::Secondary),
        Ok(
            ndk_sys::AMOTION_EVENT_BUTTON_TERTIARY
            | ndk_sys::AMOTION_EVENT_BUTTON_BACK
            | ndk_sys::AMOTION_EVENT_BUTTON_FORWARD,
        ) => Ok(Button::Other),
        _ => Err(MalformedMotion::ActionButton(action_button).into()),
    }
}

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
