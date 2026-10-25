# Accessibility

The Wayland host exposes semantics for each view to AT-SPI through AccessKit.

## Names and states

```rust
ui.icon_button(
    icons.pause,
    IconOptions::default(),
    Modifier::new().accessible_name("Pause").selected(playing),
    pause,
);
```

`tag()` sets a node identifier for lookups such as `View::bounds_of`. Assistive technology never reads the identifier.

## Dynamic semantics and actions

```rust
let modifier = Modifier::new()
    .semantics_dynamic(move |semantics| semantics.description = Some(format!("{}%", progress.get())))
    .on_accessibility_action(ActionKind::Increment, move |_| {
        volume.update(|v| *v += 1);
        Ok(())
    });
```

## Focus styles

```rust
fn slot(ui: &mut Ui, styles: &StateStyles, play: &'static Icon, playing: bool) {
    let modifier = Modifier::new().state_styles(styles.clone()).selected(playing);
    ui.icon_button(play, IconOptions::default(), modifier, || {});
}
```

As with `:focus-visible`, a key press sets `focus.visible`, while a pointer or touch press clears it. Text fields
show focus after any input.

## Preferences

```rust
let overrides = PreferenceOverrides {
    text_scaling: TextScaling::linear(1.25),
    reduced_transparency: Some(true),
    ..PreferenceOverrides::default()
};
let app = App::new(platform).preference_overrides(overrides);

let reduced = ui.local(&REDUCED_MOTION);
```

The Linux host reads contrast and reduced motion from the desktop portal. Text scale and reduced transparency come
from the app.

## Tests

```sh
nix develop .#native --command cargo test -p lavashell-wayland --lib -- --nocapture --test-threads=1
```
