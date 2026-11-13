// SPDX-License-Identifier: GPL-3.0-only
//! Reads the display and the configuration from an android.content.Context and returns them as host reports.

use crate::Error;
use crate::error::ensure_no_exception;
use crate::output::{DisplayReading, HostDisplay, display_from};
use crate::settings::{ConfigurationReading, HostConfiguration, configuration_from};
use jni::objects::{JObject, JString};
use jni::strings::JNIStr;
use jni::sys::jint;
use jni::{Env, JValue, JValueOwned, jni_sig, jni_str};

/// The local references that reading one locale makes: the java.util.Locale and its language tag.
const REFERENCES_PER_LOCALE: usize = 2;

/// The display of `context`, an android.content.Context associated with a display, such as an activity.
///
/// # Errors
/// Fails when a Java exception is pending, when the display cannot be read, and when its dpi cannot be converted into
/// a density.
pub fn display_of(env: &mut Env<'_>, context: &JObject<'_>) -> Result<HostDisplay, Error> {
    ensure_no_exception(env)?;
    display_from(read_display(env, context).map_err(Error::ReadDisplay)?)
}

/// The night mode and the locales of the configuration of `context`, an android.content.Context.
///
/// # Errors
/// Fails when a Java exception is pending, and when the configuration cannot be read.
pub fn configuration_of(
    env: &mut Env<'_>,
    context: &JObject<'_>,
) -> Result<HostConfiguration, Error> {
    ensure_no_exception(env)?;
    let reading = read_configuration(env, context).map_err(Error::ReadConfiguration)?;
    Ok(configuration_from(reading))
}

/// The size of the largest window on the display of `context`, the dpi from the display metrics of `context`, and the
/// refresh rate of the display that shows it.
fn read_display(env: &mut Env<'_>, context: &JObject<'_>) -> jni::errors::Result<DisplayReading> {
    let bounds = maximum_window_bounds(env, context)?;
    let width = int_of(env, &bounds, jni_str!("width"))?;
    let height = int_of(env, &bounds, jni_str!("height"))?;
    let resources = resources_of(env, context)?;
    let metrics = env
        .call_method(
            &resources,
            jni_str!("getDisplayMetrics"),
            jni_sig!("()Landroid/util/DisplayMetrics;"),
            &[],
        )?
        .l()?;
    let density_dpi = env
        .get_field(&metrics, jni_str!("densityDpi"), jni_sig!("I"))
        .and_then(JValueOwned::i)?;
    let display = env
        .call_method(
            context,
            jni_str!("getDisplay"),
            jni_sig!("()Landroid/view/Display;"),
            &[],
        )?
        .l()?;
    let refresh_rate = env
        .call_method(&display, jni_str!("getRefreshRate"), jni_sig!("()F"), &[])
        .and_then(JValueOwned::f)?;
    Ok(DisplayReading {
        width: f64::from(width),
        height: f64::from(height),
        density_dpi: f64::from(density_dpi),
        refresh_rate: f64::from(refresh_rate),
    })
}

/// The bounds of the largest window that the window manager of `context` can show, an android.graphics.Rect.
fn maximum_window_bounds<'local>(
    env: &mut Env<'local>,
    context: &JObject<'_>,
) -> jni::errors::Result<JObject<'local>> {
    let class = env.find_class(jni_str!("android/view/WindowManager"))?;
    let manager = env
        .call_method(
            context,
            jni_str!("getSystemService"),
            jni_sig!("(Ljava/lang/Class;)Ljava/lang/Object;"),
            &[JValue::from(&class)],
        )?
        .l()?;
    let metrics = env
        .call_method(
            &manager,
            jni_str!("getMaximumWindowMetrics"),
            jni_sig!("()Landroid/view/WindowMetrics;"),
            &[],
        )?
        .l()?;
    env.call_method(
        &metrics,
        jni_str!("getBounds"),
        jni_sig!("()Landroid/graphics/Rect;"),
        &[],
    )?
    .l()
}

/// The result of the method `name` of `object`, which takes no arguments and returns an int.
fn int_of(env: &mut Env<'_>, object: &JObject<'_>, name: &JNIStr) -> jni::errors::Result<jint> {
    (env.call_method(object, name, jni_sig!("()I"), &[])).and_then(JValueOwned::i)
}

/// Whether the night mode is active in the configuration of `context`, and the language tags of the locales in that
/// configuration.
fn read_configuration(
    env: &mut Env<'_>,
    context: &JObject<'_>,
) -> jni::errors::Result<ConfigurationReading> {
    let resources = resources_of(env, context)?;
    let configuration = env
        .call_method(
            &resources,
            jni_str!("getConfiguration"),
            jni_sig!("()Landroid/content/res/Configuration;"),
            &[],
        )?
        .l()?;
    let night_mode_active = env
        .call_method(
            &configuration,
            jni_str!("isNightModeActive"),
            jni_sig!("()Z"),
            &[],
        )
        .and_then(JValueOwned::z)?;
    let list = env
        .call_method(
            &configuration,
            jni_str!("getLocales"),
            jni_sig!("()Landroid/os/LocaleList;"),
            &[],
        )?
        .l()?;
    let count = int_of(env, &list, jni_str!("size"))?;
    let language_tags = (0..count)
        .map(|index| language_tag(env, &list, index))
        .collect::<jni::errors::Result<_>>()?;
    Ok(ConfigurationReading {
        night_mode_active,
        language_tags,
    })
}

/// The android.content.res.Resources of `context`.
fn resources_of<'local>(
    env: &mut Env<'local>,
    context: &JObject<'_>,
) -> jni::errors::Result<JObject<'local>> {
    env.call_method(
        context,
        jni_str!("getResources"),
        jni_sig!("()Landroid/content/res/Resources;"),
        &[],
    )?
    .l()
}

/// The BCP 47 language tag of the locale at `index` in `list`, an android.os.LocaleList. The local references made
/// to read it are released before it returns.
fn language_tag(env: &mut Env<'_>, list: &JObject<'_>, index: jint) -> jni::errors::Result<String> {
    env.with_local_frame(REFERENCES_PER_LOCALE, |env| {
        let locale = env
            .call_method(
                list,
                jni_str!("get"),
                jni_sig!("(I)Ljava/util/Locale;"),
                &[JValue::Int(index)],
            )?
            .l()?;
        let tag = env
            .call_method(
                &locale,
                jni_str!("toLanguageTag"),
                jni_sig!("()Ljava/lang/String;"),
                &[],
            )?
            .l()?;
        env.cast_local::<JString>(tag)?.try_to_string(env)
    })
}
