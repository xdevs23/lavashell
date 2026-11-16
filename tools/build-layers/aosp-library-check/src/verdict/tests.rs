// SPDX-License-Identifier: GPL-3.0-only
use super::{Expectation, Problem, Verdict};
use crate::api_level::ApiLevel;
use crate::architecture::Architecture;
use crate::elf_library::{ElfLibrary, LoadSegment};
use crate::synthetic_elf::PAGE_SIZE;
use crate::system_stubs::Introductions;
use object::elf;

fn expectation() -> Expectation {
    Expectation {
        architecture: Architecture::X86_64,
        page_size: PAGE_SIZE,
        api_level: ApiLevel(26),
    }
}

fn segment(offset: u64, address: u64, alignment: u64) -> LoadSegment {
    LoadSegment {
        offset,
        address,
        memory_size: 0x1000,
        alignment,
    }
}

fn valid_library() -> ElfLibrary {
    ElfLibrary {
        machine: elf::EM_X86_64,
        file_type: elf::ET_DYN,
        load_segments: vec![segment(0, 0, 0x4000), segment(0x4100, 0x8100, 0x4000)],
        relro_ends: vec![0xc000],
        needed: vec!["libc.so".to_owned(), "liblog.so".to_owned()],
        exports: ["Java_entry".to_owned()].into(),
        imports: ["malloc".to_owned(), "write_log_message".to_owned()].into(),
    }
}

fn introductions() -> Introductions {
    Introductions {
        libraries: [
            ("libc.so".to_owned(), ApiLevel(21)),
            ("liblog.so".to_owned(), ApiLevel(21)),
        ]
        .into(),
        symbols: [
            ("malloc".to_owned(), ApiLevel(21)),
            ("write_log_message".to_owned(), ApiLevel(26)),
        ]
        .into(),
    }
}

fn problems(library: &ElfLibrary, introduced: &Introductions) -> Vec<Problem> {
    Verdict::of(library, &expectation(), introduced).problems
}

#[test]
fn accepts_a_library_that_meets_every_rule() {
    let verdict = Verdict::of(&valid_library(), &expectation(), &introductions());
    assert_eq!(verdict.problems, []);
    assert_eq!(verdict.required_api_level, Some(ApiLevel(26)));
}

#[test]
fn refuses_another_machine_and_another_file_type() {
    let library = ElfLibrary {
        machine: elf::EM_AARCH64,
        file_type: elf::ET_EXEC,
        ..valid_library()
    };
    assert_eq!(
        problems(&library, &introductions()),
        [
            Problem::WrongArchitecture {
                found: elf::EM_AARCH64,
                expected: Architecture::X86_64,
            },
            Problem::NotSharedObject {
                found: elf::ET_EXEC,
            },
        ]
    );
}

#[test]
fn refuses_load_segments_aligned_below_the_page_size_or_not_to_a_power_of_two() {
    let library = ElfLibrary {
        load_segments: vec![segment(0, 0, 0x1000), segment(0, 0, 0x6000)],
        ..valid_library()
    };
    assert_eq!(
        problems(&library, &introductions()),
        [
            Problem::LoadAlignmentInvalid {
                index: 0,
                alignment: 0x1000,
                page_size: PAGE_SIZE,
            },
            Problem::LoadAlignmentInvalid {
                index: 1,
                alignment: 0x6000,
                page_size: PAGE_SIZE,
            },
        ]
    );
}

#[test]
fn refuses_a_load_segment_whose_address_and_offset_differ_modulo_its_alignment() {
    let library = ElfLibrary {
        load_segments: vec![segment(0x4100, 0x8200, 0x4000)],
        ..valid_library()
    };
    assert_eq!(
        problems(&library, &introductions()),
        [Problem::LoadAddressMisaligned {
            index: 0,
            alignment: 0x4000,
        }]
    );
}

#[test]
fn refuses_missing_segments_and_a_relro_end_off_the_page_size() {
    let without_segments = ElfLibrary {
        load_segments: Vec::new(),
        relro_ends: Vec::new(),
        ..valid_library()
    };
    assert_eq!(
        problems(&without_segments, &introductions()),
        [Problem::NoLoadSegment, Problem::NoRelroSegment]
    );
    let misaligned_relro = ElfLibrary {
        relro_ends: vec![0xc010],
        ..valid_library()
    };
    assert_eq!(
        problems(&misaligned_relro, &introductions()),
        [Problem::RelroEndMisaligned {
            index: 0,
            end: 0xc010,
            page_size: PAGE_SIZE,
        }]
    );
}

#[test]
fn refuses_libraries_and_symbols_that_no_system_stub_provides() {
    let library = ElfLibrary {
        needed: vec![
            "libc.so".to_owned(),
            "libmissing.so".to_owned(),
            "lib/libc.so".to_owned(),
        ],
        imports: [
            "malloc".to_owned(),
            "write_log_message".to_owned(),
            "unknown".to_owned(),
        ]
        .into(),
        ..valid_library()
    };
    let verdict = Verdict::of(&library, &expectation(), &introductions());
    assert_eq!(
        verdict.missing_system_libraries,
        ["libmissing.so", "lib/libc.so"]
    );
    assert_eq!(verdict.unresolved_symbols, ["unknown"]);
    assert_eq!(
        verdict.problems,
        [
            Problem::LibrariesWithoutStubs {
                names: vec!["libmissing.so".to_owned(), "lib/libc.so".to_owned()],
            },
            Problem::UnresolvedSymbols {
                names: vec!["unknown".to_owned()],
            },
        ]
    );
}

#[test]
fn refuses_an_api_level_other_than_the_newest_import_needs() {
    let mut introduced = introductions();
    let _previous_level = introduced
        .symbols
        .insert("write_log_message".to_owned(), ApiLevel(30));
    assert_eq!(
        problems(&valid_library(), &introduced),
        [Problem::ApiLevelMismatch {
            declared: ApiLevel(26),
            required: Some(ApiLevel(30)),
        }]
    );
    let without_imports = ElfLibrary {
        needed: Vec::new(),
        imports: [].into(),
        ..valid_library()
    };
    assert_eq!(
        problems(&without_imports, &introductions()),
        [Problem::ApiLevelMismatch {
            declared: ApiLevel(26),
            required: None,
        }]
    );
}
