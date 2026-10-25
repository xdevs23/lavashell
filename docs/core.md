# Core API

```rust
use lavashell::prelude::*;

fn counter(ui: &mut Ui, plus: &'static Icon) {
    ui.key("counter", Modifier::new(), |ui| {
        let count = ui.state("count", || 0_u32);
        ui.column(Modifier::new().gap(24.dp()), |ui| {
            ui.text(count.get().to_string(), Modifier::new());
            ui.icon_button(
                plus,
                IconOptions::default(),
                Modifier::new().padding(12.dp()),
                count.action(|n| *n += 1),
            );
        });
    });
}

#[derive(Debug, thiserror::Error)]
enum Error {
    #[error("cannot decode an icon")]
    Icon(#[from] lavashell_lucide::Error),
    #[error("the app failed")]
    App(#[from] lavashell::wayland::Error),
}

fn main() -> Result<(), Error> {
    let plus = lavashell_lucide::PLUS.icon()?;
    let window = Window::new("Counter", "org.example.Counter");
    Ok(App::new(platform()).run(|app| app.window(window, move |ui| counter(ui, plus)))?)
}
```

`platform()` returns `PlatformFacts` for the app. The Wayland host uses the key repeat, color scheme and languages
from the desktop.

## Units

| Unit | Meaning |
| --- | --- |
| `12.dp()` | Density-independent pixels |
| `14.sp()` | A font size before text scaling |
| `0.01.em()` | A fraction of the font size |
| `10.vw()`, `10.vh()` | Percent of the viewport width or height |
| `50.pct()` | Percent of a box |

## Modifiers

```rust
let modifier = Modifier::new()
    .background(Color::oklch(0.3, 0.02, 80.0)) // covers the padding
    .padding(12.dp())
    .background(Color::oklch(0.5, 0.02, 80.0)) // covers the content
    .enabled(false);
```

## State

```rust
let count = ui.state("count", || 0_u32);
ui.scoped("label", Modifier::new(), move |ui| {
    ui.text(count.get().to_string(), Modifier::new());
});
```

Changing `count` recomposes only the scoped subtree.

## Values of the view tree

```rust
static ACCENT: Local<Color> = Local::new(|| Color::WHITE);

ui.provide(&ACCENT, Color::oklch(0.62, 0.2, 300.0), |ui| {
    let accent = ui.local(&ACCENT);
    ui.text("Accent", Modifier::new().color(accent));
});
```

## Downstream Nix builds

```nix
let
  skia = lavashell.lib.skia {
    inherit pkgs;
    cargoLock = ./Cargo.lock;
    workspaces = [ ./. ];
  };
in
pkgs.rustPlatform.buildRustPackage {
  # ...
  cargoLock = {
    lockFile = ./Cargo.lock;
    outputHashes = {
      "skia-safe-${lavashell.lib.skiaBindings.version}" = lavashell.lib.skiaBindings.hash;
      "skia-bindings-${lavashell.lib.skiaBindings.version}" = lavashell.lib.skiaBindings.hash;
    };
  };
  buildInputs = [ skia.linux ];
  LAVASHELL_LUCIDE = lavashell.packages.x86_64-linux.lucide-static;
  LAVASHELL_HYPRLAND_PROTOCOLS = pkgs.hyprland-protocols;
}
```

`skia.android { ndk; api; }` returns the builds for `aarch64-linux-android` and `x86_64-linux-android`. A shell that
builds several targets sources the `activate` file of each build before it runs Cargo for that target.
