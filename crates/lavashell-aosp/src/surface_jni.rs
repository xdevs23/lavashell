// SPDX-License-Identifier: GPL-3.0-only

use crate::Error;
use jni::objects::JObject;
use jni::{Env, jni_str};
use ndk::native_window::NativeWindow;

pub fn surface_of_window<'local>(
    env: &mut Env<'local>,
    window: &NativeWindow,
) -> Result<JObject<'local>, Error> {
    if env.exception_check() {
        return Err(Error::PendingException);
    }
    // SAFETY: Native code can pass a native window to a Surface only through `ANativeWindow_toSurface`, which ndk
    // 0.9.0 and jni 0.22.4 offer only as an unsafe fn. Its condition is a valid JNIEnv. The JNIEnv pointer comes from
    // the `Env` of the calling thread, which is attached while `env` is borrowed. The check above confirms that no
    // exception is pending. `window` is borrowed for the whole call, so the native window is not released during the
    // call.
    let surface = unsafe { window.to_surface(env.get_raw().cast()) };
    if surface.is_null() {
        return Err(Error::WindowSurface);
    }
    // SAFETY: jni 0.22.4 wraps a raw reference only through this unsafe constructor. `ANativeWindow_toSurface`
    // returns a new local reference of the calling thread. No other object wraps that reference, and the reference
    // lives in the local frame of `env`.
    Ok(unsafe { JObject::from_raw(env, surface.cast()) })
}

pub fn window_of_surface(env: &mut Env<'_>, surface: &JObject<'_>) -> Result<NativeWindow, Error> {
    if env.exception_check() {
        return Err(Error::PendingException);
    }
    if surface.as_raw().is_null() {
        return Err(Error::NullSurface);
    }
    let class = jni_str!("android/view/Surface");
    if !env
        .is_instance_of(surface, class)
        .map_err(Error::SurfaceClass)?
    {
        return Err(Error::NotASurface);
    }
    // SAFETY: Native code can get the native window of a Surface only through `ANativeWindow_fromSurface`, which ndk
    // 0.9.0 and jni 0.22.4 offer only as an unsafe fn. Its conditions are a valid JNIEnv and a live reference to a
    // Surface. The JNIEnv pointer comes from the `Env` of the calling thread, and that thread has not left an exception
    // pending. `surface` is a live reference, not null, to an instance of that Surface class. The checks above confirm
    // both conditions. The returned `NativeWindow` has a new reference, which it releases when it is dropped.
    unsafe { NativeWindow::from_surface(env.get_raw().cast(), surface.as_raw().cast()) }
        .ok_or(Error::SurfaceWindow)
}
