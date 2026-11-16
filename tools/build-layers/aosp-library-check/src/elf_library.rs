// SPDX-License-Identifier: GPL-3.0-only
use object::Endianness;
use object::elf;
use object::read::elf::{Dyn, FileHeader, ProgramHeader, Sym};
use serde::Serialize;
use std::collections::BTreeSet;
use std::str::Utf8Error;

type Header = elf::FileHeader64<Endianness>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct LoadSegment {
    pub offset: u64,
    pub address: u64,
    pub memory_size: u64,
    pub alignment: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ElfLibrary {
    pub machine: elf::Machine,
    pub file_type: elf::FileType,
    pub load_segments: Vec<LoadSegment>,
    pub relro_ends: Vec<u64>,
    pub needed: Vec<String>,
    pub exports: BTreeSet<String>,
    pub imports: BTreeSet<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ElfError {
    #[error(transparent)]
    Format(#[from] object::read::Error),
    #[error("the PT_GNU_RELRO segment {index} ends beyond the 64-bit address space")]
    RelroEndOverflow { index: usize },
    #[error("a name in the dynamic section is not UTF-8: {source}")]
    NameEncoding {
        #[from]
        source: Utf8Error,
    },
}

struct DynamicSymbol<'data> {
    name: &'data str,
    defined: bool,
    binding: elf::SymbolBind,
    visibility: elf::SymbolVisibility,
}

impl DynamicSymbol<'_> {
    fn is_exported(&self) -> bool {
        self.defined
            && [elf::STB_GLOBAL, elf::STB_WEAK].contains(&self.binding)
            && [elf::STV_DEFAULT, elf::STV_PROTECTED].contains(&self.visibility)
    }

    fn is_imported(&self) -> bool {
        !self.defined && self.binding == elf::STB_GLOBAL && !self.name.is_empty()
    }
}

impl ElfLibrary {
    pub fn parse(data: &[u8]) -> Result<Self, ElfError> {
        let header = Header::parse(data)?;
        let endian = header.endian()?;
        let segments = header.program_headers(endian, data)?;
        let sections = header.sections(endian, data)?;
        let load_segments = segments
            .iter()
            .filter(|segment| segment.p_type(endian) == elf::PT_LOAD)
            .map(|segment| LoadSegment {
                offset: segment.p_offset(endian),
                address: segment.p_vaddr(endian),
                memory_size: segment.p_memsz(endian),
                alignment: segment.p_align(endian),
            })
            .collect();
        let relro_ends = segments
            .iter()
            .filter(|segment| segment.p_type(endian) == elf::PT_GNU_RELRO)
            .enumerate()
            .map(|(index, segment)| {
                segment
                    .p_vaddr(endian)
                    .checked_add(segment.p_memsz(endian))
                    .ok_or(ElfError::RelroEndOverflow { index })
            })
            .collect::<Result<_, _>>()?;
        let needed = match sections.dynamic(endian, data)? {
            Some((entries, strings_index)) => {
                let strings = sections.strings(endian, data, strings_index)?;
                entries
                    .iter()
                    .filter(|entry| entry.d_tag(endian) == elf::DT_NEEDED)
                    .map(
                        |entry| Ok(std::str::from_utf8(entry.string(endian, strings)?)?.to_owned()),
                    )
                    .collect::<Result<_, ElfError>>()?
            }
            None => Vec::new(),
        };
        let table = sections.symbols(endian, data, elf::SHT_DYNSYM)?;
        let symbols = table
            .symbols()
            .iter()
            .map(|symbol| {
                Ok(DynamicSymbol {
                    name: std::str::from_utf8(symbol.name(endian, table.strings())?)?,
                    defined: !symbol.is_undefined(endian),
                    binding: symbol.st_bind(),
                    visibility: symbol.st_visibility(),
                })
            })
            .collect::<Result<Vec<_>, ElfError>>()?;
        Ok(Self {
            machine: header.e_machine(endian),
            file_type: header.e_type(endian),
            load_segments,
            relro_ends,
            needed,
            exports: names_where(&symbols, DynamicSymbol::is_exported),
            imports: names_where(&symbols, DynamicSymbol::is_imported),
        })
    }
}

fn names_where<'data>(
    symbols: &[DynamicSymbol<'data>],
    keep: impl Fn(&DynamicSymbol<'data>) -> bool,
) -> BTreeSet<String> {
    symbols
        .iter()
        .filter(|symbol| keep(symbol))
        .map(|symbol| symbol.name.to_owned())
        .collect()
}

#[cfg(test)]
mod tests;
