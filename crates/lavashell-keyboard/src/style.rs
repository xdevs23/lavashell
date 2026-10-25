// SPDX-License-Identifier: GPL-3.0-only
use crate::look::{DAY, Look, NIGHT, Palette, Surface};
use lavashell_core::prelude::{Color, ColorScheme, Dp, Local};
use std::cell::RefCell;
use std::rc::Rc;

/// The colors of the keyboard, in a dark or a light variant of frosted glass. With reduced
/// transparency, the keyboard is drawn opaque instead. Two styles made from the same colors are
/// equal.
#[derive(Clone, Debug)]
pub struct KeyboardStyle {
    palette: Palette,
    /// The looks built for the most recent key height. Every clone of the style uses this cache.
    looks: Rc<RefCell<Option<Rc<Looks>>>>,
}

/// The glass look and the opaque look of one palette for keys of one height.
#[derive(Debug)]
pub(crate) struct Looks {
    key_height: Dp,
    glass: Look,
    opaque: Look,
}

impl Looks {
    pub fn get(&self, reduced_transparency: bool) -> &Look {
        if reduced_transparency {
            &self.opaque
        } else {
            &self.glass
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
    pub fn night() -> Self {
        Self::new(&NIGHT)
    }

    #[must_use]
    pub fn day() -> Self {
        Self::new(&DAY)
    }

    /// This style with the action key, the shift key under caps lock and the focus ring drawn in
    /// `accent`, and the word and the icon on the action key drawn in `on_accent`.
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

    /// The looks for keys `key_height` tall. They are made again only when the height changes.
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

/// The style of a keyboard composed in the view tree. By default it is `KeyboardStyle::night()` for
/// a dark color scheme of the platform and `KeyboardStyle::day()` for a light one. A wrapper or the
/// view can provide another style.
pub static KEYBOARD_STYLE: Local<KeyboardStyle> =
    Local::platform(|platform| match platform.color_scheme {
        ColorScheme::Dark => KeyboardStyle::night(),
        ColorScheme::Light => KeyboardStyle::day(),
    });
