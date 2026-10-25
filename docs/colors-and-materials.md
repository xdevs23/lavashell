# Colors and materials

## Colors

```rust
use lavashell::prelude::*;
use lavashell::materials::SurfaceTreatment;

const SCALE: TonalPalette = TonalPalette::new(80.0, 0.02);
const BACKGROUND: Color = SCALE.tone(0.17);
const TEXT: Color = SCALE.tone(0.95);

fn panel(ui: &mut Ui) {
    let tint = Color::oklch(0.93, 0.035, 80.0);
    let glass = SurfaceTreatment::LiquidGlass.material(tint, Size::new(355.dp(), 225.dp()));
    ui.column(Modifier::new().material(glass, 28.dp()).padding(28.dp()), |ui| {
        ui.text("42", Modifier::new().color(TEXT));
    });
}
```

```rust
let safe = Color::oklch(0.7, 0.3, 145.0).mapped(Gamut::Srgb);
let mix = BACKGROUND.mix_in(TEXT, 0.5, ColorInterpolation::Oklab);
```

Colors interpolate in premultiplied OKLCH along the shorter hue path unless a `ColorInterpolation` is given.
Composition runs in linear Rec.2020.

## Effects

```rust
let corners = 20.dp();
let decoration = Modifier::new()
    .shadow(constant!(Shadow::new(Color::oklch(0.1, 0.01, 80.0).with_alpha(0.3),
        Dp::new(12.0), Point::new(Dp::ZERO, Dp::new(8.0)))), corners)
    .backdrop(constant!(Backdrop::blur(Dp::new(10.0))), corners)
    .clip(corners)
    .background(Color::oklch(0.95, 0.02, 80.0).with_alpha(0.15))
    .padding(24.dp());
```

## Shapes

```rust
let card = RoundedRect::new(bounds, Corners::all(24.dp()));
drawing.material(card.minus(hole), &glass);

let spacing = constant!(NonNegativeDp::try_new(Dp::new(16.0)));
if let Some(merged) = Shape::union([left.into(), right.into()], spacing) {
    drawing.material(merged, &glass);
}
```

## Material groups

```rust
let glass = SurfaceTreatment::LiquidGlass.material(tint, size);
ui.row(Modifier::new().gap(8.dp()).material_group(glass, 24.dp()), |ui| {
    for (label, width) in [("Navigation", 120.dp()), ("Music", 260.dp())] {
        let island = Modifier::new()
            .width(width)
            .island(20.dp())
            .transition(Transition::new(spring).bounds_only());
        ui.text(label, island);
    }
});
```

Glass on a contour or a union that needs textures renders without the lens inside an opacity group, a content filter,
encoded-sRGB blending, a rotation or a skew.

## Treatment parameters

```rust
use lavashell::materials::{SurfaceTreatment, TreatmentParameters};

let parameters = TreatmentParameters {
    blur: Some(constant!(NonNegativeDp::try_new(Dp::new(24.0)))),
    ..SurfaceTreatment::FrostedGlass.parameters()
};
let frosted = SurfaceTreatment::FrostedGlass.material_with(tint, size, parameters);
```

## Interaction states

```rust,ignore
const ACCENT: Color = Color::oklch(0.62, 0.2, 300.0);
const TINT: Color = Color::oklch(0.93, 0.02, 80.0);
const INK: Color = Color::oklch(0.25, 0.02, 260.0);
const RING_WIDTH: NonNegativeDp = constant!(NonNegativeDp::try_new(Dp::new(2.0)));
const RING: BorderStyle = BorderStyle { color: ACCENT, width: RING_WIDTH, radius: Dp::new(24.0) };
const PRESSED: ScaleFactor = constant!(ScaleFactor::try_new(0.96));

fn slot(state: InteractionState) -> InteractionAppearance {
    let tint = if state.selected { ACCENT } else { TINT };
    let glass = SurfaceTreatment::LiquidGlass.material(tint, Size::new(96.dp(), 48.dp()));
    InteractionAppearance::default()
        .material(glass, 24.dp())
        .content(if state.selected { Color::WHITE } else { INK })
        .scale(if state.pointer.active { PRESSED } else { ScaleFactor::ONE })
        .outline(if state.focus.visible { RING } else { BorderStyle::NONE })
}

fn app_slot(ui: &mut Ui, play: &'static Icon, focused: bool) {
    let styles = StateStyles::new(slot).motion(spring!(Duration::from_millis(450), 0.85));
    let modifier = Modifier::new().selected(focused).state_styles(styles);
    ui.icon_button(play, IconOptions::default(), modifier, || {});
}
```

## Images

```rust
drawing.image(&wallpaper, screen, ImageFit::Cover(
    ImagePlacement::at(Anchor::Top).offset(0.vw(), 10.vh()),
));
```

## Output

```rust
App::new(platform)
    .output_observer(|surface, actual| eprintln!("{surface}: {actual:?}"))
    .run(|app| app.window(Window::new("Panel", "org.example.Panel")
        .output(OutputRequest::new(OutputProfile::Hdr10, Precision::Ten)), panel))?;
```

| Profile | Primaries / encoding | Precision |
| --- | --- | --- |
| `Srgb` | sRGB / sRGB transfer | `Eight`, `Ten` |
| `DisplayP3` | Display P3 / sRGB transfer | `Ten` |
| `Rec2020` | Rec.2020 / linear | `Float16` |
| `Hdr10` | Rec.2020 / ST 2084 PQ | `Ten` |
| `ScRgb` | sRGB / linear extended range | `Float16` |

If the requested output is unavailable, Lavashell falls back to 8-bit sRGB and reports `fallback: true`. A working value
of 1.0 is 203 cd/m².

`Window::transparent(true)` composes with alpha. `.background_blur(true)` requests `ext-background-effect-v1` from the
compositor.
