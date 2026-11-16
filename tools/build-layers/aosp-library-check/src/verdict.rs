// SPDX-License-Identifier: GPL-3.0-only
use crate::api_level::ApiLevel;
use crate::architecture::Architecture;
use crate::elf_library::{ElfLibrary, LoadSegment};
use crate::system_stubs::Introductions;
use object::elf;
use serde::{Serialize, Serializer};
use std::fmt;
use std::num::NonZeroU64;
use std::path::Path;

pub struct Expectation {
    pub architecture: Architecture,
    pub page_size: NonZeroU64,
    pub api_level: ApiLevel,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Problem {
    WrongArchitecture {
        found: elf::Machine,
        expected: Architecture,
    },
    NotSharedObject {
        found: elf::FileType,
    },
    NoLoadSegment,
    LoadAlignmentInvalid {
        index: usize,
        alignment: u64,
        page_size: NonZeroU64,
    },
    LoadAddressMisaligned {
        index: usize,
        alignment: u64,
    },
    NoRelroSegment,
    RelroEndMisaligned {
        index: usize,
        end: u64,
        page_size: NonZeroU64,
    },
    LibrariesWithoutStubs {
        names: Vec<String>,
    },
    UnresolvedSymbols {
        names: Vec<String>,
    },
    ApiLevelMismatch {
        declared: ApiLevel,
        required: Option<ApiLevel>,
    },
}

#[derive(Debug, Serialize)]
pub struct Verdict {
    #[serde(serialize_with = "debug_name")]
    pub architecture: elf::Machine,
    pub expected_architecture: Architecture,
    #[serde(serialize_with = "debug_name")]
    pub elf_type: elf::FileType,
    pub page_size: NonZeroU64,
    pub load_segments: Vec<LoadSegment>,
    pub relro_ends: Vec<u64>,
    pub needed_libraries: Vec<String>,
    pub missing_system_libraries: Vec<String>,
    pub unresolved_symbols: Vec<String>,
    pub api_level: ApiLevel,
    pub required_api_level: Option<ApiLevel>,
    #[serde(serialize_with = "messages")]
    pub problems: Vec<Problem>,
}

impl Verdict {
    #[must_use]
    pub fn of(library: &ElfLibrary, expectation: &Expectation, introduced: &Introductions) -> Self {
        let linking = Linking::of(library, introduced);
        let problems = header_problems(library, expectation)
            .chain(segment_problems(library, expectation.page_size))
            .chain(linking.problems(expectation.api_level))
            .collect();
        Self {
            architecture: library.machine,
            expected_architecture: expectation.architecture,
            elf_type: library.file_type,
            page_size: expectation.page_size,
            load_segments: library.load_segments.clone(),
            relro_ends: library.relro_ends.clone(),
            needed_libraries: library.needed.clone(),
            missing_system_libraries: linking.missing,
            unresolved_symbols: linking.unresolved,
            api_level: expectation.api_level,
            required_api_level: linking.required,
            problems,
        }
    }
}

struct Linking {
    missing: Vec<String>,
    unresolved: Vec<String>,
    required: Option<ApiLevel>,
}

impl Linking {
    fn of(library: &ElfLibrary, introduced: &Introductions) -> Self {
        let missing = library
            .needed
            .iter()
            .filter(|name| {
                Path::new(name).file_name() != Some(name.as_ref())
                    || !introduced.libraries.contains_key(*name)
            })
            .cloned()
            .collect();
        let unresolved = library
            .imports
            .iter()
            .filter(|name| !introduced.symbols.contains_key(*name))
            .cloned()
            .collect();
        let required = library
            .needed
            .iter()
            .filter_map(|name| introduced.libraries.get(name))
            .chain(
                library
                    .imports
                    .iter()
                    .filter_map(|name| introduced.symbols.get(name)),
            )
            .max()
            .copied();
        Self {
            missing,
            unresolved,
            required,
        }
    }

    fn problems(&self, api_level: ApiLevel) -> impl Iterator<Item = Problem> {
        [
            (!self.missing.is_empty()).then(|| Problem::LibrariesWithoutStubs {
                names: self.missing.clone(),
            }),
            (!self.unresolved.is_empty()).then(|| Problem::UnresolvedSymbols {
                names: self.unresolved.clone(),
            }),
            (self.required != Some(api_level)).then_some(Problem::ApiLevelMismatch {
                declared: api_level,
                required: self.required,
            }),
        ]
        .into_iter()
        .flatten()
    }
}

fn header_problems(
    library: &ElfLibrary,
    expectation: &Expectation,
) -> impl Iterator<Item = Problem> {
    [
        (library.machine != expectation.architecture.machine()).then_some(
            Problem::WrongArchitecture {
                found: library.machine,
                expected: expectation.architecture,
            },
        ),
        (library.file_type != elf::ET_DYN).then_some(Problem::NotSharedObject {
            found: library.file_type,
        }),
    ]
    .into_iter()
    .flatten()
}

fn segment_problems(library: &ElfLibrary, page_size: NonZeroU64) -> impl Iterator<Item = Problem> {
    let loads = library.load_segments.iter().enumerate();
    let relro_ends = library.relro_ends.iter().copied().enumerate();
    library
        .load_segments
        .is_empty()
        .then_some(Problem::NoLoadSegment)
        .into_iter()
        .chain(loads.filter_map(move |(index, segment)| load_problem(index, segment, page_size)))
        .chain(
            library
                .relro_ends
                .is_empty()
                .then_some(Problem::NoRelroSegment),
        )
        .chain(
            relro_ends
                .filter(move |(_index, end)| *end % page_size != 0)
                .map(move |(index, end)| Problem::RelroEndMisaligned {
                    index,
                    end,
                    page_size,
                }),
        )
}

fn load_problem(index: usize, segment: &LoadSegment, page_size: NonZeroU64) -> Option<Problem> {
    let alignment = segment.alignment;
    if alignment < page_size.get() || !alignment.is_power_of_two() {
        Some(Problem::LoadAlignmentInvalid {
            index,
            alignment,
            page_size,
        })
    } else if segment.address % alignment != segment.offset % alignment {
        Some(Problem::LoadAddressMisaligned { index, alignment })
    } else {
        None
    }
}

impl fmt::Display for Problem {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongArchitecture { found, expected } => {
                write!(
                    formatter,
                    "the ELF machine is {found:?}, expected {expected}"
                )
            }
            Self::NotSharedObject { found } => {
                write!(
                    formatter,
                    "the ELF type is {found:?}, expected a shared object"
                )
            }
            Self::NoLoadSegment => formatter.write_str("the ELF file has no PT_LOAD segment"),
            Self::LoadAlignmentInvalid {
                index,
                alignment,
                page_size,
            } => write!(
                formatter,
                "the alignment {alignment} of PT_LOAD[{index}] must be a power of two of at least {page_size}"
            ),
            Self::LoadAddressMisaligned { index, alignment } => write!(
                formatter,
                "the virtual address and the file offset of PT_LOAD[{index}] differ modulo the alignment {alignment}"
            ),
            Self::NoRelroSegment => formatter.write_str("the ELF file has no PT_GNU_RELRO segment"),
            Self::RelroEndMisaligned {
                index,
                end,
                page_size,
            } => write!(
                formatter,
                "the end {end:#x} of PT_GNU_RELRO[{index}] is not aligned to {page_size} bytes"
            ),
            Self::LibrariesWithoutStubs { names } => write!(
                formatter,
                "these DT_NEEDED libraries have no system stub: {}",
                names.join(", ")
            ),
            Self::UnresolvedSymbols { names } => write!(
                formatter,
                "no system library defines these imported symbols: {}",
                names.join(", ")
            ),
            Self::ApiLevelMismatch {
                declared,
                required: Some(required),
            } => write!(
                formatter,
                "the API level is {declared}, but the newest import was introduced in {required}"
            ),
            Self::ApiLevelMismatch {
                declared,
                required: None,
            } => write!(
                formatter,
                "the API level is {declared}, but no import has a known API level"
            ),
        }
    }
}

fn debug_name<S: Serializer>(value: &impl fmt::Debug, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.collect_str(&format_args!("{value:?}"))
}

fn messages<S: Serializer>(problems: &[Problem], serializer: S) -> Result<S::Ok, S::Error> {
    serializer.collect_seq(problems.iter().map(ToString::to_string))
}

#[cfg(test)]
mod tests;
