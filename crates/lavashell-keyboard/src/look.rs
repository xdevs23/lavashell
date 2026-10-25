// SPDX-License-Identifier: GPL-3.0-only
//! The sizes, fonts, colors and animations used to draw the keyboard. Each value is a choice of the
//! visual design.

use crate::board::Grid;
use lavashell_core::prelude::*;
use std::time::Duration;

pub(crate) const KEY_CORNERS: Dp = Dp::new(22.0);
pub(crate) const PANEL_CORNERS: Dp = Dp::new(28.0);
pub(crate) const BUBBLE_CORNERS: Dp = Dp::new(28.0);

/// The height of the part of a key preview that rises above its key.
pub(crate) const BUBBLE_HEIGHT: Dp = Dp::new(100.0);

/// How far a key preview extends past each side of its key.
pub(crate) const BUBBLE_OVERHANG: Dp = Dp::new(15.0);
pub(crate) const POPUP_CORNERS: Dp = Dp::new(24.0);
pub(crate) const ENTRY_CORNERS: Dp = Dp::new(18.0);
pub(crate) const LIST_CORNERS: Dp = Dp::new(30.0);

/// The indent of the heading of the list of layouts, past the padding of the list.
pub(crate) const HEADING_INDENT: Dp = Dp::new(16.0);

pub(crate) const ENTRY_PADDING: Dp = Dp::new(18.0);

/// The space between the name and the arrangement of a layout in the list.
pub(crate) const NAME_GAP: Dp = Dp::new(12.0);

/// The space between the icon and the word on the action key.
pub(crate) const ACTION_GAP: Dp = Dp::new(10.0);

/// The width of the focus ring and its distance from the key.
pub(crate) const RING: Dp = Dp::new(3.0);

/// The scale of a pressed key.
const PRESSED: ScaleFactor = constant!(ScaleFactor::try_new(0.92));

pub(crate) struct Font {
    pub size: Sp,
    pub weight: FontWeight,
}

/// The font of the character on a key and of a letter in a popup.
pub(crate) const LETTER: Font = Font {
    size: Sp::new(29.0),
    weight: FontWeight::MEDIUM,
};

pub(crate) const PAGE_KEY: Font = Font {
    size: Sp::new(21.0),
    weight: FontWeight::SEMI_BOLD,
};

pub(crate) const SPACE_BAR: Font = Font {
    size: Sp::new(15.0),
    weight: FontWeight::MEDIUM,
};

pub(crate) const ACTION_KEY: Font = Font {
    size: Sp::new(20.0),
    weight: constant!(FontWeight::try_new(650.0)),
};

pub(crate) const PREVIEW: Font = Font {
    size: Sp::new(46.0),
    weight: constant!(FontWeight::try_new(550.0)),
};

pub(crate) const LIST_HEADING: Font = Font {
    size: Sp::new(15.0),
    weight: FontWeight::SEMI_BOLD,
};

pub(crate) const LAYOUT_NAME: Font = Font {
    size: Sp::new(22.0),
    weight: FontWeight::SEMI_BOLD,
};

pub(crate) const ARRANGEMENT: Font = Font {
    size: Sp::new(16.0),
    weight: FontWeight::NORMAL,
};

pub(crate) const KEY_ICON: Dp = Dp::new(28.0);
pub(crate) const ACTION_ICON: Dp = Dp::new(26.0);
pub(crate) const CHECK_ICON: Dp = Dp::new(24.0);

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Palette {
    pub ink: Color,
    pub muted: Color,
    pub accent: Color,
    pub on_accent: Color,
    /// The colors at the top and at the bottom of the glass panel. `solid_panel` is the color of
    /// the opaque panel.
    pub panel: [Color; 2],
    pub solid_panel: Color,
    pub panel_edge: Color,
    pub panel_shadow: Color,
    /// The colors at the top and at the bottom of a glass key that types a character. `solid_key`
    /// is the color of the opaque key.
    pub key: [Color; 2],
    pub solid_key: Color,
    pub function: Color,
    pub solid_function: Color,
    pub key_edge: Color,
    pub shine: Option<Color>,
    pub key_shadow: Color,
    /// The tint over a pressed key that types a character and over the pressed action key.
    /// `pressed_function` is the tint over the other pressed keys.
    pub pressed: Color,
    pub pressed_function: Color,
    pub shifted: Color,
    /// The opacity of the accent color in the fill of the shift key while caps lock is on.
    /// `caps_edge` is the opacity of the accent color in the edge of that key.
    pub caps_fill: f64,
    pub caps_edge: f64,
    pub tonal: Color,
    pub preview: [Color; 2],
    pub preview_edge: Color,
    pub preview_shadow: Color,
    pub popup: Color,
    pub popup_edge: Color,
    pub popup_shadow: Color,
    pub entry: Color,
    pub chosen: Color,
}

const WHITE: Color = Color::WHITE;
const BLACK: Color = Color::oklch(0.0, 0.0, 0.0);
const NAVY: Color = Color::oklch(0.27, 0.037, 264.0);

/// The colors of the dark style, `KeyboardStyle::night`. Its whites have a low opacity because the
/// view blends colors in linear light, where a little white over a dark surface already gives a
/// clearly lighter grey.
pub(crate) const NIGHT: Palette = Palette {
    ink: Color::oklch(0.963, 0.007, 258.0),
    muted: Color::oklch(0.78, 0.012, 258.0),
    accent: Color::oklch(0.825, 0.08, 260.0),
    on_accent: Color::oklch(0.321, 0.121, 260.0),
    panel: [
        Color::oklch(0.27, 0.014, 258.0).with_alpha(0.58),
        Color::oklch(0.17, 0.009, 258.0).with_alpha(0.74),
    ],
    solid_panel: Color::oklch(0.2, 0.009, 258.0),
    panel_edge: WHITE.with_alpha(0.042),
    panel_shadow: BLACK.with_alpha(0.38),
    key: [WHITE.with_alpha(0.0254), WHITE.with_alpha(0.0212)],
    solid_key: Color::oklch(0.355, 0.012, 258.0),
    function: WHITE.with_alpha(0.0094),
    solid_function: Color::oklch(0.275, 0.01, 258.0),
    key_edge: WHITE.with_alpha(0.0316),
    shine: Some(WHITE.with_alpha(0.0149)),
    key_shadow: BLACK.with_alpha(0.25),
    pressed: WHITE.with_alpha(0.0545),
    pressed_function: WHITE.with_alpha(0.0649),
    shifted: WHITE.with_alpha(0.1028),
    caps_fill: 0.17,
    caps_edge: 0.5,
    tonal: WHITE.with_alpha(0.0535),
    preview: [
        Color::oklch(0.51, 0.015, 258.0),
        Color::oklch(0.47, 0.014, 258.0),
    ],
    preview_edge: WHITE.with_alpha(0.115),
    preview_shadow: BLACK.with_alpha(0.45),
    popup: Color::oklch(0.3, 0.014, 258.0).with_alpha(0.82),
    popup_edge: WHITE.with_alpha(0.05),
    popup_shadow: BLACK.with_alpha(0.5),
    entry: WHITE.with_alpha(0.03),
    chosen: WHITE.with_alpha(0.1028),
};

pub(crate) const DAY: Palette = Palette {
    ink: Color::oklch(0.25, 0.02, 258.0),
    muted: Color::oklch(0.48, 0.02, 258.0),
    accent: Color::oklch(0.47, 0.13, 260.0),
    on_accent: WHITE,
    panel: [
        Color::oklch(0.97, 0.006, 258.0).with_alpha(0.7),
        Color::oklch(0.92, 0.01, 258.0).with_alpha(0.82),
    ],
    solid_panel: Color::oklch(0.94, 0.008, 258.0),
    panel_edge: WHITE.with_alpha(0.9),
    panel_shadow: NAVY.with_alpha(0.18),
    key: [WHITE, Color::oklch(0.975, 0.004, 258.0)],
    solid_key: WHITE,
    function: Color::oklch(0.87, 0.012, 258.0).with_alpha(0.9),
    solid_function: Color::oklch(0.87, 0.012, 258.0),
    key_edge: NAVY.with_alpha(0.08),
    shine: None,
    key_shadow: NAVY.with_alpha(0.16),
    pressed: NAVY.with_alpha(0.08),
    pressed_function: NAVY.with_alpha(0.1),
    shifted: Color::oklch(0.76, 0.02, 258.0).with_alpha(0.95),
    caps_fill: 0.3,
    caps_edge: 0.7,
    tonal: NAVY.with_alpha(0.1),
    preview: [WHITE, Color::oklch(0.96, 0.005, 258.0)],
    preview_edge: NAVY.with_alpha(0.12),
    preview_shadow: NAVY.with_alpha(0.25),
    popup: Color::oklch(0.97, 0.006, 258.0).with_alpha(0.9),
    popup_edge: WHITE.with_alpha(0.9),
    popup_shadow: NAVY.with_alpha(0.25),
    entry: NAVY.with_alpha(0.05),
    chosen: NAVY.with_alpha(0.14),
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Surface {
    Glass,
    Opaque,
}

#[derive(Clone, Debug)]
pub(crate) struct Look {
    pub ink: Color,
    pub muted: Color,
    pub accent: Color,
    pub panel: Material,
    /// The material of the keys that type a character, painted on each row as one surface.
    pub keys: Material,
    pub letter: StateStyles,
    pub function: StateStyles,
    pub shifted: StateStyles,
    pub caps: StateStyles,
    pub action: StateStyles,
    pub tonal: StateStyles,
    pub preview: Material,
    pub popup: Material,
    pub entry: Color,
    pub chosen: Color,
}

impl Look {
    pub fn new(palette: &Palette, key_height: Dp, surface: Surface) -> Self {
        let surfaces = Surfaces {
            palette,
            key_height,
            glass: surface == Surface::Glass,
        };
        let key = |fill: Color| surfaces.key(fill.into());
        Self {
            ink: palette.ink,
            muted: palette.muted,
            accent: palette.accent,
            panel: surfaces.panel(),
            keys: surfaces.key(surfaces.fill(vertical(palette.key, key_height), palette.solid_key)),
            letter: tinted(None, palette.pressed, None),
            function: tinted(
                Some(surfaces.key(surfaces.fill(palette.function.into(), palette.solid_function))),
                palette.pressed_function,
                None,
            ),
            shifted: tinted(Some(key(palette.shifted)), palette.pressed_function, None),
            caps: tinted(Some(surfaces.caps()), palette.pressed_function, None),
            action: tinted(
                Some(surfaces.action()),
                palette.pressed,
                Some(palette.on_accent),
            ),
            tonal: tinted(
                Some(key(palette.tonal)),
                palette.pressed_function,
                Some(palette.ink),
            ),
            preview: surfaces.preview(),
            popup: surfaces.popup(),
            entry: palette.entry,
            chosen: palette.chosen,
        }
    }
}

/// A gradient from the first color at the top to the second at `height`.
fn vertical([top, bottom]: [Color; 2], height: Dp) -> Brush {
    let kind = GradientKind::Linear {
        from: Point::new(0.dp(), 0.dp()),
        to: Point::new(0.dp(), height),
    };
    Gradient::new(
        kind,
        [ColorStop::new(0.0, top), ColorStop::new(1.0, bottom)],
    )
    .into()
}

const EDGE: Stroke = constant!(Stroke::new(Color::TRANSPARENT, Dp::new(1.0)));

/// A border of 1 dp in `color` around a surface.
fn edge(color: Color) -> Stroke {
    Stroke { color, ..EDGE }
}

/// `shadow` with its color replaced by `color`.
fn colored(shadow: Shadow, color: Color) -> Shadow {
    Shadow { color, ..shadow }
}

/// A transparent shadow blurred by `blur` that falls `drop` below its surface. `colored` gives it a
/// color.
const fn shadow(blur: f64, drop: f64) -> Result<Shadow, NumberError> {
    Shadow::new(
        Color::TRANSPARENT,
        Dp::new(blur),
        Point::new(Dp::ZERO, Dp::new(drop)),
    )
}

const KEY_SHADOW: Shadow = constant!(shadow(1.5, 1.0));

/// The line of light inside the top edge of a key, drawn only with `NIGHT`.
const SHINE: Shadow = constant!(shadow(0.0, 1.0)).inset(true);

const PANEL_SHADOW: Shadow = constant!(shadow(17.0, -10.0));
const ACTION_SHADOW: Shadow = constant!(shadow(4.0, 2.0));
const PREVIEW_SHADOW: Shadow = constant!(shadow(7.0, 8.0));
const POPUP_SHADOW: Shadow = constant!(shadow(17.0, 14.0));

const PANEL_BLUR: Backdrop = constant!(Backdrop::blur(Dp::new(18.0)));
const POPUP_BLUR: Backdrop = constant!(Backdrop::blur(Dp::new(20.0)));

struct Surfaces<'a> {
    palette: &'a Palette,
    key_height: Dp,
    glass: bool,
}

impl Surfaces<'_> {
    /// `glass` for a glass surface, or the opaque `solid` color for an opaque surface.
    fn fill(&self, glass: Brush, solid: Color) -> Brush {
        if self.glass { glass } else { solid.into() }
    }

    /// The material of a key: `fill` with an edge, a shadow and, when the palette has a shine, a
    /// line of light inside the top edge.
    fn key(&self, fill: Brush) -> Material {
        let palette = self.palette;
        let material = Material::new(fill)
            .border(edge(palette.key_edge))
            .shadow(colored(KEY_SHADOW, palette.key_shadow));
        match palette.shine {
            Some(shine) => material.shadow(colored(SHINE, shine)),
            None => material,
        }
    }

    fn panel(&self) -> Material {
        let palette = self.palette;
        let height = Grid::panel_height(self.key_height);
        let panel = Material::new(self.fill(vertical(palette.panel, height), palette.solid_panel))
            .border(edge(palette.panel_edge))
            .shadow(colored(PANEL_SHADOW, palette.panel_shadow));
        if self.glass {
            panel.backdrop(PANEL_BLUR)
        } else {
            panel
        }
    }

    fn action(&self) -> Material {
        let accent = self.palette.accent;
        let [lightness, chroma, hue] = accent.to_oklch();
        let lighter = Color::oklch(lightness + 0.03, chroma, hue);
        Material::new(vertical([lighter, accent], self.key_height))
            .border(edge(WHITE.with_alpha(0.3)))
            .shadow(colored(ACTION_SHADOW, accent.with_alpha(0.25)))
    }

    fn caps(&self) -> Material {
        let palette = self.palette;
        Material::new(palette.accent.with_alpha(palette.caps_fill))
            .border(edge(palette.accent.with_alpha(palette.caps_edge)))
    }

    fn preview(&self) -> Material {
        let palette = self.palette;
        let height = self.key_height + BUBBLE_HEIGHT;
        Material::new(vertical(palette.preview, height))
            .border(edge(palette.preview_edge))
            .shadow(colored(PREVIEW_SHADOW, palette.preview_shadow))
    }

    fn popup(&self) -> Material {
        let palette = self.palette;
        let fill = if self.glass {
            palette.popup
        } else {
            palette.popup.with_alpha(1.0)
        };
        let popup = Material::new(fill)
            .border(edge(palette.popup_edge))
            .shadow(colored(POPUP_SHADOW, palette.popup_shadow));
        if self.glass {
            popup.backdrop(POPUP_BLUR)
        } else {
            popup
        }
    }
}

/// The styles of a key drawn with `material` at rest. While the key is pressed, `tint` covers it
/// and it shrinks to `PRESSED`. `content` colors its text and icons.
fn tinted(material: Option<Material>, tint: Color, content: Option<Color>) -> StateStyles {
    StateStyles::new(move |state| {
        let mut appearance = InteractionAppearance::default();
        if let Some(material) = &material {
            appearance = appearance.material(material.clone(), KEY_CORNERS);
        }
        if let Some(content) = content {
            appearance = appearance.content(content);
        }
        if state.pointer.active {
            appearance = appearance.tint(tint, KEY_CORNERS);
        }
        appearance.scale(if state.pointer.active {
            PRESSED
        } else {
            ScaleFactor::ONE
        })
    })
    .motion(press_spring())
}

fn press_spring() -> Spring {
    spring!(Duration::from_millis(240), 0.8)
}

/// The scale from which a key preview grows when it opens.
pub(crate) const PREVIEW_OPENED_FROM: ScaleFactor = constant!(ScaleFactor::try_new(0.9));

/// The scale to which a key preview shrinks when it closes.
pub(crate) const PREVIEW_CLOSED_AT: ScaleFactor = constant!(ScaleFactor::try_new(0.9));

const PREVIEW_OPENING: Duration = Duration::from_millis(60);

const PREVIEW_CLOSING: Duration = Duration::from_millis(100);

/// The animation that opens a key preview. It slows down toward the end.
pub(crate) fn preview_opening() -> Tween {
    Tween::new(PREVIEW_OPENING)
        .easing(Easing::QuartOut)
        .preserve_velocity(false)
}

/// The animation that closes a key preview. It speeds up toward the end.
pub(crate) fn preview_closing() -> Tween {
    Tween::new(PREVIEW_CLOSING)
        .easing(cubic_bezier!(0.32, 0.0, 0.67, 0.0))
        .preserve_velocity(false)
}

pub(crate) fn show_spring() -> Spring {
    spring!(Duration::from_millis(620), 0.72)
}

/// The spring that moves the panel while it hides. It also moves the keys, which enter enlarged so
/// that each row looks like one shape, to their size at rest.
pub(crate) fn hide_spring() -> Spring {
    spring!(Duration::from_millis(400), 1.0)
}

/// The time after which `hide_spring`, starting at rest, has come within one `pixel` of the end of
/// `distance`, or `Duration::MAX` when it never comes that close. The spring does not overshoot, so
/// the remaining distance only shrinks and the first such time is final.
pub(crate) fn hide_time(distance: Dp, pixel: Dp) -> Duration {
    let spring = hide_spring();
    let start = Vector([distance.get()]);
    let covered = |time: Duration| {
        let (left, _) = spring.sample(&start, &Vector([0.0]), time.as_secs_f64());
        left.0[0] <= pixel.get()
    };
    spring.first_time(covered).unwrap_or(Duration::MAX)
}

/// The radius of the ends of a pill that is `height` tall.
pub(crate) fn pill(height: Dp) -> Dp {
    height / 2.0
}
