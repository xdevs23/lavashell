// SPDX-License-Identifier: GPL-3.0-only
use object::write::elf::{FileHeader, ProgramHeader, Sym, Writer};
use object::{Endianness, elf};
use std::num::{NonZeroU64, TryFromIntError};

pub const PAGE_SIZE: NonZeroU64 = NonZeroU64::MIN.saturating_add(0x4000 - 1);

#[derive(Debug, thiserror::Error)]
pub enum SyntheticElfError {
    #[error(transparent)]
    Write(#[from] object::write::Error),
    #[error(transparent)]
    SegmentCount(#[from] TryFromIntError),
}

pub struct SyntheticSymbol {
    pub name: &'static str,
    pub defined: bool,
    pub binding: elf::SymbolBind,
    pub visibility: elf::SymbolVisibility,
}

impl SyntheticSymbol {
    pub fn export(name: &'static str) -> Self {
        Self {
            name,
            defined: true,
            binding: elf::STB_GLOBAL,
            visibility: elf::STV_DEFAULT,
        }
    }

    pub fn import(name: &'static str) -> Self {
        Self {
            name,
            defined: false,
            binding: elf::STB_GLOBAL,
            visibility: elf::STV_DEFAULT,
        }
    }
}

pub struct SyntheticSegment {
    pub kind: elf::ProgramType,
    pub offset: u64,
    pub address: u64,
    pub memory_size: u64,
    pub alignment: u64,
}

pub struct SyntheticLibrary {
    pub machine: elf::Machine,
    pub file_type: elf::FileType,
    pub segments: Vec<SyntheticSegment>,
    pub needed: Vec<&'static str>,
    pub symbols: Vec<SyntheticSymbol>,
}

impl SyntheticLibrary {
    pub fn shared_object(needed: Vec<&'static str>, symbols: Vec<SyntheticSymbol>) -> Self {
        Self {
            machine: elf::EM_X86_64,
            file_type: elf::ET_DYN,
            segments: vec![
                SyntheticSegment {
                    kind: elf::PT_LOAD,
                    offset: 0,
                    address: 0,
                    memory_size: 0x1000,
                    alignment: PAGE_SIZE.get(),
                },
                SyntheticSegment {
                    kind: elf::PT_GNU_RELRO,
                    offset: PAGE_SIZE.get(),
                    address: PAGE_SIZE.get(),
                    memory_size: PAGE_SIZE.get(),
                    alignment: 1,
                },
            ],
            needed,
            symbols,
        }
    }

    pub fn encode(&self) -> Result<Vec<u8>, SyntheticElfError> {
        let mut bytes = Vec::new();
        let mut writer = Writer::new(Endianness::Little, true, &mut bytes);
        writer.reserve_file_header();
        writer.reserve_program_headers(u32::try_from(self.segments.len())?);
        let needed: Vec<_> = self
            .needed
            .iter()
            .map(|name| writer.add_dynamic_string(name.as_bytes()))
            .collect();
        let symbol_names: Vec<_> = self
            .symbols
            .iter()
            .map(|symbol| writer.add_dynamic_string(symbol.name.as_bytes()))
            .collect();
        let _null_symbol = writer.reserve_null_dynamic_symbol_index();
        for _symbol in &self.symbols {
            let _symbol_index = writer.reserve_dynamic_symbol_index();
        }
        let _dynsym_offset = writer.reserve_dynsym();
        let _dynstr_offset = writer.reserve_dynstr()?;
        let _dynamic_offset = writer.reserve_dynamic(needed.len() + 1);
        let _null_section = writer.reserve_null_section_index();
        let dynsym_index = writer.reserve_dynsym_section_index();
        let _dynstr_index = writer.reserve_dynstr_section_index();
        let _dynamic_index = writer.reserve_dynamic_section_index();
        let _shstrtab_index = writer.reserve_shstrtab_section_index();
        writer.reserve_shstrtab()?;
        writer.reserve_section_headers();

        writer.write_file_header(&FileHeader {
            os_abi: elf::ELFOSABI_SYSV,
            abi_version: 0,
            e_type: self.file_type,
            e_machine: self.machine,
            e_entry: 0,
            e_flags: elf::FileFlags(0),
        })?;
        let _program_headers_offset = writer.write_align_program_headers();
        for segment in &self.segments {
            writer.write_program_header(&ProgramHeader {
                p_type: segment.kind,
                p_flags: elf::ProgramFlags(0),
                p_offset: segment.offset,
                p_vaddr: segment.address,
                p_paddr: segment.address,
                p_filesz: 0,
                p_memsz: segment.memory_size,
                p_align: segment.alignment,
            });
        }
        let _null_symbol_offset = writer.write_null_dynamic_symbol();
        for (symbol, name) in self.symbols.iter().zip(&symbol_names) {
            let _symbol_index = writer.write_dynamic_symbol(&Sym {
                section: symbol.defined.then_some(dynsym_index.0),
                st_name: writer.dynamic_string_offset(Some(*name)),
                st_info: elf::SymbolInfo::new(symbol.binding, elf::STT_FUNC),
                st_other: symbol.visibility.into(),
                st_shndx: elf::SHN_UNDEF,
                st_value: 0,
                st_size: 0,
            });
        }
        writer.write_dynstr();
        let _dynamic_offset = writer.write_align_dynamic();
        for name in needed {
            writer.write_dynamic_string(elf::DT_NEEDED, name)?;
        }
        writer.write_dynamic(elf::DT_NULL, 0)?;
        writer.write_shstrtab();
        let _null_section_offset = writer.write_null_section_header();
        writer.write_dynsym_section_header(0, 1);
        writer.write_dynstr_section_header(0);
        writer.write_dynamic_section_header(0);
        writer.write_shstrtab_section_header();
        Ok(bytes)
    }
}
