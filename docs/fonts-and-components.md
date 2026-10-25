# Fonts and components

## Fonts

```rust,ignore
let fonts = Fonts::new()
    .file("body", "assets/body.ttf")?
    .embedded("display", include_bytes!("assets/display.ttf").as_slice())?
    .system("mono", ["Iosevka", "monospace"])?;
let app = App::new(platform).fonts(fonts);
```

```rust,ignore
fn screen(ui: &mut Ui, model: &State<Model>) {
    ui.provide(&TEXT_STYLE, theme::body(), |ui| {
        ui.provide(&INTERACTION_STYLE, theme::interaction(), |ui| {
            ui.column(Modifier::new().gap(16.dp()), |ui| {
                let heading = Modifier::new().text_style(theme::heading(28.sp()));
                ui.text(model.get().title, heading);
                let query = model.handler(|m, query| m.query = query);
                let field = TextFieldOptions::default();
                ui.text_field(model.get().query, field, theme::field(), query);
            });
        });
    });
}
```

The examples load Fontsource packages from `LAVASHELL_EXAMPLE_NODE_MODULES`. `nix develop` and `nix run` set that
variable. Without Nix:

```sh
npm ci --prefix examples --ignore-scripts --no-audit --no-fund
export LAVASHELL_EXAMPLE_NODE_MODULES=$PWD/examples/node_modules
```

## Components

```rust,ignore
ui.button(theme::primary_action(), model.action(Model::export), |ui| {
    ui.row(Modifier::new().gap(8.dp()).align_items(Align::Center), |ui| {
        ui.icon(icons.upload, IconOptions::default(), Modifier::new());
        ui.text(copy("export"), Modifier::new());
    });
});

ui.toggle(muted.get(), Modifier::new(), muted.handler(|m, value| *m = value), |ui, on| {
    ui.text(if on { "Muted" } else { "Sound" }, Modifier::new());
});

let level = volume.clone();
let change = volume.handler(|v, x| *v = x);
ui.slider(move || level.get(), Some(0.1), Modifier::new().width(240.dp()), change, |ui, value| {
    track(ui, value);
});

let edit = draft.handler(|d, text| *d = text);
ui.multiline_field(draft.get(), TextFieldOptions::default(), theme::field(), edit);
```

`.focus_within(border)` outlines a container while a descendant has visible focus.

## On-screen keyboard

```rust,ignore
let keyboard = Keyboard::new(Target::View, &settings, layout)?;
let shown = keyboard.clone();
let view = View::new(platform, move |ui| {
    ui.column(Modifier::new(), |ui| {
        ui.stack(Modifier::new().weight(1.0), app);
        ui.keyboard(&shown, Modifier::new());
    });
});
```

`ui.keyboard_from(&keyboard, origin, closed, modifier)` expands the panel from the node composed with
`ui.shared(origin, ..)`. `Target::Host { session, sink }` types into a field in another view or process. See the
[keyboard example](../examples/keyboard/README.md).
