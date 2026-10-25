# Retained rendering

## Scoped recomposition

```rust
let model = ui.state("model", Model::default);
let title = ui.state("title", || model.select(|m| m.title.clone()));
ui.scoped("header", Modifier::new(), move |ui| {
    ui.text(title.get(), Modifier::new());
});
```

Changing `model` does not recompose the header unless `title` changes.

## Effect quality

```rust
use lavashell::effect_quality::{EffectDraw, FrameBudget, FramePlan, Planned, QualityChooser};

struct Lowest;

impl QualityChooser for Lowest {
    fn choose(&mut self, _: FrameBudget, draws: &[EffectDraw]) -> FramePlan {
        FramePlan::for_draws(draws, |draw| draw.id, |draw| {
            draw.options.last().copied().map_or(Planned::Wait, Planned::Draw)
        })
    }
}

let app = App::new(platform)
    .ui_budget(Duration::from_millis(4))
    .configure_painter(|painter| painter.effect_quality(Lowest));
```

## Custom effects

```rust
inventory::submit! { EffectRegistration { install } }

fn install(registry: &mut EffectRegistry, _: &FieldRoom) -> Result<(), Error> {
    registry.register_effect(ID, |c| matches!(c, DrawCommand::Shadow { .. }), GreenFill)
}
```

Building `lavashell-renderer` without the `backdrop` feature removes the built-in glass effect.

## Independent scenes

```rust
struct Scene;

impl SceneSource for Scene {
    fn next_deadline(&self) -> Duration {
        // The time of the next scene frame.
    }

    fn produce(
        &mut self,
        gpu: &mut VulkanRenderer,
        now: Duration,
        viewport: Viewport,
    ) -> Result<Option<PendingScene>, Error> {
        // Submit ready GPU work and return without waiting.
    }
}

let app = App::new(platform).scene(Scene);
```

`VulkanRenderer::import_scene` imports images from a producer on another Vulkan device without copying them.
`EmbeddedVulkanRenderer` paints the UI into images from a host engine.
