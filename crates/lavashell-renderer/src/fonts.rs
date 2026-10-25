// SPDX-License-Identifier: GPL-3.0-only
//! The fonts that the renderer registers for an application. Widgets name a font by an alias. The renderer maps each
//! alias to embedded font data or to a list of system families.

use crate::Error;
use crate::scalar::scalar;
use lavashell_core::scene::FontWeight;
use skia_safe::font_arguments::{VariationPosition, variation_position::Coordinate};
use skia_safe::textlayout::{FontCollection, TypefaceFontProvider};
use skia_safe::{Data, FontArguments, FontMgr, Typeface};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

#[derive(Clone, Debug)]
enum Source {
    Embedded(Arc<[u8]>),
    System(Vec<String>),
}

/// The fonts of one application, each registered under an alias as embedded font data or as an ordered list of
/// system families. The platform font manager looks up every alias without a registered font.
#[derive(Clone, Debug, Default)]
pub struct Fonts(BTreeMap<String, Source>);

impl Fonts {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// # Errors
    /// Returns an error when a font is already registered under `alias`.
    pub fn embedded(
        self,
        alias: impl Into<String>,
        bytes: impl Into<Arc<[u8]>>,
    ) -> Result<Self, Error> {
        self.registered(alias.into(), Source::Embedded(bytes.into()))
    }

    /// # Errors
    /// Returns an error when the file cannot be read or a font is already registered under `alias`.
    pub fn file(self, alias: impl Into<String>, path: impl AsRef<Path>) -> Result<Self, Error> {
        let path = path.as_ref();
        let bytes = std::fs::read(path).map_err(|source| Error::ReadFont {
            path: path.to_owned(),
            source,
        })?;
        self.embedded(alias, bytes)
    }

    /// # Errors
    /// Returns an error when a font is already registered under `alias`.
    pub fn system(
        self,
        alias: impl Into<String>,
        families: impl IntoIterator<Item = impl Into<String>>,
    ) -> Result<Self, Error> {
        let families = families.into_iter().map(Into::into).collect();
        self.registered(alias.into(), Source::System(families))
    }

    fn registered(mut self, alias: String, source: Source) -> Result<Self, Error> {
        if self.0.contains_key(&alias) {
            return Err(Error::DuplicateFontAlias(alias));
        }
        self.0.extend([(alias, source)]);
        Ok(self)
    }

    pub(crate) fn families<'a>(&'a self, alias: &'a str) -> Vec<&'a str> {
        match self.0.get(alias) {
            Some(Source::System(families)) if !families.is_empty() => {
                families.iter().map(String::as_str).collect()
            }
            _ => vec![alias],
        }
    }

    pub(crate) fn collection(&self) -> Result<FontCollection, Error> {
        let manager = FontMgr::new();
        let mut provider = TypefaceFontProvider::new();
        for (alias, source) in &self.0 {
            let Source::Embedded(bytes) = source else {
                continue;
            };
            let face = manager
                .new_from_data(Data::new_copy(bytes), None)
                .ok_or_else(|| Error::DecodeFont {
                    alias: alias.clone(),
                })?;
            for instance in weight_instances(alias, &face)? {
                let _registered_count = provider.register_typeface(instance, Some(alias.as_str()));
            }
        }
        let mut collection = FontCollection::new();
        collection.set_asset_font_manager(Some(provider.into()));
        collection.set_default_font_manager(manager, Some("sans-serif"));
        Ok(collection)
    }
}

/// An instance of `face` at every named weight inside the range of its weight axis, or `face` alone when it has
/// no weight axis or no named weight lies inside the range.
fn weight_instances(alias: &str, face: &Typeface) -> Result<Vec<Typeface>, Error> {
    let parameters = face
        .variation_design_parameters()
        .ok_or_else(|| Error::ReadFontAxes {
            alias: alias.to_owned(),
        })?;
    let Some(axis) = parameters
        .into_iter()
        .find(|axis| axis.tag == Coordinate::wght)
    else {
        return Ok(vec![face.clone()]);
    };
    let range = f64::from(axis.min)..=f64::from(axis.max);
    let inside = FontWeight::NAMED
        .into_iter()
        .filter(|weight| range.contains(&weight.get()))
        .collect::<Vec<_>>();
    if inside.is_empty() {
        return Ok(vec![face.clone()]);
    }
    (inside.into_iter())
        .map(|weight| {
            let value = scalar(weight.get()).map_err(|source| Error::FontWeightRange {
                alias: alias.to_owned(),
                weight,
                source,
            })?;
            let coordinates = [Coordinate {
                axis: Coordinate::wght,
                value,
            }];
            let position = VariationPosition {
                coordinates: &coordinates,
            };
            (face.clone_with_arguments(
                &FontArguments::new().set_variation_design_position(position),
            ))
            .ok_or_else(|| Error::InstantiateFont {
                alias: alias.to_owned(),
                weight,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_aliases_preserve_order_and_invalid_font_data_is_rejected() -> Result<(), Error> {
        let fonts = Fonts::new().system("mono", ["Iosevka", "monospace"])?;
        assert_eq!(fonts.families("mono"), ["Iosevka", "monospace"]);
        assert_eq!(fonts.families("application-font"), ["application-font"]);
        assert!(
            Fonts::new()
                .embedded("broken", b"invalid font".as_slice())?
                .collection()
                .is_err()
        );
        Ok(())
    }

    #[test]
    fn a_second_registration_of_an_alias_fails() -> Result<(), Error> {
        let fonts = Fonts::new().system("mono", ["Iosevka"])?;
        assert!(matches!(
            fonts.system("mono", ["monospace"]),
            Err(Error::DuplicateFontAlias(alias)) if alias == "mono"
        ));
        Ok(())
    }
}

#[cfg(test)]
mod variable_font_tests {
    use crate::test_error::TestError;

    #[test]
    fn registered_variable_weights_resolve_to_distinct_matching_faces() -> Result<(), TestError> {
        let mut collection = crate::test_font::test_font()?.collection()?;
        let mut identities = std::collections::BTreeSet::new();
        for weight in [400, 500, 600, 700] {
            let style = skia_safe::FontStyle::new(
                skia_safe::font_style::Weight::from(weight),
                skia_safe::font_style::Width::NORMAL,
                skia_safe::font_style::Slant::Upright,
            );
            let faces = collection.find_typefaces(&["probe"], style);
            assert_eq!(faces.len(), 1);
            let face = &faces[0];
            assert_eq!(
                face.font_style().weight(),
                skia_safe::font_style::Weight::from(weight)
            );
            assert!(
                identities.insert(face.unique_id()),
                "weights unexpectedly share one face"
            );
        }
        Ok(())
    }
}
