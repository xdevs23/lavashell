// SPDX-License-Identifier: GPL-3.0-only
use crate::Error;
use crate::error::ensure_no_exception;
use crate::output::{DisplayReading, HostDisplay};
use crate::settings::{ConfigurationReading, HostConfiguration};
use jni::objects::{JObject, JString};
use jni::signature::MethodSignature;
use jni::strings::JNIStr;
use jni::sys::{jboolean, jfloat, jint};
use jni::{Env, JValue, JValueOwned, jni_sig, jni_str};
use lavashell_core::units::{Px, RefreshRate};

const REFERENCES_PER_LOCALE: usize = 2;

pub fn display_of(env: &mut Env<'_>, context: &JObject<'_>) -> Result<HostDisplay, Error> {
    ensure_no_exception(env)?;
    HostDisplay::try_from(read_display(env, context).map_err(Error::ReadDisplay)?)
}

pub fn configuration_of(
    env: &mut Env<'_>,
    context: &JObject<'_>,
) -> Result<HostConfiguration, Error> {
    ensure_no_exception(env)?;
    let reading = read_configuration(env, context).map_err(Error::ReadConfiguration)?;
    Ok(HostConfiguration::from(reading))
}

fn read_display(env: &mut Env<'_>, context: &JObject<'_>) -> jni::errors::Result<DisplayReading> {
    let bounds = maximum_window_bounds(env, context)?;
    let width = int_of(env, &bounds, jni_str!("width"))?;
    let height = int_of(env, &bounds, jni_str!("height"))?;
    let resources = resources_of(env, context)?;
    let metrics = object_of(
        env,
        &resources,
        jni_str!("getDisplayMetrics"),
        jni_sig!("()Landroid/util/DisplayMetrics;"),
    )?;
    let density_dpi = env
        .get_field(&metrics, jni_str!("densityDpi"), jni_sig!("I"))
        .and_then(JValueOwned::i)?;
    let display = object_of(
        env,
        context,
        jni_str!("getDisplay"),
        jni_sig!("()Landroid/view/Display;"),
    )?;
    let refresh_rate_hz = float_of(env, &display, jni_str!("getRefreshRate"))?;
    Ok(DisplayReading {
        width: Px::new(f64::from(width)),
        height: Px::new(f64::from(height)),
        density_dpi: f64::from(density_dpi),
        refresh_rate: RefreshRate::from_hertz(f64::from(refresh_rate_hz)),
    })
}

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
    let metrics = object_of(
        env,
        &manager,
        jni_str!("getMaximumWindowMetrics"),
        jni_sig!("()Landroid/view/WindowMetrics;"),
    )?;
    object_of(
        env,
        &metrics,
        jni_str!("getBounds"),
        jni_sig!("()Landroid/graphics/Rect;"),
    )
}

fn object_of<'local>(
    env: &mut Env<'local>,
    object: &JObject<'_>,
    method: &JNIStr,
    signature: MethodSignature<'_, '_>,
) -> jni::errors::Result<JObject<'local>> {
    env.call_method(object, method, signature, &[])?.l()
}

fn int_of(env: &mut Env<'_>, object: &JObject<'_>, method: &JNIStr) -> jni::errors::Result<jint> {
    (env.call_method(object, method, jni_sig!("()I"), &[])).and_then(JValueOwned::i)
}

fn float_of(
    env: &mut Env<'_>,
    object: &JObject<'_>,
    method: &JNIStr,
) -> jni::errors::Result<jfloat> {
    (env.call_method(object, method, jni_sig!("()F"), &[])).and_then(JValueOwned::f)
}

fn bool_of(
    env: &mut Env<'_>,
    object: &JObject<'_>,
    method: &JNIStr,
) -> jni::errors::Result<jboolean> {
    (env.call_method(object, method, jni_sig!("()Z"), &[])).and_then(JValueOwned::z)
}

fn read_configuration(
    env: &mut Env<'_>,
    context: &JObject<'_>,
) -> jni::errors::Result<ConfigurationReading> {
    let resources = resources_of(env, context)?;
    let configuration = object_of(
        env,
        &resources,
        jni_str!("getConfiguration"),
        jni_sig!("()Landroid/content/res/Configuration;"),
    )?;
    let night_mode_active = bool_of(env, &configuration, jni_str!("isNightModeActive"))?;
    let list = object_of(
        env,
        &configuration,
        jni_str!("getLocales"),
        jni_sig!("()Landroid/os/LocaleList;"),
    )?;
    let count = int_of(env, &list, jni_str!("size"))?;
    let language_tags = (0..count)
        .map(|index| language_tag(env, &list, index))
        .collect::<jni::errors::Result<_>>()?;
    Ok(ConfigurationReading {
        night_mode_active,
        language_tags,
    })
}

fn resources_of<'local>(
    env: &mut Env<'local>,
    context: &JObject<'_>,
) -> jni::errors::Result<JObject<'local>> {
    object_of(
        env,
        context,
        jni_str!("getResources"),
        jni_sig!("()Landroid/content/res/Resources;"),
    )
}

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
        let tag = object_of(
            env,
            &locale,
            jni_str!("toLanguageTag"),
            jni_sig!("()Ljava/lang/String;"),
        )?;
        env.cast_local::<JString>(tag)?.try_to_string(env)
    })
}
