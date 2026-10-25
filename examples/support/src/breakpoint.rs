// SPDX-License-Identifier: GPL-3.0-only
use lavashell::prelude::*;

/// The sizes needed to choose between the full and the compact arrangement of a layout: the content sizes of the
/// full arrangement from its last layout, and the size given to the container of either arrangement.
#[derive(Clone)]
pub struct Breakpoint {
    full: State<Option<ContentSizes>>,
    given: State<Option<Size>>,
}

impl Breakpoint {
    /// The breakpoint stored in the state of `ui` under `key`, so the sizes measured in one frame are available in
    /// the next.
    pub fn new(ui: &mut Ui, key: &str) -> Self {
        Self {
            full: ui.state((key, "full"), || None),
            given: ui.state((key, "given"), || None),
        }
    }

    /// Whether the full arrangement fits, as computed by `fits` from the content sizes of the full arrangement and the
    /// size given to the container. Returns `true` until both sizes have been measured.
    #[must_use]
    pub fn fits(&self, fits: impl FnOnce(ContentSizes, Size) -> bool) -> bool {
        match (self.full.get(), self.given.get()) {
            (Some(full), Some(given)) => fits(full, given),
            _ => true,
        }
    }

    /// The modifier for the container of the arrangement. The modifier records the size given to the container. With
    /// `full` set, the modifier also records the content sizes of the full arrangement.
    pub fn container(&self, full: bool) -> Modifier {
        let given = self.given.clone();
        let container = Modifier::new().on_measure(move |size| given.set_if_changed(Some(size)));
        if full {
            let measured = self.full.clone();
            container.on_content_sizes(move |sizes| measured.set_if_changed(Some(sizes)))
        } else {
            container
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn breakpoint(full: Option<Size>, given: Option<Size>) -> Breakpoint {
        let sizes = full.map(|minimum| ContentSizes {
            preferred: minimum,
            minimum,
        });
        Breakpoint {
            full: State::new(sizes),
            given: State::new(given),
        }
    }

    #[test]
    fn the_full_arrangement_fits_until_it_and_its_container_are_measured() {
        let wide = Size::new(800.dp(), 100.dp());
        let narrow = Size::new(400.dp(), 100.dp());
        let fits = |full: ContentSizes, given: Size| full.minimum.width <= given.width;
        assert!(breakpoint(None, None).fits(fits));
        assert!(breakpoint(Some(wide), None).fits(fits));
        assert!(breakpoint(None, Some(narrow)).fits(fits));
        assert!(!breakpoint(Some(wide), Some(narrow)).fits(fits));
        assert!(breakpoint(Some(narrow), Some(wide)).fits(fits));
    }
}
