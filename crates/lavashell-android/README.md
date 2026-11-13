# AOSP host integration

Requires AOSP 12 (API 31) or later.

## Run an app on a Surface

```rust,ignore
use lavashell::android::{
    AospHost, NativeWindow, configuration_of, display_of, motion_event_of, window_of_surface,
};
use lavashell::prelude::Platform;
use lavashell::renderer::{PresentationPolicy, SurfaceTiming};

// Before the Surface exists. The app is built on the run thread.
let host = AospHost::new(display_of(env, &activity)?, configuration_of(env, &activity)?);
let served = host.clone();
let run = std::thread::spawn(move || served.serve(|| lavashell::run::<Platform>(build_app())));

// In onDisplayChanged of a DisplayListener registered for the display that shows the activity:
host.display_changed(display_of(env, &activity)?);

// In onConfigurationChanged:
host.configuration_changed(configuration_of(env, &activity)?);

// In surfaceCreated. No explicit presentation timestamp was ever set for this Surface.
let timing = SurfaceTiming::new(PresentationPolicy::Automatic);
host.surface_created(NativeWindow::new(window_of_surface(env, &surface)?, timing));

// In surfaceChanged:
host.surface_changed();

// In every frame callback of the Choreographer. The host app posts the callback again in each frame until the run ends.
host.frame();

// In onTouchEvent and onGenericMotionEvent of the view that shows the Surface. Both return true for each event passed
// to the host.
if let Some(event) = motion_event_of(env, &event)? {
    host.motion(event);
}

// In surfaceDestroyed. The call returns after the renderer has retired the Surface. Retiring it closes the window and
// ends the run.
host.surface_destroyed();
let ended = run.join();
# Ok::<(), lavashell::android::Error>(())
```

Use the same `SurfaceTiming` every time you acquire a Surface. The runtime draws with automatic presentation timing
only.

## Frames from another app

```rust,ignore
use lavashell::android::{BufferSizes, StreamFormat, SurfaceStream, surface_of_window};
use lavashell::prelude::*;
use lavashell::renderer::Latch;
use std::sync::mpsc;

// In `build_app`. Each frame shows the newest image sent through `latched`.
let (latched, images) = mpsc::channel();
let shown = State::new(None);
let showing = shown.clone();
let app = app.on_frame(move |_| {
    if let Some(image) = images.try_iter().last() {
        showing.set(Some(image));
    }
});

// In the canvas of the pane:
if let Some(image) = shown.get() {
    drawing.stream_image(&image, pane, ImageFit::contain());
}

// The stream is created on the renderer thread. It sends on `arrived` when a frame arrives.
let (arrived, arrivals) = mpsc::channel();
let sizes = BufferSizes {
    default: PixelSize::new(1770, 960),
    max: PixelSize::new(2124, 1152),
};
let attached = host.run_on_streams(move |streams, _| {
    let mut stream = SurfaceStream::new(sizes, StreamFormat::Rgba8888)?;
    stream.on_frame(move || {
        let _ended_app_reads_nothing = arrived.send(());
    });
    let window = stream.native_window().clone();
    Ok::<_, lavashell::android::Error>((streams.attach(stream), window))
})?;
let (stream, window) = attached?;
let surface = surface_of_window(env, &window)?;

// After `arrivals` received, and again while the producer has not yet written the latched frame:
let latch = host.run_on_streams(move |streams, painter| streams.latch(painter, stream))?;
if let Latch::Frame(image) = latch? {
    latched.send(image)?;
}
```

Each `?` converts `RunEnded`, `lavashell::android::Error` or `SendError` into the error type of the function around it.

## Native build

* Link the shared library with 16 KiB page alignment.
* The AOSP build of Skia does not decode WOFF2. Supply TTF data.
* Building the AOSP packages in the flake accepts the Android Software Development Kit License Agreement.

```sh
nix develop .#android
```
