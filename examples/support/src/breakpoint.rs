// SPDX-License-Identifier: GPL-3.0-only
use lavashell::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    Compact,
    Regular,
}

impl Layout {
    #[must_use]
    pub fn choose<T>(self, compact: T, regular: T) -> T {
        match self {
            Self::Compact => compact,
            Self::Regular => regular,
        }
    }

    pub fn container(self, breakpoint: &Breakpoint) -> Modifier {
        match self {
            Self::Compact => breakpoint.given_size(),
            Self::Regular => breakpoint.given_size().then(breakpoint.content_sizes()),
        }
    }
}

#[derive(Clone)]
pub struct Breakpoint {
    full: State<Option<ContentSizes>>,
    given: State<Option<Size>>,
}

impl Breakpoint {
    pub fn new(ui: &mut Ui, key: &str) -> Self {
        Self {
            full: ui.state((key, "full"), || None),
            given: ui.state((key, "given"), || None),
        }
    }

    #[must_use]
    pub fn layout(&self, fits: impl FnOnce(ContentSizes, Size) -> bool) -> Layout {
        match (self.full.get(), self.given.get()) {
            (Some(full), Some(given)) if !fits(full, given) => Layout::Compact,
            _ => Layout::Regular,
        }
    }

    pub fn given_size(&self) -> Modifier {
        let given = self.given.clone();
        Modifier::new().on_measure(move |size| given.set_if_changed(Some(size)))
    }

    pub fn content_sizes(&self) -> Modifier {
        let measured = self.full.clone();
        Modifier::new().on_content_sizes(move |sizes| measured.set_if_changed(Some(sizes)))
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
    fn the_regular_layout_is_used_until_it_and_its_container_are_measured() {
        let wide = Size::new(800.dp(), 100.dp());
        let narrow = Size::new(400.dp(), 100.dp());
        let fits = |full: ContentSizes, given: Size| full.minimum.width <= given.width;
        assert_eq!(breakpoint(None, None).layout(fits), Layout::Regular);
        assert_eq!(breakpoint(Some(wide), None).layout(fits), Layout::Regular);
        assert_eq!(breakpoint(None, Some(narrow)).layout(fits), Layout::Regular);
        assert_eq!(
            breakpoint(Some(wide), Some(narrow)).layout(fits),
            Layout::Compact
        );
        assert_eq!(
            breakpoint(Some(narrow), Some(wide)).layout(fits),
            Layout::Regular
        );
    }
}
