# Frame timing

```rust
let reports = Rc::new(RefCell::new(Vec::new()));
let log = reports.clone();
let app = App::new(platform)
    .gpu_timing(true)
    .frame_observer(move |report| log.borrow_mut().push(report));
```

```text
animation_error  = animation_step - display_step
prediction_error = animation_time[current] - display_time[current]

error_ms_per_second = sum(abs(animation_error_ms)) / measured_display_seconds
GN_error_percent    = 100 * sum(abs(animation_error)) / (2 * sum(display_step))
duration_ratio      = sum(animation_step) / sum(display_step)
```

`FrameReport::metrics` contains the step and error of each displayed pair. A report without `presentation_time` does not
include metrics. The definitions follow the animation-error section of the [GamersNexus methodology][animation-error].

## Recording

```sh
LAVASHELL_FRAME_LOG=frames.csv nix run .#motion -- --autoplay --exit-after 12
```

`App::incremental(false)` redraws every frame in full for a baseline.

[animation-error]:
https://gamersnexus.net/gpus-gn-extras-cpus/problem-gpu-benchmarks-reality-vs-numbers-animation-error-methodology-white
