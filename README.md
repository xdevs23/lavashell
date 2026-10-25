# Lavashell

A Rust UI toolkit for Wayland and AOSP that draws with Skia through Vulkan.

## Features

* Components with ordered modifiers and headless controls without a built-in look
* Reactive state, values passed down the view tree and scoped recomposition
* Springs, tweens, keyframes, shared transitions and contour morphing, timed to the presented frame
* OKLCH colors and editable materials with glass, backdrop and shadow effects
* Retained rendering that repaints damaged regions and caches copies of opaque packets
* Text editing, clipboard and drag-and-drop transfers
* Accessibility semantics through AT-SPI on Linux, text scaling and reduced motion
* Window shapes
* AOSP embedding, including drawing into the input Surface of a video encoder
* Composition over images from an independent Vulkan scene

## Quick start

```sh
nix run .#counter
nix develop
nix develop .#validation
nix flake check
nix develop .#native --command python3 tools/nix-run-native.py
```

See the [examples](examples/README.md) and the [documentation](docs/).

## Using Lavashell

```toml
[dependencies]
lavashell = { git = "https://github.com/xdevs23/lavashell" }

# Copy the revision from the Cargo.toml of Lavashell.
[patch.crates-io.skia-safe]
git = "https://github.com/xquant23/rust-skia_rust-skia"
rev = "..."

[patch.crates-io.skia-bindings]
git = "https://github.com/xquant23/rust-skia_rust-skia"
rev = "..."
```

```rust
use lavashell::prelude::*;
use std::time::Duration;

fn counter(ui: &mut Ui) {
    ui.key("counter", Modifier::new(), |ui| {
        let count = ui.state("count", || 0_u32);
        ui.column(Modifier::new().gap(24.dp()), |ui| {
            ui.text(count.get().to_string(), Modifier::new());
            ui.button(Modifier::new().padding(12.dp()), count.action(|n| *n += 1), |ui| {
                ui.text("Add", Modifier::new());
            });
        });
    });
}

fn platform() -> PlatformFacts {
    PlatformFacts {
        touch_slop: None,
        caret_width: None,
        long_press: Duration::from_millis(300),
        double_tap: Duration::from_millis(300),
        key_repeat: None,
        text_size: 14.sp(),
        foreground: Color::WHITE,
        color_scheme: ColorScheme::Dark,
        password_mask: '•',
        keyboard: KeyboardPreferences { automatic_capitals: true, key_preview: true },
        locales: Vec::new(),
    }
}

fn main() -> Result<(), lavashell::wayland::Error> {
    let window = Window::new("Counter", "org.example.Counter");
    App::new(platform()).run(|app| app.window(window, counter))
}
```

## License

Copyright (C) 2026 Simão Gomes Viana

Lavashell is licensed under GPL-3.0-only; see [COPYING](COPYING).
