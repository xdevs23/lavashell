// SPDX-License-Identifier: GPL-3.0-only
use crate::look::{DAY, Look, NIGHT, Palette, Surface};
use lavashell_core::prelude::{Color, ColorScheme, Dp, Local};
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone, Debug)]
pub struct KeyboardStyle {
    palette: Palette,
    looks: Rc<RefCell<Option<Rc<Looks>>>>,
}

#[derive(Debug)]
pub(crate) struct Looks {
    key_height: Dp,
    glass: Look,
    opaque: Look,
}

impl Looks {
    pub fn get(&self, surface: Surface) -> &Look {
        match surface {
            Surface::Glass => &self.glass,
            Surface::Opaque => &self.opaque,
        }
    }
}

impl PartialEq for KeyboardStyle {
    fn eq(&self, other: &Self) -> bool {
        self.palette == other.palette
    }
}

impl KeyboardStyle {
    #[must_use]
    pub fn of(scheme: ColorScheme) -> Self {
        Self::new(scheme.select(&NIGHT, &DAY))
    }

    #[must_use]
    pub fn night() -> Self {
        Self::new(&NIGHT)
    }

    #[must_use]
    pub fn day() -> Self {
        Self::new(&DAY)
    }

    #[must_use]
    pub fn accent(self, accent: Color, on_accent: Color) -> Self {
        Self::new(&Palette {
            accent,
            on_accent,
            ..self.palette
        })
    }

    fn new(palette: &Palette) -> Self {
        Self {
            palette: *palette,
            looks: Rc::default(),
        }
    }

    pub(crate) fn looks(&self, key_height: Dp) -> Rc<Looks> {
        let mut cached = self.looks.borrow_mut();
        match cached.as_ref() {
            Some(looks) if looks.key_height == key_height => Rc::clone(looks),
            _ => {
                let looks = Rc::new(Looks {
                    key_height,
                    glass: Look::new(&self.palette, key_height, Surface::Glass),
                    opaque: Look::new(&self.palette, key_height, Surface::Opaque),
                });
                *cached = Some(Rc::clone(&looks));
                looks
            }
        }
    }
}

pub static KEYBOARD_STYLE: Local<KeyboardStyle> =
    Local::platform(|platform| KeyboardStyle::of(platform.color_scheme));
