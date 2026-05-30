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
- The o65 program image is expected at `$030000`.
- The loader relocates the image in place.
- It then jumps to the relocated program using native 65816 `RTL` semantics.
- If no exported entry symbol is found, execution starts at the relocated text
  segment start.
- If an exported text symbol named `main` or `_main` exists, that symbol is used
  as the entry point. `_main` takes priority over `main`.

The addresses are currently build-time assumptions in `asm/o65_loader.a65`. If
using this on a real hardware system, you would adjust them accordingly.

## Implemented o65 Support

The loader currently supports:

- Native 65816 executable files.
- Simple addressing mode.
- 16-bit and 32-bit o65 size fields.
- Header option scanning, with structural validation of option lengths.
- Undefined/external reference list skipping.
- Text and data relocation tables.
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
- `0x06`: chained o65 file
- `0x07`: requested alignment is not satisfied by the in-place segment start
- `0x08`: unsupported CPU mode
- `0x09`: malformed header option

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
- chained-file rejection
- non-simple addressing rejection
- unsupported CPU rejection
- malformed header option rejection
- alignment acceptance/rejection
- 16-bit and 32-bit size fields
- header options and external references
- `WORD`, `HIGH`, `LOW`, and `SEGADDR` relocation behavior
- BSS clearing behavior with and without `BSSZERO`
- exported `main` / `_main` entry-point selection

The tests also keep a loader-size guard. As of this README, the assembled
loader is `1758` bytes.

## Remaining gaps / TODO

This is still a not a complete o65 runtime loader. Known gaps include:

- The input image address and loader address are hardcoded.
- Chained files are rejected, not loaded in sequence.
- 6502 and 65816-emulation-mode programs are rejected, not entered using a
  different calling convention.
- Undefined external references are skipped, not resolved through a late-binding
  symbol table.
- Export scanning only uses `main` / `_main`; it does not expose a general
  symbol lookup API.
- Header option payloads are not interpreted.
- Pagewise relocation mode is not implemented.
- Memory bounds and malformed-table checks are still minimal.
- The loader assumes loaded programs return with `RTL`.

## Notes

The authoritative o65 details are in Andre Fachat's format document. cc65 also
ships useful constants for the mode bits in `o65.inc`, and the tests mirror the
mode values used by that toolchain.
