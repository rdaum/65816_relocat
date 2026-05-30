use std::process::Command;
use std::sync::OnceLock;

use wdc65816::{HasAddressBus, Processor, StatusRegister};

const LOADER_ADDR: usize = 0x010000;
const PROGRAM_ADDR: usize = 0x030000;
const SYMBOL_TABLE_ADDR: usize = 0x050000;
const ZP_ADDR: usize = 0x00ae00;
const RETURN_BANK: u8 = 0x02;
const RETURN_ADDR: u16 = 0x1234;

const ZP_STATUS: usize = ZP_ADDR;
const ZP_PROGRAM: usize = ZP_ADDR + 1;
const ZP_SEG_BASE: usize = ZP_ADDR + 5;
const ZP_HEADER_TABLE: usize = ZP_ADDR + 0x2b;
const ZP_ENTRY_ADDR: usize = ZP_ADDR + 0x73;

const O65_CPU_65816: u16 = 0x8000;
const O65_CPU2_65C02: u16 = 0x0010;
const O65_CPU2_65816_EMU: u16 = 0x0050;
const O65_PAGE_RELOC: u16 = 0x4000;
const O65_SIZE_32BIT: u16 = 0x2000;
const O65_FTYPE_OBJ: u16 = 0x1000;
const O65_ADDR_SIMPLE: u16 = 0x0800;
const O65_CHAIN: u16 = 0x0400;
const O65_BSSZERO: u16 = 0x0200;
const O65_ALIGN_2: u16 = 0x0001;
const O65_ALIGN_4: u16 = 0x0002;
const O65_ALIGN_256: u16 = 0x0003;

const TEXT_SEGMENT: u8 = 2;
const DATA_SEGMENT: u8 = 3;

#[derive(Clone)]
struct Memory {
    bytes: Vec<u8>,
}

impl Memory {
    fn new() -> Self {
        Self {
            bytes: vec![0; 0x1_000000],
        }
    }

    fn load(&mut self, addr: usize, data: &[u8]) {
        self.bytes[addr..addr + data.len()].copy_from_slice(data);
    }

    fn byte(&self, addr: usize) -> u8 {
        self.bytes[addr]
    }

    fn word(&self, addr: usize) -> u16 {
        u16::from_le_bytes([self.bytes[addr], self.bytes[addr + 1]])
    }

    fn long24(&self, addr: usize) -> u32 {
        u32::from(self.bytes[addr])
            | (u32::from(self.bytes[addr + 1]) << 8)
            | (u32::from(self.bytes[addr + 2]) << 16)
    }
}

impl HasAddressBus for Memory {
    fn read(&mut self, address: usize) -> u8 {
        self.bytes[address & 0x00ff_ffff]
    }

    fn write(&mut self, address: usize, value: u8) {
        self.bytes[address & 0x00ff_ffff] = value;
    }

    fn io(&mut self) {}
}

#[derive(Default)]
struct O65 {
    mode: u16,
    tbase: u32,
    text: Vec<u8>,
    dbase: u32,
    data: Vec<u8>,
    bbase: u32,
    blen: u32,
    zbase: u32,
    zlen: u32,
    stack: u32,
    options: Vec<Vec<u8>>,
    external_refs: Vec<Vec<u8>>,
    text_relocs: Vec<u8>,
    data_relocs: Vec<u8>,
    exports: Vec<Export>,
}

#[derive(Default)]
struct Export {
    name: Vec<u8>,
    segment: u8,
    value: u32,
}

impl O65 {
    fn new(text: Vec<u8>) -> Self {
        Self {
            mode: O65_CPU_65816 | O65_ADDR_SIMPLE,
            text,
            zbase: 0x000080,
            ..Self::default()
        }
    }

    fn build(&self) -> Vec<u8> {
        let mut out = vec![0x01, 0x00, b'o', b'6', b'5', 0x00];
        push_u16(&mut out, self.mode);

        if self.mode & 0x2000 == 0 {
            for value in [
                self.tbase,
                self.text.len() as u32,
                self.dbase,
                self.data.len() as u32,
                self.bbase,
                self.blen,
                self.zbase,
                self.zlen,
                self.stack,
            ] {
                push_u16(&mut out, value as u16);
            }
        } else {
            for value in [
                self.tbase,
                self.text.len() as u32,
                self.dbase,
                self.data.len() as u32,
                self.bbase,
                self.blen,
                self.zbase,
                self.zlen,
                self.stack,
            ] {
                push_u32(&mut out, value);
            }
        }

        for option in &self.options {
            assert!(option.len() <= 253);
            out.push((option.len() + 2) as u8);
            out.push(0x01);
            out.extend(option);
        }
        out.push(0x00);

        out.extend(&self.text);
        out.extend(&self.data);

        if self.mode & 0x2000 == 0 {
            push_u16(&mut out, self.external_refs.len() as u16);
        } else {
            push_u32(&mut out, self.external_refs.len() as u32);
        }
        for reference in &self.external_refs {
            out.extend(reference);
            out.push(0x00);
        }

        out.extend(&self.text_relocs);
        out.push(0x00);
        out.extend(&self.data_relocs);
        out.push(0x00);

        if self.mode & O65_SIZE_32BIT == 0 {
            push_u16(&mut out, self.exports.len() as u16);
        } else {
            push_u32(&mut out, self.exports.len() as u32);
        }
        for export in &self.exports {
            out.extend(&export.name);
            out.push(0x00);
            out.push(export.segment);
            if self.mode & O65_SIZE_32BIT == 0 {
                push_u16(&mut out, export.value as u16);
            } else {
                push_u32(&mut out, export.value);
            }
        }
        out
    }
}

fn push_u16(out: &mut Vec<u8>, value: u16) {
    out.extend(value.to_le_bytes());
}

fn push_u32(out: &mut Vec<u8>, value: u32) {
    out.extend(value.to_le_bytes());
}

fn build_loader() -> Vec<u8> {
    static LOADER: OnceLock<Vec<u8>> = OnceLock::new();
    LOADER
        .get_or_init(|| {
            let output = Command::new("make")
                .arg("-C")
                .arg("asm")
                .arg("-B")
                .arg("o65_loader.bin")
                .output()
                .expect("failed to execute make; install make and cc65/ca65");
            assert!(
                output.status.success(),
                "failed to build asm/o65_loader.bin\nstdout:\n{}\nstderr:\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );

            std::fs::read("asm/o65_loader.bin").expect("asm/o65_loader.bin should exist after make")
        })
        .clone()
}

fn run_loader(program: &[u8]) -> (Processor, Memory) {
    run_loader_at(PROGRAM_ADDR, program)
}

fn run_loader_at(program_addr: usize, program: &[u8]) -> (Processor, Memory) {
    run_loader_at_with_symbols(program_addr, program, 0, &[])
}

fn run_loader_with_symbols(program: &[u8], symbol_table: &[u8]) -> (Processor, Memory) {
    run_loader_at_with_symbols(PROGRAM_ADDR, program, SYMBOL_TABLE_ADDR, symbol_table)
}

fn run_loader_at_with_symbols(
    program_addr: usize,
    program: &[u8],
    symbol_table_addr: usize,
    symbol_table: &[u8],
) -> (Processor, Memory) {
    let loader = build_loader();
    let mut memory = Memory::new();
    memory.load(LOADER_ADDR, &loader);
    memory.load(program_addr, program);
    if !symbol_table.is_empty() {
        memory.load(symbol_table_addr, symbol_table);
    }
    for field_offset in (0..36).step_by(4) {
        memory.write(ZP_HEADER_TABLE + field_offset + 2, 0xcc);
        memory.write(ZP_HEADER_TABLE + field_offset + 3, 0xcc);
    }

    let mut cpu = Processor::new();
    cpu.p = StatusRegister::from_byte(0x00, false);
    cpu.pbr = 0x01;
    cpu.dbr = 0x01;
    cpu.pc = 0x0000;
    cpu.s = 0x01fc;
    cpu.a = (program_addr & 0xff) as u8;
    cpu.b = ((program_addr >> 8) & 0xff) as u8;
    cpu.xl = ((program_addr >> 16) & 0xff) as u8;
    cpu.xh = ((symbol_table_addr >> 16) & 0xff) as u8;
    cpu.yl = (symbol_table_addr & 0xff) as u8;
    cpu.yh = ((symbol_table_addr >> 8) & 0xff) as u8;

    let return_minus_one = RETURN_ADDR.wrapping_sub(1).to_le_bytes();
    memory.write(0x01fd, return_minus_one[0]);
    memory.write(0x01fe, return_minus_one[1]);
    memory.write(0x01ff, RETURN_BANK);

    for _ in 0..20_000 {
        if cpu.pbr == RETURN_BANK && cpu.pc == RETURN_ADDR {
            return (cpu, memory);
        }
        cpu.step(&mut memory);
    }

    panic!("loader did not return; cpu={cpu:?}");
}

fn symbol_table(entries: &[(&[u8], u32)]) -> Vec<u8> {
    let mut out = Vec::new();
    for (name, value) in entries {
        out.extend(*name);
        out.push(0x00);
        out.push((value & 0xff) as u8);
        out.push(((value >> 8) & 0xff) as u8);
        out.push(((value >> 16) & 0xff) as u8);
    }
    out.push(0x00);
    out
}

fn seg_base(memory: &Memory) -> usize {
    memory.long24(ZP_SEG_BASE) as usize
}

#[test]
fn loader_stays_small() {
    let size = build_loader().len();
    assert!(size <= 2750, "loader grew to {size} bytes");
}

#[test]
fn returns_bad_header_status() {
    let (cpu, memory) = run_loader(&[0x01, 0x00, b'n', b'o', b'p', 0x00]);

    assert_eq!(memory.byte(ZP_STATUS), 0x01);
    assert_eq!(cpu.c() & 0x00ff, 0x0001);
}

#[test]
fn rejects_object_files() {
    let mut o65 = O65::new(vec![0x6b]);
    o65.mode |= O65_FTYPE_OBJ;

    let (cpu, memory) = run_loader(&o65.build());

    assert_eq!(memory.byte(ZP_STATUS), 0x04);
    assert_eq!(cpu.c() & 0x00ff, 0x0004);
}

#[test]
fn rejects_non_simple_addressing() {
    let mut o65 = O65::new(vec![0x6b]);
    o65.mode &= !O65_ADDR_SIMPLE;

    let (cpu, memory) = run_loader(&o65.build());

    assert_eq!(memory.byte(ZP_STATUS), 0x05);
    assert_eq!(cpu.c() & 0x00ff, 0x0005);
}

#[test]
fn rejects_pagewise_relocation_mode() {
    let mut o65 = O65::new(vec![0x6b]);
    o65.mode |= O65_PAGE_RELOC;

    let (cpu, memory) = run_loader(&o65.build());

    assert_eq!(memory.byte(ZP_STATUS), 0x0b);
    assert_eq!(cpu.c() & 0x00ff, 0x000b);
}

#[test]
fn enters_native_65816_programs_with_rtl_return() {
    let o65 = O65::new(vec![0x6b]);

    let (cpu, memory) = run_loader_at(0x03_1000, &o65.build());

    assert_eq!(memory.byte(ZP_STATUS), 0x00);
    assert_eq!(cpu.c() & 0x00ff, 0x0000);
    assert!(!cpu.p.e);
}

#[test]
fn enters_6502_programs_in_emulation_mode() {
    let mut o65 = O65::new(vec![0x60]);
    o65.mode &= !O65_CPU_65816;

    let (cpu, memory) = run_loader_at(0x03_2000, &o65.build());

    assert_eq!(memory.byte(ZP_STATUS), 0x00);
    assert_eq!(cpu.c() & 0x00ff, 0x0000);
    assert!(!cpu.p.e);
}

#[test]
fn enters_65816_emulation_mode_programs_with_rts_return() {
    let mut o65 = O65::new(vec![0x60]);
    o65.mode &= !O65_CPU_65816;
    o65.mode |= O65_CPU2_65816_EMU;

    let (cpu, memory) = run_loader_at(0x03_4000, &o65.build());

    assert_eq!(memory.byte(ZP_STATUS), 0x00);
    assert_eq!(cpu.c() & 0x00ff, 0x0000);
    assert!(!cpu.p.e);
}

#[test]
fn rejects_unsupported_cpu2_modes() {
    let mut o65 = O65::new(vec![0x60]);
    o65.mode &= !O65_CPU_65816;
    o65.mode |= O65_CPU2_65C02;

    let (cpu, memory) = run_loader(&o65.build());

    assert_eq!(memory.byte(ZP_STATUS), 0x08);
    assert_eq!(cpu.c() & 0x00ff, 0x0008);
}

#[test]
fn rejects_malformed_header_option_length() {
    let mut program = O65::new(vec![0x6b]).build();
    program[26] = 1;

    let (cpu, memory) = run_loader(&program);

    assert_eq!(memory.byte(ZP_STATUS), 0x09);
    assert_eq!(cpu.c() & 0x00ff, 0x0009);
}

#[test]
fn relocates_chained_files_before_entering_first_image() {
    let mut first = O65::new(vec![0x6b]);
    first.mode |= O65_CHAIN;

    let mut second = O65::new(vec![
        0x6b, // The chain loader should not enter this image.
        0x34, 0x12,
    ]);
    second.tbase = 0x1000;
    second.text_relocs = vec![0x02, 0x80 | TEXT_SEGMENT];

    let first_image = first.build();
    let second_image = second.build();
    let first_base = PROGRAM_ADDR + 27;
    let second_base = PROGRAM_ADDR + first_image.len() + 27;
    let mut chain = first_image;
    chain.extend(second_image);

    let (_cpu, memory) = run_loader(&chain);

    assert_eq!(memory.byte(ZP_STATUS), 0x00);
    assert_eq!(memory.long24(ZP_ENTRY_ADDR), first_base as u32);
    assert_eq!(
        memory.word(second_base + 1),
        ((second_base as u32 + 0x0234) & 0xffff) as u16
    );
}

#[test]
fn rejects_unsatisfied_alignment() {
    let mut o65 = O65::new(vec![0x6b]);
    o65.mode |= O65_ALIGN_2;

    let (cpu, memory) = run_loader(&o65.build());

    assert_eq!(memory.byte(ZP_STATUS), 0x07);
    assert_eq!(cpu.c() & 0x00ff, 0x0007);
}

#[test]
fn accepts_aligned_word_segment_start() {
    let mut o65 = O65::new(vec![0x6b]);
    o65.mode |= O65_ALIGN_2;
    o65.options = vec![b"x".to_vec()];

    let (_cpu, memory) = run_loader(&o65.build());

    assert_eq!(memory.byte(ZP_STATUS), 0x00);
    assert_eq!(seg_base(&memory) & 0x01, 0);
}

#[test]
fn accepts_aligned_longword_segment_start() {
    let mut o65 = O65::new(vec![0x6b]);
    o65.mode |= O65_ALIGN_4;
    o65.options = vec![b"abc".to_vec()];

    let (_cpu, memory) = run_loader(&o65.build());

    assert_eq!(memory.byte(ZP_STATUS), 0x00);
    assert_eq!(seg_base(&memory) & 0x03, 0);
}

#[test]
fn accepts_aligned_page_segment_start() {
    let mut o65 = O65::new(vec![0x6b]);
    o65.mode |= O65_ALIGN_256;
    o65.options = vec![vec![b'x'; 227]];

    let (_cpu, memory) = run_loader(&o65.build());

    assert_eq!(memory.byte(ZP_STATUS), 0x00);
    assert_eq!(seg_base(&memory) & 0xff, 0);
}

#[test]
fn runs_minimal_program_and_records_segment_base() {
    let program = O65::new(vec![0x6b]).build();

    let (_cpu, memory) = run_loader(&program);

    assert_eq!(memory.byte(ZP_STATUS), 0x00);
    assert_eq!(
        memory.long24(ZP_PROGRAM),
        (PROGRAM_ADDR + program.len()) as u32
    );
    assert_eq!(seg_base(&memory), PROGRAM_ADDR + 27);
}

#[test]
fn accepts_program_pointer_from_caller() {
    let program_addr = 0x04_2000;
    let program = O65::new(vec![0x6b]).build();

    let (_cpu, memory) = run_loader_at(program_addr, &program);

    assert_eq!(memory.byte(ZP_STATUS), 0x00);
    assert_eq!(
        memory.long24(ZP_PROGRAM),
        (program_addr + program.len()) as u32
    );
    assert_eq!(seg_base(&memory), program_addr + 27);
}

#[test]
fn starts_at_exported_main_when_present() {
    let mut o65 = O65::new(vec![0x00, 0x6b]);
    o65.exports = vec![Export {
        name: b"main".to_vec(),
        segment: TEXT_SEGMENT,
        value: 1,
    }];

    let (_cpu, memory) = run_loader(&o65.build());
    let base = seg_base(&memory);

    assert_eq!(memory.byte(ZP_STATUS), 0x00);
    assert_eq!(memory.long24(ZP_ENTRY_ADDR), (base + 1) as u32);
}

#[test]
fn starts_at_exported_under_main_when_present() {
    let mut o65 = O65::new(vec![0x6b, 0x6b]);
    o65.exports = vec![Export {
        name: b"_main".to_vec(),
        segment: TEXT_SEGMENT,
        value: 1,
    }];

    let (_cpu, memory) = run_loader(&o65.build());
    let base = seg_base(&memory);

    assert_eq!(memory.byte(ZP_STATUS), 0x00);
    assert_eq!(memory.long24(ZP_ENTRY_ADDR), (base + 1) as u32);
}

#[test]
fn prefers_exported_under_main_over_main() {
    let mut o65 = O65::new(vec![0x00, 0x6b, 0x6b]);
    o65.exports = vec![
        Export {
            name: b"_main".to_vec(),
            segment: TEXT_SEGMENT,
            value: 2,
        },
        Export {
            name: b"main".to_vec(),
            segment: TEXT_SEGMENT,
            value: 1,
        },
    ];

    let (_cpu, memory) = run_loader(&o65.build());
    let base = seg_base(&memory);

    assert_eq!(memory.byte(ZP_STATUS), 0x00);
    assert_eq!(memory.long24(ZP_ENTRY_ADDR), (base + 2) as u32);
}

#[test]
fn applies_word_relocation_in_text_segment() {
    let mut o65 = O65::new(vec![
        0x6b, // RTL, so execution still exits cleanly after relocation.
        0x34, 0x12,
    ]);
    o65.tbase = 0x1000;
    o65.text_relocs = vec![
        0x02, // Offset 2: the low byte of the word after the RTL.
        0x80 | 0x02,
    ];

    let (_cpu, memory) = run_loader(&o65.build());
    let base = seg_base(&memory);

    assert_eq!(memory.byte(ZP_STATUS), 0x00);
    assert_eq!(
        memory.word(base + 1),
        ((base as u32 + 0x0234) & 0xffff) as u16
    );
}

#[test]
fn applies_word_relocation_in_data_segment() {
    let mut o65 = O65::new(vec![0x6b]);
    o65.data = vec![0x34, 0x12];
    o65.dbase = 0x2000;
    o65.data_relocs = vec![0x01, 0x80 | DATA_SEGMENT];

    let (_cpu, memory) = run_loader(&o65.build());
    let base = seg_base(&memory);
    let data_base = base + 1;

    assert_eq!(memory.byte(ZP_STATUS), 0x00);
    assert_eq!(
        memory.word(data_base),
        ((data_base as u32 + 0x1234 - 0x2000) & 0xffff) as u16
    );
}

#[test]
fn applies_low_high_and_segaddr_relocations() {
    let mut o65 = O65::new(vec![
        0x6b, // RTL
        0x11, // LOW text relocation target.
        0x22, // HIGH text relocation target.
        0x56, 0x34, 0x00, // SEGADDR text relocation target.
    ]);
    o65.tbase = 0x001200;
    o65.text_relocs = vec![
        0x02,
        0x20 | 0x02,
        0x01,
        0x40 | 0x02,
        0x00, // HIGH relocation low-byte payload.
        0x01,
        0xc0 | 0x02,
    ];

    let (_cpu, memory) = run_loader(&o65.build());
    let base = seg_base(&memory) as u32;

    assert_eq!(memory.byte(ZP_STATUS), 0x00);
    assert_eq!(memory.byte(base as usize + 1), (base + 0x11) as u8);
    assert_eq!(
        memory.byte(base as usize + 2),
        (((base + 0x2200) & 0x0000_ff00) >> 8) as u8
    );
    assert_eq!(memory.long24(base as usize + 3), base + 0x2256);
}

#[test]
fn rejects_relocation_past_segment_end() {
    let mut o65 = O65::new(vec![0x6b, 0x34]);
    o65.text_relocs = vec![0x02, 0x80 | TEXT_SEGMENT];

    let (cpu, memory) = run_loader(&o65.build());
    let base = seg_base(&memory);

    assert_eq!(memory.byte(ZP_STATUS), 0x0a);
    assert_eq!(cpu.c() & 0x00ff, 0x000a);
    assert_eq!(memory.byte(base + 1), 0x34);
}

#[test]
fn clears_bss_after_segments() {
    let mut o65 = O65::new(vec![0x6b]);
    o65.mode |= O65_BSSZERO;
    o65.data = vec![0xaa, 0xbb];
    o65.blen = 4;

    let (_cpu, memory) = run_loader(&o65.build());
    let base = seg_base(&memory);

    assert_eq!(memory.byte(ZP_STATUS), 0x00);
    assert_eq!(&memory.bytes[base + 1..base + 3], &[0xaa, 0xbb]);
    assert_eq!(&memory.bytes[base + 3..base + 7], &[0x00, 0x00, 0x00, 0x00]);
}

#[test]
fn leaves_bss_storage_alone_without_bsszero() {
    let mut o65 = O65::new(vec![0x6b]);
    o65.data = vec![0xaa, 0xbb];
    o65.blen = 4;
    o65.external_refs = vec![b"xy".to_vec()];
    let program = o65.build();

    let (_cpu, memory) = run_loader(&program);
    let base = seg_base(&memory);

    assert_eq!(memory.byte(ZP_STATUS), 0x00);
    assert_eq!(&memory.bytes[base + 1..base + 3], &[0xaa, 0xbb]);
    assert_eq!(&memory.bytes[base + 3..base + 7], &[0x01, 0x00, b'x', b'y']);
}

#[test]
fn skips_header_options_and_external_references() {
    let mut o65 = O65::new(vec![0x6b, 0x78, 0x56]);
    o65.options = vec![b"abc".to_vec(), b"z".to_vec()];
    o65.external_refs = vec![b"puts".to_vec(), b"main".to_vec()];
    o65.text_relocs = vec![0x02, 0x80 | 0x02];

    let (_cpu, memory) = run_loader(&o65.build());
    let base = seg_base(&memory);

    assert_eq!(memory.byte(ZP_STATUS), 0x00);
    assert_eq!(
        memory.word(base + 1),
        ((base as u32 + 0x5678) & 0xffff) as u16
    );
}

#[test]
fn rejects_unresolved_external_relocation() {
    let mut o65 = O65::new(vec![0x6b, 0x34, 0x12]);
    o65.external_refs = vec![b"puts".to_vec()];
    o65.text_relocs = vec![
        0x02,
        0x80, // WORD relocation against undefined segment 0.
        0x00,
        0x00,
    ];

    let (cpu, memory) = run_loader(&o65.build());
    let base = seg_base(&memory);

    assert_eq!(memory.byte(ZP_STATUS), 0x0c);
    assert_eq!(cpu.c() & 0x00ff, 0x000c);
    assert_eq!(memory.word(base + 1), 0x1234);
}

#[test]
fn rejects_external_relocation_index_past_reference_list() {
    let mut o65 = O65::new(vec![0x6b, 0x34, 0x12]);
    o65.external_refs = vec![b"puts".to_vec()];
    o65.text_relocs = vec![
        0x02,
        0x80, // WORD relocation against undefined segment 0.
        0x01,
        0x00,
    ];
    let symbols = symbol_table(&[(b"puts", 0x12_3456)]);

    let (cpu, memory) = run_loader_with_symbols(&o65.build(), &symbols);
    let base = seg_base(&memory);

    assert_eq!(memory.byte(ZP_STATUS), 0x0d);
    assert_eq!(cpu.c() & 0x00ff, 0x000d);
    assert_eq!(memory.word(base + 1), 0x1234);
}

#[test]
fn resolves_external_word_relocation_from_symbol_table() {
    let mut o65 = O65::new(vec![0x6b, 0x00, 0x00]);
    o65.external_refs = vec![b"puts".to_vec()];
    o65.text_relocs = vec![
        0x02,
        0x80, // WORD relocation against undefined segment 0.
        0x00,
        0x00,
    ];
    let symbols = symbol_table(&[(b"puts", 0x12_3456)]);

    let (_cpu, memory) = run_loader_with_symbols(&o65.build(), &symbols);
    let base = seg_base(&memory);

    assert_eq!(memory.byte(ZP_STATUS), 0x00);
    assert_eq!(memory.word(base + 1), 0x3456);
}

#[test]
fn publishes_exports_for_later_chained_external_relocations() {
    let mut first = O65::new(vec![0x6b, 0xea]);
    first.mode |= O65_CHAIN;
    first.exports = vec![Export {
        name: b"puts".to_vec(),
        segment: TEXT_SEGMENT,
        value: 1,
    }];

    let mut second = O65::new(vec![0x6b, 0x00, 0x00]);
    second.external_refs = vec![b"puts".to_vec()];
    second.text_relocs = vec![
        0x02,
        0x80, // WORD relocation against undefined segment 0.
        0x00,
        0x00,
    ];

    let first_image = first.build();
    let second_image = second.build();
    let first_base = PROGRAM_ADDR + 27;
    let second_base = PROGRAM_ADDR + first_image.len() + 27;
    let mut chain = first_image;
    chain.extend(second_image);
    let symbols = symbol_table(&[]);

    let (_cpu, memory) = run_loader_with_symbols(&chain, &symbols);

    assert_eq!(memory.byte(ZP_STATUS), 0x00);
    assert_eq!(memory.word(second_base + 1), (first_base + 1) as u16);
    assert_eq!(&memory.bytes[SYMBOL_TABLE_ADDR..SYMBOL_TABLE_ADDR + 5], b"puts\0");
    assert_eq!(memory.long24(SYMBOL_TABLE_ADDR + 5), (first_base + 1) as u32);
}

#[test]
fn supports_32_bit_o65_size_fields() {
    let mut o65 = O65::new(vec![0x6b, 0x78, 0x56]);
    o65.mode |= O65_SIZE_32BIT;
    o65.tbase = 0x020000;
    o65.text_relocs = vec![0x02, 0x80 | 0x02];

    let (_cpu, memory) = run_loader(&o65.build());
    let base = seg_base(&memory);

    assert_eq!(memory.byte(ZP_STATUS), 0x00);
    assert_eq!(
        memory.word(base + 1),
        ((base as u32 + 0x5678) & 0xffff) as u16
    );
}
