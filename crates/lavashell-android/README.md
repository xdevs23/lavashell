# AOSP host integration

Requires AOSP 12 (API 31) or later.

## Render into a Surface

```rust,ignore
use lavashell::android::{AndroidRenderer, NativeWindow, window_of_surface};
use lavashell::prelude::FrameTime;
use lavashell::renderer::{Fonts, FrameOutcome, PresentationPolicy, SurfaceAlpha, SurfaceTiming, skia};

// Create, render and drop the renderer on one thread.
let mut renderer = AndroidRenderer::new(&Fonts::new())?;
// This Surface has never received an explicit timestamp.
let timing = SurfaceTiming::new(PresentationPolicy::Automatic);
let window = NativeWindow::new(window_of_surface(env, &surface)?, timing);
renderer.attach(window, SurfaceAlpha::Opaque)?;

// In the frame callback of the host:
let failures = view.show_frame_at(&mut tree, Viewport::new(size, density), renderer.painter(), frame)?;
for failure in failures {
    log::error!("{}", lavashell::core::error::message_chain(failure));
}
let result = renderer.render(tree.at(frame.presentation), density, skia::Color::BLACK)?;
match result {
    FrameOutcome::Shown => view.record_presentation(frame.presentation),
    FrameOutcome::Pending | FrameOutcome::Stale => { /* Request another frame from the host. */ }
}

// Before surfaceDestroyed returns:
let window = renderer.detach();
# Ok::<(), lavashell::android::Error>(())
```

## Explicit timestamps

```rust,ignore
if renderer.supports_presentation_time() {
    renderer.render_at(tree.at(frame.presentation), density, skia::Color::BLACK, timestamp_ns)?;
}
```

After you use an explicit timestamp, every later frame sent to that Surface needs one too. Use the same
`SurfaceTiming` every time you acquire that Surface. Use `PresentationPolicy::Explicit` for an encoder input or a
Surface with an unknown history.

## Input

| `MotionEvent` | `InputEvent` |
| --- | --- |
| `ACTION_DOWN`, `ACTION_POINTER_DOWN` | `TouchDown` |
| `ACTION_MOVE`, historical positions first | `TouchMove` for each pointer |
| `ACTION_UP`, `ACTION_POINTER_UP` | `TouchUp` |
| `ACTION_CANCEL` | `TouchCancel` for each touch, `CancelPointer` for the primary pointer |

| Tool type | `PointerKind` |
| --- | --- |
| `TOOL_TYPE_STYLUS`, `TOOL_TYPE_ERASER` | `Pen` |
| `TOOL_TYPE_MOUSE` | `Mouse` |
| `TOOL_TYPE_FINGER`, `TOOL_TYPE_UNKNOWN` | `Finger` |

## Frames from another app

```rust,ignore
use lavashell::android::{Latch, surface_of_window};
use lavashell::prelude::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

let sizes = BufferSizes {
    default: PixelSize::new(1770, 960),
    max: PixelSize::new(2124, 1152),
};
let mut stream = SurfaceStream::new(sizes, StreamFormat::Rgba8888)?;
let arrived = Arc::new(AtomicBool::new(false));
let wake = arrived.clone();
stream.on_frame(move || wake.store(true, Ordering::Release));
let surface = surface_of_window(env, stream.native_window())?;
let mut latest: Option<StreamImage> = None;

// On the rendering thread, before the View is laid out:
if arrived.swap(false, Ordering::Acquire) || stream.waits_for_producer() {
    if let Latch::Frame(image) = renderer.latch(&mut stream)? {
        latest = Some(image);
    }
}

// In the canvas of the pane:
if let Some(image) = &latest {
    drawing.stream_image(image, pane, ImageFit::contain());
}
# Ok::<(), lavashell::android::Error>(())
```

## Native build

* Link the shared library with 16 KiB page alignment.
* The AOSP build of Skia does not decode WOFF2. Supply TTF data.
* You accept the Android Software Development Kit License Agreement when you build the AOSP packages in the flake.

```sh
nix develop .#android
```
