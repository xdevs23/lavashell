// SPDX-License-Identifier: GPL-3.0-only

use crate::Error;
use lavashell_core::scene::FontWeight;
use num_traits::AsPrimitive;
use skia_safe::font_arguments::{VariationPosition, variation_position::Coordinate};
use skia_safe::font_style::{Slant, Weight};
use skia_safe::textlayout::{FontCollection, TypefaceFontProvider};
use skia_safe::{Data, FontArguments, FontMgr, Typeface};
use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const DEFAULT_FAMILY: &str = "sans-serif";

#[derive(Clone, Debug)]
pub enum FontOrigin {
    Embedded,
    File(PathBuf),
}

impl fmt::Display for FontOrigin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Embedded => f.write_str("embedded font"),
            Self::File(path) => write!(f, "font file {}", path.display()),
        }
    }
}

#[derive(Clone, Debug)]
pub struct RegisteredFont {
    pub alias: String,
    pub origin: FontOrigin,
}

impl fmt::Display for RegisteredFont {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} registered as {}", self.origin, self.alias)
    }
}

#[derive(Clone, Debug)]
struct Face {
    bytes: Arc<[u8]>,
    origin: FontOrigin,
}

#[derive(Clone, Debug)]
enum Source {
    Embedded(Vec<Face>),
    System(Vec<String>),
}

#[derive(Clone, Debug, Default)]
pub struct Fonts(BTreeMap<String, Source>);

impl Fonts {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn embedded(
        self,
        alias: impl Into<String>,
        bytes: impl Into<Arc<[u8]>>,
    ) -> Result<Self, Error> {
        let face = Face {
            bytes: bytes.into(),
            origin: FontOrigin::Embedded,
        };
        self.registered(alias.into(), Source::Embedded(vec![face]))
    }

    pub fn file(self, alias: impl Into<String>, path: impl AsRef<Path>) -> Result<Self, Error> {
        self.files(alias, [path])
    }

    pub fn files(
        self,
        alias: impl Into<String>,
        paths: impl IntoIterator<Item = impl AsRef<Path>>,
    ) -> Result<Self, Error> {
        let alias = alias.into();
        let faces: Vec<_> = (paths.into_iter())
            .map(|path| read_font_file(path.as_ref()))
            .collect::<Result<_, _>>()?;
        if faces.is_empty() {
            return Err(Error::NoFontFiles { alias });
        }
        self.registered(alias, Source::Embedded(faces))
    }

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

    pub(crate) fn load(&self) -> Result<LoadedFonts, Error> {
        let system = FontMgr::new();
        let mut provider = TypefaceFontProvider::new();
        for (alias, source) in &self.0 {
            let Source::Embedded(faces) = source else {
                continue;
            };
            for Face { bytes, origin } in faces {
                let font = RegisteredFont {
                    alias: alias.clone(),
                    origin: origin.clone(),
                };
                let face = system
                    .new_from_data(Data::new_copy(bytes), None)
                    .ok_or_else(|| Error::DecodeFont { font: font.clone() })?;
                for instance in weight_instances(&font, &face)? {
                    let _count_of_registered_faces_is_not_needed =
                        provider.register_typeface(instance, Some(alias.as_str()));
                }
            }
        }
        let aliases = (self.0.iter())
            .filter_map(|(alias, source)| match source {
                Source::System(families) if !families.is_empty() => {
                    Some((alias.clone(), families.clone()))
                }
                _ => None,
            })
            .collect();
        Ok(LoadedFonts {
            aliases,
            embedded: provider.into(),
            system,
            static_face_weights_by_family: HashMap::new(),
        })
    }
}

pub(crate) struct LoadedFonts {
    aliases: BTreeMap<String, Vec<String>>,
    embedded: FontMgr,
    system: FontMgr,
    static_face_weights_by_family: HashMap<String, Vec<FontWeight>>,
}

impl LoadedFonts {
    pub fn families<'a>(&'a self, alias: &'a str) -> Vec<&'a str> {
        alias_families(&self.aliases, alias)
    }

    pub fn static_face_weights(&mut self, alias: &str) -> &[FontWeight] {
        (self.static_face_weights_by_family.entry(alias.to_owned())).or_insert_with(|| {
            let families = alias_families(&self.aliases, alias);
            find_static_face_weights([&self.embedded, &self.system], &families)
        })
    }

    pub fn collection(&self) -> FontCollection {
        let mut collection = FontCollection::new();
        collection.set_asset_font_manager(Some(self.embedded.clone()));
        collection.set_default_font_manager(self.system.clone(), Some(DEFAULT_FAMILY));
        collection
    }

    pub fn uncached_collection(&self) -> FontCollection {
        let mut collection = self.collection();
        collection.paragraph_cache_mut().turn_on(false);
        collection
    }
}

fn read_font_file(path: &Path) -> Result<Face, Error> {
    std::fs::read(path)
        .map(|bytes| Face {
            bytes: Arc::from(bytes),
            origin: FontOrigin::File(path.to_owned()),
        })
        .map_err(|source| Error::ReadFont {
            path: path.to_owned(),
            source,
        })
}

fn alias_families<'a>(aliases: &'a BTreeMap<String, Vec<String>>, alias: &'a str) -> Vec<&'a str> {
    (aliases.get(alias)).map_or_else(
        || vec![alias],
        |families| families.iter().map(String::as_str).collect(),
    )
}

fn find_static_face_weights(managers: [&FontMgr; 2], families: &[&str]) -> Vec<FontWeight> {
    let mut faces = (families.iter().chain(&[DEFAULT_FAMILY]))
        .flat_map(|family| managers.map(|manager| manager.match_family(family)))
        .find_map(|mut faces| (faces.count() > 0).then_some(faces))
        .unwrap_or_default();
    (0..faces.count())
        .filter_map(|index| faces.new_typeface(index))
        .filter(|face| face.font_style().slant() == Slant::Upright && !has_weight_axis(face))
        .map(|face| FontWeight::saturating_whole(*face.font_style().weight()))
        .collect()
}

fn weight_instances(font: &RegisteredFont, face: &Typeface) -> Result<Vec<Typeface>, Error> {
    let parameters = face
        .variation_design_parameters()
        .ok_or_else(|| Error::ReadFontAxes { font: font.clone() })?;
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
            with_weight_arguments(skia_weight(weight), |arguments| {
                face.clone_with_arguments(arguments)
            })
            .ok_or_else(|| Error::InstantiateFont {
                font: font.clone(),
                weight,
            })
        })
        .collect()
}

pub(crate) fn skia_weight(weight: FontWeight) -> Weight {
    let whole: i32 = weight.get().round().as_();
    Weight::from(whole)
}

pub(crate) fn with_weight_arguments<R>(
    weight: Weight,
    use_arguments: impl FnOnce(&FontArguments) -> R,
) -> R {
    let coordinates = [Coordinate {
        axis: Coordinate::wght,
        value: (*weight).as_(),
    }];
    let position = VariationPosition {
        coordinates: &coordinates,
    };
    use_arguments(&FontArguments::new().set_variation_design_position(position))
}

pub(crate) fn has_weight_axis(face: &Typeface) -> bool {
    (face.variation_design_parameters().into_iter().flatten())
        .any(|axis| axis.tag == Coordinate::wght)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_font::PROBE_FAMILY;

    #[test]
    fn fallback_aliases_preserve_order_and_invalid_font_data_is_rejected() -> Result<(), Error> {
        let fonts = Fonts::new()
            .system("mono", ["Iosevka", "monospace"])?
            .load()?;
        assert_eq!(fonts.families("mono"), ["Iosevka", "monospace"]);
        assert_eq!(fonts.families("application-font"), ["application-font"]);
        assert!(
            Fonts::new()
                .embedded("broken", b"invalid font".as_slice())?
                .load()
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

    #[test]
    fn an_alias_without_font_files_fails() {
        assert!(matches!(
            Fonts::new().files(PROBE_FAMILY, Vec::<&Path>::new()),
            Err(Error::NoFontFiles { alias }) if alias == PROBE_FAMILY
        ));
    }

    #[test]
    fn a_text_file_fails_to_decode_with_its_path() -> Result<(), Error> {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        assert!(matches!(
            Fonts::new().file(PROBE_FAMILY, &manifest)?.load(),
            Err(Error::DecodeFont { font: RegisteredFont { origin: FontOrigin::File(path), .. } })
                if path == manifest
        ));
        Ok(())
    }
}

#[cfg(test)]
mod variable_font_tests {
    use crate::test_error::TestError;
    use crate::test_font::PROBE_FAMILY;

    #[test]
    fn registered_variable_weights_resolve_to_distinct_matching_faces() -> Result<(), TestError> {
        let mut collection = crate::test_font::test_font()?.load()?.collection();
        let mut identities = std::collections::BTreeSet::new();
        for weight in [400, 500, 600, 700] {
            let style = skia_safe::FontStyle::new(
                skia_safe::font_style::Weight::from(weight),
                skia_safe::font_style::Width::NORMAL,
                skia_safe::font_style::Slant::Upright,
            );
            let faces = collection.find_typefaces(&[PROBE_FAMILY], style);
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

    #[test]
    fn a_family_with_a_weight_axis_has_no_static_faces_and_is_looked_up_once()
    -> Result<(), TestError> {
        let mut fonts = crate::test_font::test_font()?.load()?;
        assert_eq!(fonts.static_face_weights(PROBE_FAMILY), []);
        assert!(
            fonts
                .static_face_weights_by_family
                .contains_key(PROBE_FAMILY)
        );
        Ok(())
    }
}

#[cfg(test)]
mod static_face_tests {
    use crate::test_error::TestError;
    use crate::test_font::PROBE_FAMILY;
    use lavashell_core::scene::FontWeight;
    use skia_safe::FontStyle;
    use skia_safe::font_style::{Slant, Weight, Width};

    #[test]
    fn static_faces_registered_under_one_alias_form_one_family() -> Result<(), TestError> {
        let mut fonts = crate::test_font::static_test_faces()?.load()?;
        let mut weights = fonts.static_face_weights(PROBE_FAMILY).to_vec();
        weights.sort_unstable();
        assert_eq!(weights, [FontWeight::NORMAL, FontWeight::BOLD]);
        let mut collection = fonts.collection();
        let mut identities = std::collections::BTreeSet::new();
        for weight in [Weight::NORMAL, Weight::BOLD] {
            let style = FontStyle::new(weight, Width::NORMAL, Slant::Upright);
            let faces = collection.find_typefaces(&[PROBE_FAMILY], style);
            assert_eq!(faces.len(), 1);
            assert_eq!(faces[0].font_style().weight(), weight);
            assert!(
                identities.insert(faces[0].unique_id()),
                "weights unexpectedly share one face"
            );
        }
        Ok(())
    }
}
