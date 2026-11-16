// SPDX-License-Identifier: GPL-3.0-only
use super::{ElfError, ElfLibrary, LoadSegment};
use crate::synthetic_elf::{
    SyntheticElfError, SyntheticLibrary, SyntheticSegment, SyntheticSymbol,
};
use object::elf;
use std::collections::BTreeSet;

#[derive(Debug, thiserror::Error)]
enum TestError {
    #[error(transparent)]
    Synthetic(#[from] SyntheticElfError),
    #[error(transparent)]
    Elf(#[from] ElfError),
}

fn names(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(ToString::to_string).collect()
}

#[test]
fn reads_the_header_segments_and_needed_libraries() -> Result<(), TestError> {
    let synthetic = SyntheticLibrary::shared_object(vec!["libc.so", "libm.so"], Vec::new());
    let library = ElfLibrary::parse(&synthetic.encode()?)?;
    assert_eq!(library.machine, elf::EM_X86_64);
    assert_eq!(library.file_type, elf::ET_DYN);
    assert_eq!(
        library.load_segments,
        [LoadSegment {
            offset: 0,
            address: 0,
            memory_size: 0x1000,
            alignment: 0x4000,
        }]
    );
    assert_eq!(library.relro_ends, [0x8000]);
    assert_eq!(library.needed, ["libc.so", "libm.so"]);
    Ok(())
}

#[test]
fn exports_only_defined_global_or_weak_symbols_that_other_libraries_can_see()
-> Result<(), TestError> {
    let symbols = vec![
        SyntheticSymbol::export("global_default"),
        SyntheticSymbol {
            binding: elf::STB_WEAK,
            ..SyntheticSymbol::export("weak_default")
        },
        SyntheticSymbol {
            visibility: elf::STV_PROTECTED,
            ..SyntheticSymbol::export("global_protected")
        },
        SyntheticSymbol {
            visibility: elf::STV_HIDDEN,
            ..SyntheticSymbol::export("global_hidden")
        },
        SyntheticSymbol {
            binding: elf::STB_LOCAL,
            ..SyntheticSymbol::export("local_default")
        },
    ];
    let library =
        ElfLibrary::parse(&SyntheticLibrary::shared_object(Vec::new(), symbols).encode()?)?;
    assert_eq!(
        library.exports,
        names(&["global_default", "global_protected", "weak_default"])
    );
    assert_eq!(library.imports, BTreeSet::new());
    Ok(())
}

#[test]
fn imports_only_undefined_global_symbols() -> Result<(), TestError> {
    let symbols = vec![
        SyntheticSymbol::import("malloc"),
        SyntheticSymbol {
            binding: elf::STB_WEAK,
            ..SyntheticSymbol::import("optional_hook")
        },
    ];
    let library =
        ElfLibrary::parse(&SyntheticLibrary::shared_object(Vec::new(), symbols).encode()?)?;
    assert_eq!(library.imports, names(&["malloc"]));
    assert_eq!(library.exports, BTreeSet::new());
    Ok(())
}

#[test]
fn refuses_a_relro_segment_that_ends_beyond_the_address_space() -> Result<(), TestError> {
    let mut synthetic = SyntheticLibrary::shared_object(Vec::new(), Vec::new());
    synthetic.segments.push(SyntheticSegment {
        kind: elf::PT_GNU_RELRO,
        offset: 0,
        address: u64::MAX,
        memory_size: 1,
        alignment: 1,
    });
    assert!(matches!(
        ElfLibrary::parse(&synthetic.encode()?),
        Err(ElfError::RelroEndOverflow { index: 1 })
    ));
    Ok(())
}

#[test]
fn refuses_data_that_is_not_an_elf_file() {
    assert!(matches!(
        ElfLibrary::parse(b"not an ELF file"),
        Err(ElfError::Format(_))
    ));
}
