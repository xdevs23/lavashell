# Animation

Lavashell samples motion at the predicted presentation time of each frame.

## Animated values

```rust
use lavashell::prelude::*;
use std::time::Duration;

fn screen(ui: &mut Ui, plus: &'static Icon) {
    let expanded = ui.state("expanded", || false);
    let destination = Point::new(if expanded.get() { 180.dp() } else { 0.dp() }, 0.dp());
    let spring = spring!(Duration::from_millis(450), 0.85);
    let offset = match ui.animate("offset", destination, spring) {
        Ok(offset) => offset,
        Err(error) => {
            ui.fail(error);
            return;
        }
    };
    ui.icon_button(
        plus,
        IconOptions::default(),
        Modifier::new().animated_offset(offset),
        expanded.action(|value| *value = !*value),
    );
}
```

`animated_offset`, `animated_opacity` and `animated_background` repaint without layout.
`Modifier::new().height(&animation)` lays out again on each frame without composing.

## Transitions

```rust
fn card(ui: &mut Ui, expanded: bool) {
    let modifier = Modifier::new()
        .clip(if expanded { 24.dp() } else { 12.dp() })
        .transition(spring!(Duration::from_millis(450), 0.85));
    ui.shared("card", modifier, |ui| {
        ui.spacer(if expanded { 320.dp() } else { 140.dp() }, 120.dp(), Modifier::new());
    });
}
```

```rust
fn segment(ui: &mut Ui, title: &str) {
    let transition = Transition::new(spring!(Duration::from_millis(450), 0.85))
        .enter(
            Presence::fade()
                .slide(constant!(FinitePoint::try_new(Point::new(Dp::ZERO, Dp::new(12.0)))))
                .scale(constant!(ScaleFactor::try_new(0.9))),
        )
        .exit(Presence::fade());
    ui.key(title, Modifier::new().transition(transition), |ui| {
        ui.text(title, Modifier::new());
    });
}

fn island(ui: &mut Ui, segments: &[&'static str], glass: Material) {
    let modifier = Modifier::new()
        .material(glass, 26.dp())
        .clip(26.dp())
        .transition(Transition::new(spring!(Duration::from_millis(450), 0.85)).reflow());
    ui.layout(Axis::Horizontal, modifier, |ui| {
        for &title in segments {
            segment(ui, title);
        }
    });
}
```

## Keyframes

```rust
use lavashell::core::animation::KeyframesError;
use std::time::Duration;

fn pulse(track: &Animation<f64>) -> Result<(), KeyframesError> {
    let frames = Keyframes::new([
        Keyframe::new(Duration::ZERO, 0.0),
        Keyframe::new(Duration::from_millis(450), 1.0),
        Keyframe::new(Duration::from_millis(1200), 0.25),
    ])?;
    track.play(frames, Playback {
        repeat: Repeat::Forever,
        direction: Direction::Alternate,
    });
    Ok(())
}
```

## Morphing

```rust
let circle = Contour::of(&Shape::rounded(bounds, 48.dp()), device_pixel, 96)?;
let star = Contour::resample(&star_points, true, 96)?.aligned_to(&circle)?;
let morph = ui.motion().animation(circle)?;
morph.animate_to(star, spring)?;
```

## Timers

```rust
let timer = ui.motion().timer(move || ticks.update(|n| *n += 1));
timer.repeat(Duration::from_millis(500), Duration::from_millis(250));
```

## Embedded hosts

```rust
view.show_frame_at(&mut tree, Viewport::new(size, density), measurer, frame_time)?;
// Paint `tree.at(frame_time.presentation)`, then:
view.record_presentation(shown_at);
```

Wake the host at `View::next_deadline` and `SceneTree::next_change`. See [frame timing](frame-timing.md).
