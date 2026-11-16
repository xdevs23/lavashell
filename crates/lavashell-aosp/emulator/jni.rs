// SPDX-License-Identifier: GPL-3.0-only

use crate::error::NativeMethodError;
use crate::round_trips::with_round_trips;
use crate::stream_round_trip::StreamRoundTrip;
use crate::ycbcr::Conversion;
use ::jni::errors::ThrowRuntimeExAndDefault;
use ::jni::objects::{JIntArray, JLongArray, JObject, JPrimitiveArray, TypeArray};
use ::jni::sys::{jfloat, jint, jlong};
use ::jni::{Env, EnvUnowned, JValue, jni_mangle, jni_sig, jni_str};
use lavashell_aosp::{BufferSizes, StreamFormat, surface_of_window};
use lavashell_core::error::message_chain;
use lavashell_core::prelude::PixelSize;
use lavashell_renderer::{ForeignFormat, skia};
use ndk::hardware_buffer::{HardwareBuffer, HardwareBufferRef};
use ndk::media::image_reader::ImageFormat;

macro_rules! native_methods {
    ($($method:item)*) => {
        $(#[jni_mangle("org.lavashell.surfacestream.StreamRoundTrip")] $method)*
    };
}

native_methods! {
    pub fn create<'local>(
        env: EnvUnowned<'local>,
        _: JObject<'local>,
        width: jint,
        height: jint,
        max_width: jint,
        max_height: jint,
        format: jint,
    ) -> jlong {
        export(env, |_| {
            let sizes = to_buffer_sizes([width, height], [max_width, max_height])?;
            let round_trip = StreamRoundTrip::new(sizes, to_stream_format(format)?)?;
            with_round_trips(|round_trips| round_trips.insert(round_trip))
        })
    }

    pub fn create_surface<'local>(
        env: EnvUnowned<'local>,
        _: JObject<'local>,
        handle: jlong,
    ) -> JObject<'local> {
        export(env, |env| {
            let window = with_round_trip(handle, |round_trip| round_trip.producer_window())?;
            surface_of_window(env, &window).map_err(NativeMethodError::Surface)
        })
    }

    pub fn latch<'local>(env: EnvUnowned<'local>, _: JObject<'local>, handle: jlong) {
        export(env, |_| with_round_trip(handle, StreamRoundTrip::latch));
    }

    pub fn await_wake<'local>(env: EnvUnowned<'local>, _: JObject<'local>, handle: jlong) {
        export(env, |_| {
            with_round_trip(handle, StreamRoundTrip::await_wake)
        });
    }

    pub fn render<'local>(env: EnvUnowned<'local>, _: JObject<'local>, handle: jlong) -> jint {
        export(env, |_| {
            with_round_trip(handle, StreamRoundTrip::render).map(to_color_int)
        })
    }

    pub fn render_grouped<'local>(
        env: EnvUnowned<'local>,
        _: JObject<'local>,
        handle: jlong,
        opacity: jfloat,
    ) -> JIntArray<'local> {
        export(env, |env| {
            let pixels = with_round_trip(handle, |round_trip| {
                round_trip.render_grouped(f64::from(opacity))
            })?;
            to_jni_array(env, &pixels.map(to_color_int))
        })
    }

    pub fn read_drawn_size<'local>(
        env: EnvUnowned<'local>,
        _: JObject<'local>,
        handle: jlong,
    ) -> JLongArray<'local> {
        export(env, |env| {
            let Some(size) = with_round_trip(handle, |round_trip| round_trip.drawn_size())? else {
                return Ok(JLongArray::default());
            };
            to_jni_array(env, &[size.width.into(), size.height.into()])
        })
    }

    pub fn count_wakes<'local>(env: EnvUnowned<'local>, _: JObject<'local>, handle: jlong) -> jlong {
        export(env, |_| {
            with_round_trip(handle, |round_trip| Ok(round_trip.wakes().into()))
        })
    }

    pub fn resize<'local>(
        env: EnvUnowned<'local>,
        _: JObject<'local>,
        handle: jlong,
        width: jint,
        height: jint,
        max_width: jint,
        max_height: jint,
    ) {
        export(env, |_| {
            let sizes = to_buffer_sizes([width, height], [max_width, max_height])?;
            with_round_trip(handle, |round_trip| round_trip.resize(sizes))
        });
    }

    pub fn destroy<'local>(env: EnvUnowned<'local>, _: JObject<'local>, handle: jlong) -> jlong {
        export(env, |_| {
            let round_trip = with_round_trips(|round_trips| round_trips.take(handle))?;
            Ok(round_trip.close()?.into())
        })
    }

    pub fn encode_ycbcr<'local>(
        env: EnvUnowned<'local>,
        _: JObject<'local>,
        handle: jlong,
        buffer: JObject<'local>,
        color: jint,
    ) -> JObject<'local> {
        export(env, |env| {
            let buffer = acquire_hardware_buffer(env, &buffer)?;
            let format =
                with_round_trip(handle, |round_trip| round_trip.read_buffer_format(buffer))?;
            let model = format.model.try_into();
            let range = format.range.try_into();
            let conversion = Conversion::new(
                model.map_err(NativeMethodError::YcbcrConversion)?,
                range.map_err(NativeMethodError::YcbcrConversion)?,
            );
            let [_, red, green, blue] = color.to_be_bytes();
            let ycbcr =
                (conversion.encode([red, green, blue])).ok_or(NativeMethodError::EncodeColor)?;
            let tolerance = (conversion.tolerance()).ok_or(NativeMethodError::EncodeColor)?;
            new_conversion_object(env, ycbcr, tolerance, &format)
        })
    }
}

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
    if env.exception_check() {
        return Ok(());
    }
    match env.throw(message_chain(failure)) {
        Ok(()) | Err(::jni::errors::Error::JavaException) => Ok(()),
        Err(error) => Err(error),
    }
}

fn with_round_trip<T>(
    handle: jlong,
    work: impl FnOnce(&mut StreamRoundTrip) -> Result<T, NativeMethodError>,
) -> Result<T, NativeMethodError> {
    with_round_trips(|round_trips| work(round_trips.get_mut(handle)?))
}

fn to_buffer_sizes(
    [width, height]: [jint; 2],
    [max_width, max_height]: [jint; 2],
) -> Result<BufferSizes, NativeMethodError> {
    let size = |width, height| Ok(PixelSize::new(to_length(width)?, to_length(height)?));
    BufferSizes::new(size(width, height)?, size(max_width, max_height)?)
        .map_err(NativeMethodError::BufferSizes)
}

fn to_length(pixels: jint) -> Result<u32, NativeMethodError> {
    u32::try_from(pixels).map_err(NativeMethodError::NegativeLength)
}

fn to_stream_format(id: jint) -> Result<StreamFormat, NativeMethodError> {
    match ImageFormat::from(id) {
        ImageFormat::PRIVATE => Ok(StreamFormat::Private),
        ImageFormat::RGBA_8888 => Ok(StreamFormat::Rgba8888),
        _ => Err(NativeMethodError::UnknownFormat(id)),
    }
}

fn to_color_int(color: skia::Color) -> jint {
    jint::from_be_bytes([color.a(), color.r(), color.g(), color.b()])
}

fn to_jni_array<'local, T: TypeArray>(
    env: &mut Env<'local>,
    values: &[T],
) -> Result<JPrimitiveArray<'local, T>, NativeMethodError> {
    let array = JPrimitiveArray::<T>::new(env, values.len())
        .map_err(NativeMethodError::ReturnToActivity)?;
    array
        .set_region(env, 0, values)
        .map_err(NativeMethodError::ReturnToActivity)?;
    Ok(array)
}

fn new_conversion_object<'local>(
    env: &mut Env<'local>,
    codes: [u8; 3],
    tolerance: u8,
    format: &ForeignFormat,
) -> Result<JObject<'local>, NativeMethodError> {
    let codes = to_jni_array(env, &codes.map(jint::from))?;
    let description =
        (env.new_string(format!("{format:?}"))).map_err(NativeMethodError::ReturnToActivity)?;
    let arguments = [
        JValue::Object(&codes),
        JValue::Int(tolerance.into()),
        JValue::Object(&description),
    ];
    let class = jni_str!("org/lavashell/surfacestream/YcbcrConversion");
    (env.new_object(class, jni_sig!("([IILjava/lang/String;)V"), &arguments))
        .map_err(NativeMethodError::ReturnToActivity)
}

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
    // SAFETY: Native code can get the AHardwareBuffer of a HardwareBuffer object only through
    // `AHardwareBuffer_fromHardwareBuffer`. ndk 0.9.0 offers that function only as this unsafe fn. jni 0.22.4 does not
    // offer it. The function needs a valid JNIEnv and a live reference to an open HardwareBuffer.
    // For such a reference, AOSP returns a non-null buffer, which `from_jni` assumes. The JNIEnv pointer comes from the
    // `Env` of the calling thread. `encode_ycbcr` calls this function first. The only JNI call before `from_jni` is
    // `isClosed`, which leaves no exception pending when it succeeds. The declaration of `encodeYcbcr` types
    // `buffer` as a non-null HardwareBuffer, so the caller passes a live reference to an instance of that class. The
    // check above confirms that it is not closed. The activity closes the HardwareBuffer only after this call returns.
    // `acquire` adds a reference to the native buffer before this function returns, so the returned buffer stays valid
    // after the activity closes the HardwareBuffer.
    let buffer = unsafe { HardwareBuffer::from_jni(env.get_raw().cast(), buffer.as_raw().cast()) };
    Ok(buffer.acquire())
}
