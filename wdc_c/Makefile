WDCROOT=${HOME}/.wine/drive_c/wdc
WDCBIN=${WDCROOT}/Tools/bin
CC=wine ${WDCBIN}/WDC816CC.exe
LN=wine ${WDCBIN}/WDCLN.exe
AS=wine ${WDCBIN}/WDC816AS.exe
LIBDIR=${WDCROOT}/Tools/lib

%.obj: %.asm
	${AS} -L -S -G $^

%.asm: %.c
	${CC} -PX -SI -SF -PX -MC -MV -MU -MT -LT -AT -SOP -SO0S $^

%.s28: $(DEPS)
	${LN} -HM28 -C10000 -D20000 -T  $^ -O $@  -Lcc

boot.s28: c0c.obj loader.obj boot.obj 
c0c.obj: c0c.asm
boot.obj: boot.asm
boot.asm: boot.c
loader.asm: loader.c

clean:
	rm -f *.lst *.s28 *.map *.bin loader.asm boot.asm *.obj
