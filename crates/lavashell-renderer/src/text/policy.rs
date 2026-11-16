// SPDX-License-Identifier: GPL-3.0-only
use skia_safe::FontHinting;
use skia_safe::font::Edging;
use skia_safe::textlayout::{ParagraphStyle, TextStyle};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Hinting {
    None,
    Slight,
    Normal,
    Full,
}

impl From<FontHinting> for Hinting {
    fn from(hinting: FontHinting) -> Self {
        match hinting {
            FontHinting::None => Self::None,
            FontHinting::Slight => Self::Slight,
            FontHinting::Normal => Self::Normal,
            FontHinting::Full => Self::Full,
        }
    }
}

impl From<Hinting> for FontHinting {
    fn from(hinting: Hinting) -> Self {
        match hinting {
            Hinting::None => Self::None,
            Hinting::Slight => Self::Slight,
            Hinting::Normal => Self::Normal,
            Hinting::Full => Self::Full,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextAntialiasing {
    Aliased,
    Grayscale,
    Subpixel,
    SubpixelApproximate,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum PixelGeometry {
    #[default]
    None,
    Rgb,
    Bgr,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct TextDestination {
    pub opaque: bool,
    pub geometry: PixelGeometry,
}

impl TextDestination {
    #[must_use]
    pub fn surface_props(self) -> skia_safe::SurfaceProps {
        let geometry = match (self.opaque, self.geometry) {
            (true, PixelGeometry::Rgb) => skia_safe::PixelGeometry::RGBH,
            (true, PixelGeometry::Bgr) => skia_safe::PixelGeometry::BGRH,
            _ => skia_safe::PixelGeometry::Unknown,
        };
        skia_safe::SurfaceProps::new(skia_safe::SurfacePropsFlags::default(), geometry)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TextRendering {
    pub hinting: Hinting,
    pub fractional_positioning: bool,
    pub antialiasing: TextAntialiasing,
}

impl From<Edging> for TextAntialiasing {
    fn from(edging: Edging) -> Self {
        match edging {
            Edging::Alias => Self::Aliased,
            Edging::AntiAlias => Self::Grayscale,
            Edging::SubpixelAntiAlias => Self::Subpixel,
        }
    }
}

impl Default for TextRendering {
    fn default() -> Self {
        let defaults = TextStyle::new();
        Self {
            hinting: defaults.font_hinting().into(),
            fractional_positioning: defaults.subpixel(),
            antialiasing: defaults.font_edging().into(),
        }
    }
}

impl TextRendering {
    #[must_use]
    pub fn for_destination(mut self, destination: TextDestination) -> Self {
        let subpixel = matches!(
            self.antialiasing,
            TextAntialiasing::Subpixel | TextAntialiasing::SubpixelApproximate
        );
        if subpixel
            && (destination.geometry == PixelGeometry::None
                || (!destination.opaque && self.antialiasing == TextAntialiasing::Subpixel))
        {
            self.antialiasing = TextAntialiasing::Grayscale;
        }
        self
    }

    pub(crate) fn apply(self, text: &mut TextStyle, paragraph: &mut ParagraphStyle) {
        let edging = match self.antialiasing {
            TextAntialiasing::Aliased => Edging::Alias,
            TextAntialiasing::Grayscale | TextAntialiasing::SubpixelApproximate => {
                Edging::AntiAlias
            }
            TextAntialiasing::Subpixel => Edging::SubpixelAntiAlias,
        };
        let _chained_style = text
            .set_font_hinting(self.hinting.into())
            .set_subpixel(self.fractional_positioning)
            .set_font_edging(edging);
        if self.hinting == Hinting::None {
            let _chained_paragraph_style = paragraph.turn_hinting_off();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opaque_geometry_is_required_before_resolving_subpixel_text() {
        let request = TextRendering {
            antialiasing: TextAntialiasing::Subpixel,
            ..Default::default()
        };
        for geometry in [PixelGeometry::None, PixelGeometry::Rgb, PixelGeometry::Bgr] {
            for opaque in [false, true] {
                let target = TextDestination { opaque, geometry };
                let enabled = opaque && geometry != PixelGeometry::None;
                let policy = request.for_destination(target);
                assert_eq!(policy.antialiasing == TextAntialiasing::Subpixel, enabled);
                assert_eq!(target.surface_props().pixel_geometry().is_h(), enabled);
            }
        }
    }

    #[test]
    fn default_policy_preserves_native_paragraph_font_defaults() {
        let defaults = TextStyle::new();
        let mut text = TextStyle::new();
        let mut paragraph = ParagraphStyle::new();
        TextRendering::default().apply(&mut text, &mut paragraph);
        assert_eq!(text.font_hinting(), defaults.font_hinting());
        assert_eq!(text.subpixel(), defaults.subpixel());
        assert_eq!(text.font_edging(), defaults.font_edging());
        assert!(paragraph.hinting_is_on());
    }

    #[test]
    fn approximate_coverage_still_requires_explicit_output_geometry() {
        let policy = TextRendering {
            antialiasing: TextAntialiasing::SubpixelApproximate,
            ..Default::default()
        };
        for opaque in [true, false] {
            for geometry in [PixelGeometry::None, PixelGeometry::Rgb, PixelGeometry::Bgr] {
                let destination = TextDestination { opaque, geometry };
                assert_eq!(
                    policy.for_destination(destination).antialiasing,
                    if geometry == PixelGeometry::None {
                        TextAntialiasing::Grayscale
                    } else {
                        TextAntialiasing::SubpixelApproximate
                    }
                );
                if !opaque {
                    assert_eq!(
                        destination.surface_props().pixel_geometry(),
                        skia_safe::PixelGeometry::Unknown
                    );
                }
            }
        }
    }
}
