#include "loader.h"

void main() {
  ProgramPtr code_base, load_address;
  uint32_t reloc_address;
  uint8_t o65_error;

  load_address = (ProgramPtr)0x030000;
  reloc_address = 0x030000;

  o65_error = 0;
  code_base = RelocO65(load_address, &o65_error);
}
