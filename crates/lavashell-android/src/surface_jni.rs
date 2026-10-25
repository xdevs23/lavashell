// SPDX-License-Identifier: GPL-3.0-only
//! Conversions between a Java android.view.Surface and the native window behind it.

use crate::Error;
use jni::objects::JObject;
use jni::{Env, jni_str};
use ndk::native_window::NativeWindow;

/// A new local reference to the Java android.view.Surface for `window`. It passes a native window, such as the one of
/// a `SurfaceStream`, to a producer in Java.
///
/// # Errors
/// Fails when a Java exception is pending, or when AOSP returns no Surface.
pub fn surface_of_window<'local>(
    env: &mut Env<'local>,
    window: &NativeWindow,
) -> Result<JObject<'local>, Error> {
    if env.exception_check() {
        return Err(Error::PendingException);
    }
    // SAFETY: Native code can pass a native window to Java only through `ANativeWindow_toSurface`, which ndk 0.9.0 and
    // jni 0.22.4 offer only as an unsafe fn. Its condition is a valid JNIEnv. The JNIEnv pointer comes from the `Env`
    // of the calling thread, which is attached while `env` is borrowed. The check above confirms that no exception is
    // pending. `window` is borrowed for the whole call, so the native window is not released during the call.
    let surface = unsafe { window.to_surface(env.get_raw().cast()) };
    if surface.is_null() {
        return Err(Error::WindowSurface);
    }
    // SAFETY: jni 0.22.4 wraps a raw reference only through this unsafe constructor. `ANativeWindow_toSurface`
    // returns a new local reference of the calling thread. No other object wraps that reference, and the reference
    // lives in the local frame of `env`.
    Ok(unsafe { JObject::from_raw(env, surface.cast()) })
}

/// The native window behind `surface`, an android.view.Surface, with a new reference added to the native window. The
/// caller may release the Java reference after this call.
///
/// # Errors
/// Fails when a Java exception is pending, when `surface` is null or not an android.view.Surface, or when AOSP returns
/// no native window.
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
    // 0.9.0 and jni 0.22.4 offer only as an unsafe fn. Its conditions are a valid JNIEnv and a live reference to an
    // android.view.Surface. The JNIEnv pointer comes from the `Env` of the calling thread, with no exception pending.
    // `surface` is a live reference, not null, to an instance of android.view.Surface. The checks above confirm both
    // conditions. The returned `NativeWindow` has a new reference, which it releases when it is dropped.
    unsafe { NativeWindow::from_surface(env.get_raw().cast(), surface.as_raw().cast()) }
        .ok_or(Error::SurfaceWindow)
}
