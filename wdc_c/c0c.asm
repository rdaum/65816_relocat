	XREF _ROM_BEG_DATA
	XREF _BEG_DATA
	XREF _END_DATA
	XREF _BEG_UDATA
	XREF _END_UDATA 
	XREF _~main
stack equ $f000
start:
	clc				; clear carry
	xce				; clear emulation
	rep #$30		; 16 bit registers
	longi on
	longa on
	lda #$fe00		; set direct page
	tcd
	lda #stack		; set stack
	tcs

	sep #$20			; back to 8 bit
	longa off
	lda #^_BEG_DATA		; get data section bank
	pha
	plb					; set data bank register
	rep #$20			; back to 16 bit mode
	longa on

	;; copy ROM data if we have some
	lda #_END_DATA-_BEG_DATA ;number of bytes to copy
	beq SKIP ;if none, just skip
	dec A ;less one for MVN instruction
	ldx #<_ROM_BEG_DATA ;get source into X
	ldy #<_BEG_DATA ;get dest into Y
	mvn #^_ROM_BEG_DATA,#^_BEG_DATA ;copy bytes 

SKIP:
	ldx #_END_UDATA-_BEG_UDATA ;get number of bytes to clear
	beq done ;nothing to do
	lda #0 ;get a zero for storing
	sep #$20 ;do byte at a time
	ldy #_BEG_UDATA ;get beginning of zeros
LOOP STA |0,Y ;clear memory
	iny ;bump pointer
	dex ;decrement count
	bne LOOP ;continue till done
	rep #$20 ;16 bit memory reg
done: 

	jsl _~main
	rtl
