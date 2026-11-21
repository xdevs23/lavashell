# Lavashell

## What is Lavashell?

Lavashell is a UI framework built in Rust for desktop shells and the apps that run in them: panels, taskbars, on-screen keyboards, lock screens and windows.

## What it supports

* **Linux**: Wayland compositors, with windows, layer-shell panels, lock screens, clipboard and drag and drop
* **AOSP**: apps that draw into a surface handed over by the host
* **Desktop settings**: dark mode, reduced motion and contrast follow the settings portal
* **Input**: pointer, touch, keyboard and the built-in on-screen keyboard
* **Accessibility**: screen readers on Linux read the views through AT-SPI

## Example

This example opens a window with a counter and a button that adds one:

```rust
use lavashell::prelude::*;

fn counter(ui: &mut Ui) {
    let count = ui.state("count", || 0_u32);
    ui.column(Modifier::new().gap(16.dp()).padding(32.dp()), |ui| {
        ui.text(count.get().to_string(), Modifier::new().font_size(48.sp()));
        ui.button(Modifier::new().padding(12.dp()), count.action(|n| *n += 1), |ui| {
            ui.text("Add one", Modifier::new());
        });
    });
}

fn open_counter(facts: PlatformFacts) -> Result<(), CounterError> {
    let mut app = App::new(facts);
    app.window(Window::new("Counter", "org.example.counter"), counter)?;
    Ok(lavashell::run(app)?)
}

#[derive(Debug, thiserror::Error)]
enum CounterError {
    #[error(transparent)]
    Setup(#[from] lavashell::SetupError),
    #[error(transparent)]
    Run(#[from] lavashell::RunError),
}
```

## License

Lavashell is free and open source under the GNU General Public License, version 3 only ([`COPYING`](COPYING)).
