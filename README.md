# 65816 o65 Loader

This is a relocating executable loader for the o65 binary format, written in
65816 assembly for ca65.

o65 is a small relocatable binary/object format for 6502-family (or other
similar) systems. It is useful when code cannot be linked to one fixed address
ahead of time: the loader can take an o65 executable image, relocate its
text/data references for the address where the image actually sits in memory,
and then transfer control to the loaded program.

The format is described here:

http://www.6502.org/users/andre/o65/

## Repository Layout

- `asm/` contains the maintained ca65 implementation.
- `tests/` contains an emulator-backed Rust integration test suite.

## Loader Model

The situation is:

- The loader binary is linked to run at `$010000`.
- The caller passes the o65 program image address in registers:
  - `A`: low 16 bits of the image address
  - `X`: bank byte of the image address
- The loader relocates the image in place.
- Native 65816 programs are entered with `RTL` semantics and are expected to
  return with `RTL`.
- Plain 6502 programs and `CPU2_65816_EMU` programs are entered in 65816
  emulation mode and are expected to return with `RTS`.
- If no exported entry symbol is found, execution starts at the relocated text
  segment start.
- If an exported text symbol named `main` or `_main` exists, that symbol is used
  as the entry point. `_main` takes priority over `main`.

The loader code address and direct-page workspace address are fixed at link
time by `asm/o65_loader.cfg`. The o65 image address is supplied by the caller at
runtime.

That split is intentional. A resident OS/monitor service normally has a known
address in the system memory map, and link-time placement keeps the loader
small and straightforward. Porting it to a different memory map should usually
mean adjusting `asm/o65_loader.cfg`; making the loader itself
position-independent is a separate portability feature, not a requirement for
the basic OS-resident use case.

## Implemented o65 Support

The loader currently supports:

- Native 65816 executable files.
- Plain 6502 executable files, entered in 65816 emulation mode.
- `CPU2_65816_EMU` executable files, entered in 65816 emulation mode.
- Simple addressing mode.
- 16-bit and 32-bit o65 size fields.
- Header option scanning, with structural validation of option lengths.
- Undefined/external reference list skipping.
- Chained executable loading.
- Text and data relocation tables.
- Relocation target bounds checking against the text/data segment currently
  being relocated.
- Exported global list scanning for `main` / `_main`.
- BSS clearing when the `BSSZERO` mode bit is set.
- Alignment checking for 2-byte, 4-byte, and 256-byte alignment requests.
- Relocation types:
  - `WORD`
  - `HIGH`
  - `LOW`
  - `SEGADDR`
  - `SEG`
- Segment relocation targets:
  - text
  - data
  - bss
  - zero/direct

(Unsupported inputs are rejected instead of being silently misloaded).

## Status Codes

The loader returns status in `A` and stores it in zero page `status`.

- `0x00`: success
- `0x01`: bad o65 header magic
- `0x02`: unknown relocation segment
- `0x03`: unknown relocation type
- `0x04`: object file, not executable
- `0x05`: non-simple addressing mode
- `0x07`: requested alignment is not satisfied by the in-place segment start
- `0x08`: unsupported CPU mode
- `0x09`: malformed header option
- `0x0a`: relocation target is outside the text/data segment being relocated
- `0x0b`: pagewise relocation mode is unsupported

## Build

The implementation uses ca65/ld65 from cc65:

```sh
make -C asm
```

Generated assembler/linker outputs are ignored by git.

## Tests

The test suite assembles the loader, loads it into a 24-bit memory model, runs
it under the `wdc65816` Rust emulator crate, and asserts on relocated memory,
loader status, and selected entry points.

It requires `make`, `ca65`, and `ld65` on `PATH`.

```sh
cargo test
```

Coverage includes:

- bad header rejection
- object-file rejection
- chained-file loading
- non-simple addressing rejection
- unsupported CPU2 rejection
- 6502 and 65816-emulation-mode entry/return
- malformed header option rejection
- pagewise relocation rejection
- alignment acceptance/rejection
- 16-bit and 32-bit size fields
- header options and external references
- `WORD`, `HIGH`, `LOW`, and `SEGADDR` relocation behavior
- data relocation table targeting
- BSS clearing behavior with and without `BSSZERO`
- exported `main` / `_main` entry-point selection

The tests also keep a loader-size guard. As of this README, the assembled
loader is `2100` bytes.

## Remaining gaps / TODO

This is still a not a complete o65 runtime loader. Known gaps include:

- 65C02, 65SC02, 65CE02, and 6502X CPU2 modes are rejected rather than
  emulated.
- Undefined external references are skipped, not resolved through a late-binding
  symbol table.
- Export scanning only uses `main` / `_main`; it does not expose a general
  symbol lookup API.
- Header option payloads are not interpreted.
- Pagewise relocation mode is rejected rather than implemented.
- Whole-image bounds and malformed-table truncation checks are still minimal.
- Native 65816 programs must return with `RTL`; 6502/emulation-mode programs
  must return with `RTS`.

## Notes

The authoritative o65 details are in Andre Fachat's format document. cc65 also
ships useful constants for the mode bits in `o65.inc`, and the tests mirror the
mode values used by that toolchain.
