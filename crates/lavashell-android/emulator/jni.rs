// SPDX-License-Identifier: GPL-3.0-only
//! The native methods of `StreamRoundTripActivity`. They convert the values passed between the JVM and Rust.

use crate::error::NativeMethodError;
use crate::stream_round_trip::StreamRoundTrip;
use ::jni::errors::ThrowRuntimeExAndDefault;
use ::jni::objects::JLongArray;
use ::jni::objects::{JClass, JIntArray, JObject};
use ::jni::sys::{jboolean, jfloat, jint, jlong};
use ::jni::{Env, EnvUnowned, JValue, jni_mangle, jni_sig, jni_str};
use lavashell_android::{BufferSizes, StreamFormat, surface_of_window};
use lavashell_core::error::message_chain;
use lavashell_core::prelude::PixelSize;
use lavashell_renderer::skia;
use lavashell_renderer::vulkan::FormatProperties;
use lavashell_vulkan::ycbcr::Conversion;
use ndk::hardware_buffer::{HardwareBuffer, HardwareBufferRef};
use std::cell::RefCell;

thread_local! {
    /// The round trips created on this thread, at the index passed by the activity as the handle. A destroyed round
    /// trip leaves `None` at its index, so a handle never refers to a round trip created later.
    static ROUND_TRIPS: RefCell<Vec<Option<StreamRoundTrip>>> = const { RefCell::new(Vec::new()) };
}

#[jni_mangle("org.lavashell.surfacestream.StreamRoundTripActivity")]
pub fn create<'local>(
    env: EnvUnowned<'local>,
    _: JClass<'local>,
    width: u32,
    height: u32,
    max_width: u32,
    max_height: u32,
    rgba: jboolean,
) -> jlong {
    export(env, |_| {
        let format = if rgba {
            StreamFormat::Rgba8888
        } else {
            StreamFormat::Private
        };
        let round_trip = StreamRoundTrip::new(
            to_buffer_sizes([width, height], [max_width, max_height]),
            format,
        )?;
        ROUND_TRIPS.with(|round_trips| {
            let mut round_trips = round_trips
                .try_borrow_mut()
                .map_err(NativeMethodError::RoundTripsInUse)?;
            let handle =
                jlong::try_from(round_trips.len()).map_err(NativeMethodError::TooManyRoundTrips)?;
            round_trips.push(Some(round_trip));
            Ok(handle)
        })
    })
}

#[jni_mangle("org.lavashell.surfacestream.StreamRoundTripActivity")]
pub fn create_surface<'local>(
    env: EnvUnowned<'local>,
    _: JClass<'local>,
    handle: jlong,
) -> JObject<'local> {
    export(env, |env| {
        with_round_trip(handle, |round_trip| {
            surface_of_window(env, round_trip.producer_window()).map_err(NativeMethodError::Surface)
        })
    })
}

#[jni_mangle("org.lavashell.surfacestream.StreamRoundTripActivity")]
pub fn latch<'local>(env: EnvUnowned<'local>, _: JClass<'local>, handle: jlong) {
    export(env, |_| with_round_trip(handle, StreamRoundTrip::latch));
}

#[jni_mangle("org.lavashell.surfacestream.StreamRoundTripActivity")]
pub fn await_wake<'local>(env: EnvUnowned<'local>, _: JClass<'local>, handle: jlong) {
    export(env, |_| {
        with_round_trip(handle, StreamRoundTrip::await_wake)
    });
}

#[jni_mangle("org.lavashell.surfacestream.StreamRoundTripActivity")]
pub fn render<'local>(env: EnvUnowned<'local>, _: JClass<'local>, handle: jlong) -> jint {
    export(env, |_| {
        with_round_trip(handle, StreamRoundTrip::render).map(to_color_int)
    })
}

#[jni_mangle("org.lavashell.surfacestream.StreamRoundTripActivity")]
pub fn render_grouped<'local>(
    env: EnvUnowned<'local>,
    _: JClass<'local>,
    handle: jlong,
    opacity: jfloat,
) -> JIntArray<'local> {
    export(env, |env| {
        let pixels = with_round_trip(handle, |round_trip| {
            round_trip.render_grouped(f64::from(opacity))
        })?;
        to_jvm_int_array(env, &pixels.map(to_color_int))
    })
}

#[jni_mangle("org.lavashell.surfacestream.StreamRoundTripActivity")]
pub fn read_drawn_size<'local>(
    env: EnvUnowned<'local>,
    _: JClass<'local>,
    handle: jlong,
) -> JLongArray<'local> {
    export(env, |env| {
        let Some(size) = with_round_trip(handle, |round_trip| Ok(round_trip.drawn_size()))? else {
            return Ok(JLongArray::default());
        };
        to_jvm_long_array(env, &[size.width.into(), size.height.into()])
    })
}

#[jni_mangle("org.lavashell.surfacestream.StreamRoundTripActivity")]
pub fn count_wakes<'local>(env: EnvUnowned<'local>, _: JClass<'local>, handle: jlong) -> jlong {
    export(env, |_| {
        with_round_trip(handle, |round_trip| Ok(round_trip.wakes().into()))
    })
}

#[jni_mangle("org.lavashell.surfacestream.StreamRoundTripActivity")]
pub fn resize<'local>(
    env: EnvUnowned<'local>,
    _: JClass<'local>,
    handle: jlong,
    width: u32,
    height: u32,
    max_width: u32,
    max_height: u32,
) {
    export(env, |_| {
        let sizes = to_buffer_sizes([width, height], [max_width, max_height]);
        with_round_trip(handle, |round_trip| round_trip.resize(sizes))
    });
}

#[jni_mangle("org.lavashell.surfacestream.StreamRoundTripActivity")]
pub fn destroy<'local>(env: EnvUnowned<'local>, _: JClass<'local>, handle: jlong) {
    export(env, |_| {
        let round_trip = take_round_trip(handle)?;
        drop(round_trip);
        Ok(())
    });
}

#[jni_mangle("org.lavashell.surfacestream.StreamRoundTripActivity")]
pub fn destroy_counting_wakes<'local>(
    env: EnvUnowned<'local>,
    _: JClass<'local>,
    handle: jlong,
) -> jlong {
    export(env, |_| Ok(take_round_trip(handle)?.close().into()))
}

#[jni_mangle("org.lavashell.surfacestream.StreamRoundTripActivity")]
pub fn encode_ycbcr<'local>(
    env: EnvUnowned<'local>,
    _: JClass<'local>,
    handle: jlong,
    buffer: JObject<'local>,
    color: jint,
) -> JObject<'local> {
    export(env, |env| {
        let buffer = acquire_hardware_buffer(env, &buffer)?;
        let format = with_round_trip(handle, |round_trip| round_trip.read_buffer_format(&buffer))?;
        // The renderer samples `buffer` with the suggested model and range, so the color is encoded with the same ones.
        let model = format.suggested_ycbcr_model.try_into();
        let range = format.suggested_ycbcr_range.try_into();
        let conversion = Conversion::new(
            model.map_err(NativeMethodError::YcbcrConversion)?,
            range.map_err(NativeMethodError::YcbcrConversion)?,
        );
        let [_, red, green, blue] = color.to_be_bytes();
        let ycbcr =
            (conversion.encode([red, green, blue])).ok_or(NativeMethodError::EncodeColor)?;
        let tolerance = (conversion.tolerance()).ok_or(NativeMethodError::EncodeColor)?;
        new_jvm_conversion(env, ycbcr, tolerance, &format)
    })
}

/// Runs `work` for a native method. A failure is thrown into the JVM as a `RuntimeException` whose message lists the
/// failure and its causes. The native method then returns the default value of `T`.
fn export<'local, T: Default>(
    mut env: EnvUnowned<'local>,
    work: impl FnOnce(&mut Env<'local>) -> Result<T, NativeMethodError>,
) -> T {
    env.with_env(|env| match work(env) {
        Ok(value) => Ok(value),
        Err(failure) => throw(env, &failure).map(|()| T::default()),
    })
    .resolve::<ThrowRuntimeExAndDefault>()
}

fn throw(env: &mut Env<'_>, failure: &NativeMethodError) -> ::jni::errors::Result<()> {
    // An exception raised in the JVM during the failed operation is still pending and has more information than the
    // Rust error. That exception is not replaced.
    if env.exception_check() {
        return Ok(());
    }
    match env.throw(message_chain(failure)) {
        // jni reports the exception that `throw` left pending as the error `JavaException`. Here that error means
        // that the throw succeeded.
        Ok(()) | Err(::jni::errors::Error::JavaException) => Ok(()),
        Err(error) => Err(error),
    }
}

fn with_round_trip<T>(
    handle: jlong,
    work: impl FnOnce(&mut StreamRoundTrip) -> Result<T, NativeMethodError>,
) -> Result<T, NativeMethodError> {
    ROUND_TRIPS.with(|round_trips| {
        let mut round_trips = round_trips
            .try_borrow_mut()
            .map_err(NativeMethodError::RoundTripsInUse)?;
        let round_trip = (usize::try_from(handle).ok())
            .and_then(|index| round_trips.get_mut(index))
            .and_then(Option::as_mut)
            .ok_or(NativeMethodError::UnknownHandle(handle))?;
        work(round_trip)
    })
}

fn take_round_trip(handle: jlong) -> Result<StreamRoundTrip, NativeMethodError> {
    ROUND_TRIPS.with(|round_trips| {
        let mut round_trips = round_trips
            .try_borrow_mut()
            .map_err(NativeMethodError::RoundTripsInUse)?;
        (usize::try_from(handle).ok())
            .and_then(|index| round_trips.get_mut(index))
            .and_then(Option::take)
            .ok_or(NativeMethodError::UnknownHandle(handle))
    })
}

fn to_buffer_sizes([width, height]: [u32; 2], [max_width, max_height]: [u32; 2]) -> BufferSizes {
    BufferSizes {
        default: PixelSize::new(width, height),
        max: PixelSize::new(max_width, max_height),
    }
}

/// `color` packed into one ARGB integer, the form of a color int in android.graphics.Color.
fn to_color_int(color: skia::Color) -> jint {
    jint::from_be_bytes([color.a(), color.r(), color.g(), color.b()])
}

fn to_jvm_int_array<'local>(
    env: &mut Env<'local>,
    values: &[jint],
) -> Result<JIntArray<'local>, NativeMethodError> {
    let array = env
        .new_int_array(values.len())
        .map_err(NativeMethodError::ReturnToActivity)?;
    array
        .set_region(env, 0, values)
        .map_err(NativeMethodError::ReturnToActivity)?;
    Ok(array)
}

fn to_jvm_long_array<'local>(
    env: &mut Env<'local>,
    values: &[jlong],
) -> Result<JLongArray<'local>, NativeMethodError> {
    let array = env
        .new_long_array(values.len())
        .map_err(NativeMethodError::ReturnToActivity)?;
    array
        .set_region(env, 0, values)
        .map_err(NativeMethodError::ReturnToActivity)?;
    Ok(array)
}

/// A new JVM object org.lavashell.surfacestream.YcbcrConversion with the Y, Cb and Cr codes of the test color, the
/// tolerance for the drawn color and the format properties returned by Vulkan for the buffer.
fn new_jvm_conversion<'local>(
    env: &mut Env<'local>,
    [y, cb, cr]: [u8; 3],
    tolerance: u8,
    format: &FormatProperties,
) -> Result<JObject<'local>, NativeMethodError> {
    // The JVM has no unsigned 64-bit type. The activity reads these bits back as a ULong.
    let external_format = jlong::from_ne_bytes(format.external_format.to_ne_bytes());
    let arguments = [
        JValue::Int(y.into()),
        JValue::Int(cb.into()),
        JValue::Int(cr.into()),
        JValue::Int(tolerance.into()),
        JValue::Int(format.format.as_raw()),
        JValue::Long(external_format),
        JValue::Long(format.format_features.as_raw().into()),
        JValue::Int(format.suggested_ycbcr_model.as_raw()),
        JValue::Int(format.suggested_ycbcr_range.as_raw()),
        JValue::Int(format.suggested_x_chroma_offset.as_raw()),
        JValue::Int(format.suggested_y_chroma_offset.as_raw()),
    ];
    let class = jni_str!("org/lavashell/surfacestream/YcbcrConversion");
    (env.new_object(class, jni_sig!("(IIIIIJJIIII)V"), &arguments))
        .map_err(NativeMethodError::ReturnToActivity)
}

/// The native `AHardwareBuffer` behind `buffer`, with a new reference added to it. `buffer` must be an
/// android.hardware.HardwareBuffer that no other thread closes during the call.
fn acquire_hardware_buffer(
    env: &mut Env<'_>,
    buffer: &JObject<'_>,
) -> Result<HardwareBufferRef, NativeMethodError> {
    let closed = env.call_method(buffer, jni_str!("isClosed"), jni_sig!("()Z"), &[]);
    if closed
        .and_then(::jni::JValueOwned::z)
        .map_err(NativeMethodError::InspectBuffer)?
    {
        return Err(NativeMethodError::ClosedBuffer);
    }
    // SAFETY: Native code can get the AHardwareBuffer of an android.hardware.HardwareBuffer object only through
    // `AHardwareBuffer_fromHardwareBuffer`. ndk 0.9.0 offers that function only as this unsafe fn. jni 0.22.4 does not
    // offer it. The function needs a valid JNIEnv and a live reference to an open android.hardware.HardwareBuffer.
    // For such a reference, AOSP returns a non-null buffer, which `from_jni` assumes. The JNIEnv pointer comes from the
    // `Env` of the calling thread. `encode_ycbcr` calls this function first. The only JNI call before `from_jni` is
    // `isClosed`, which leaves no exception pending when it succeeds. The Kotlin declaration of `encodeYcbcr` types
    // `buffer` as a non-null HardwareBuffer, so the JVM passes a live reference to an instance of that class. The
    // check above confirms that it is not closed. The activity closes the HardwareBuffer only after this call returns.
    // `acquire` adds a reference to the native buffer before this function returns, so the returned buffer stays valid
    // after the activity closes the HardwareBuffer.
    let buffer = unsafe { HardwareBuffer::from_jni(env.get_raw().cast(), buffer.as_raw().cast()) };
    Ok(buffer.acquire())
}
