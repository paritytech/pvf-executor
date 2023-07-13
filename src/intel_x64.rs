use std::matches;

use crate::{CodeGenerator, codegen::{self, CodeEmitter, Relocation, OffsetMap}, ir::{Ir, IrReg, IrReg::*, IrCp::*, IrOperand::*, IrCond, IrCond::*, IrLabel, IrSignature}};

// Memory segment map
//
// Tables and data chunks are aligned to page size
//
//                 +-------------------+
//                 | DataChunkN        |
//                ~~~~~~~~~~~~~~~~~~~~~~~
//                 +-------------------+
//                 | DataChunk1        |
//                 +-------------------+
//                 | DataChunk0        |
//                 +-------------------+
//                 | TableN            |
//                ~~~~~~~~~~~~~~~~~~~~~~~
//                 +-------------------+
//                 | Table1            |
//                 +-------------------+
//                 | Table0            |
//        -0x20000 +-------------------+
//                 | Globals           |
//        -0x10000 +-------------------+
//                 | Transient VM data |
// base pointer -> +-------------------+
//                 | Volatile memory   |
//                 +-------------------+

pub struct IntelX64Compiler {
	call_targets: Vec<LinkTarget>,
	abs_off_targets: Vec<LinkTarget>,
	// table_map: Vec<isize>,
}

impl IntelX64Compiler {
	// pub fn new(tables: &Vec<u32>) -> Self {
	// 	let mut table_map = Vec::new();
	// 	for table_size in tables {
	// 		let table_pages = 1 + table_size * 8 / 0x10000;
	// 		table_map.push(-0x20000isize - table_pages as isize * 0x10000)
	// 	}
	// 	Self { call_targets: Vec::new(), table_map }
	// }
	pub fn new() -> Self {
		// let mut table_map = Vec::new();
		// for table_size in tables {
		// 	let table_pages = 1 + table_size * 8 / 0x10000;
		// 	table_map.push(-0x20000isize - table_pages as isize * 0x10000)
		// }
		Self { call_targets: Vec::new(), abs_off_targets: Vec::new() }
	}
}

#[derive(Debug)]
struct LinkTarget {
	offset: usize,
	func_index: u32,
}

const AX: u8 = 0;
const CX: u8 = 1;
const DX: u8 = 2;
const BX: u8 = 3;
const SP: u8 = 4;
const BP: u8 = 5;
const SI: u8 = 6;
const DI: u8 = 7;
const R8: u8 = 0;
const R9: u8 = 1;
const R10: u8 = 2;
const R11: u8 = 3;
const R12: u8 = 4;
const R15: u8 = 7;

const REX_B: u8 = 0x41;
const REX_X: u8 = 0x42;
const REX_R: u8 = 0x44;
const REX_W: u8 = 0x48;

const MOD_RM: u8 = 0x00;
const MOD_DISP8: u8 = 0x40;
const MOD_DISP32: u8 = 0x80;
const MOD_REG: u8 = 0xc0;

const MOD_SIB: u8 = 0x4;
const MOD_RIPREL: u8 = 0x5;

const SIB1: u8 = 0x00;
const SIB2: u8 = 0x40;
const SIB4: u8 = 0x80;
const SIB8: u8 = 0xc0;

const OPER_SIZE_OVR: u8 = 0x66;
const REP: u8 = 0xf3;

const ABI_PARAM_REGS: [(u8, u8); 6] = [(0, DI), (0, SI), (0, DX), (0, CX), (REX_R, R8), (REX_R, R9)];

const fn native_reg(r: &IrReg) -> u8 {
	match r {
		Sra => AX,
		Src => CX,
		Srd => DX,
	}
}

const fn native_cond(cond: &IrCond) -> u8 {
	match cond {
		Zero => 0x04,
		NotZero => 0x05,
		Equal => 0x04,
		NotEqual => 0x05,
		LessSigned => 0x0c,
		LessUnsigned => 0x02,
		GreaterSigned => 0x0f,
		GreaterUnsigned => 0x07,
		LessOrEqualSigned => 0x0e,
		LessOrEqualUnsigned => 0x06,
		GreaterOrEqualSigned => 0x0d,
		GreaterOrEqualUnsigned => 0x03,
	}
}

struct JmpTarget(usize, IrLabel);

impl CodeGenerator for IntelX64Compiler {
	fn compile_func(&mut self, code: &mut CodeEmitter, index: u32, body: Ir, signatures: &Vec<Option<IrSignature>>, offset_map: &OffsetMap) {
		macro_rules! emit {
			($($e:expr),*) => { { $(code.emit($e));* } }
		}

		macro_rules! emit_with_offset {
			($($e:expr),* ; $modrm:expr, $offset:expr) => {
				{
					emit!($($e),*);
					if $offset < i8::MIN as i32 || $offset > i8::MAX as i32 {
						emit!(MOD_DISP32 | $modrm);
						code.emit_imm32_le($offset);
					} else {
						emit!(MOD_DISP8 | $modrm, $offset as u8);
					}
				}
			};
			($($e:expr),* ; $modrm:expr, $sib:expr, $offset:expr) => {
				{
					emit!($($e),*);
					if $offset < i8::MIN as i32 || $offset > i8::MAX as i32 {
						emit!(MOD_DISP32 | $modrm, $sib);
						code.emit_imm32_le($offset);
					} else {
						emit!(MOD_DISP8 | $modrm, $sib, $offset as u8);
					}
				}
			}
		}

		macro_rules! emit_maybe_rexw {
			($rexw:expr, $($e:expr),*) => {
				{
					if $rexw {
						emit!(REX_W);
					}
					emit!($($e),*)
				}
			}
		}

		let mut jmp_targets = Vec::new();
		println!("S {:?}", signatures);
		let self_signature = signatures[index as usize].as_ref().expect("Self signature available");

		for insn in body.code() {
			match insn {
				Label(label) => code.label(label.clone()),
				EnterFunction(n_locals) => {
					emit!(REX_B, 0x50 | R12); // push r12
					emit!(REX_B, 0x50 | R15); // push r15

					emit!(REX_W | REX_B, 0xb8 | R15); // movabs r15, imm64
					code.reloc(Relocation::MemoryAbsolute64);
					code.emit_imm64_le(0);

					emit!(0x50 | BX); // push rbx
					emit!(0x50 | BP); // push rbp

					emit!(REX_W, 0x89, MOD_REG | SP << 3 | BX); // mov rbx, rsp
					emit!(REX_W, 0x89, MOD_REG | SP << 3 | BP); // mov rbp, rsp

					let n_params = self_signature.params;
					let n_total = n_locals + n_params;

					if n_total > 0 {
						// FIXME: Long offsets
						// emit!(REX_W, 0x83, MOD_REG | 0x0 << 3 | SP, n_total as u8 * 8); // add rsp, ntot*8
						let n_reg_params = std::cmp::min(n_params, ABI_PARAM_REGS.len() as u32);
						let n_stack_params = n_params.saturating_sub(ABI_PARAM_REGS.len() as u32);

						for i in 0..n_reg_params as usize {
							if ABI_PARAM_REGS[i].0 > 0 {
								// emit!(ABI_PARAM_REGS[i].0);
								emit!(REX_B); // FIXME: hack
							}
							emit!(0x50 | ABI_PARAM_REGS[i].1); // push <abi_reg>
						}

						if n_stack_params > 0 {
							// After the last off-stack argument, there were pushed:
							// - return address (by `call`)
							// - r12 (in preamble)
							// - r15 (in preamble)
							// - rbx (by `Ir` control flow code)
							// - rbp (by `Ir` control flow code)
							let mut caller_frame_off = 5i32 * 8;

							for _ in 0..n_stack_params {
								emit_with_offset!(REX_W, 0x8b ; AX << 3 | BX, caller_frame_off); // mov rax, [rbx+off]
								emit!(0x50 | AX); // push rax
								caller_frame_off += 8;
							}
						}

						if *n_locals > 0 {
							// All the locals are guaranteed to be initialized to zero
							emit!(0x31, MOD_REG | AX << 3 | AX); // xor eax, eax

							for _ in 0..*n_locals {
								emit!(0x50 | AX); // push rax
							}
						}
					}
				}
				LeaveFunction => {
					emit!(REX_W, 0x89, MOD_REG | BX << 3 | SP); // mov rsp, rbx
					emit!(0x58 | BP); // pop rbp
					emit!(0x58 | BX); // pop rbx
					emit!(REX_B, 0x58 | R15); // pop r15
					emit!(REX_B, 0x58 | R12); // pop r12
				}
				EnterBlock => {
					emit!(0x50 | BP); // push rbp
					emit!(REX_W, 0x89, MOD_REG | SP << 3 | BP); // mov rbp, rsp
				}
				LeaveBlock => {
					emit!(REX_W, 0x89, MOD_REG | BP << 3 | SP); // mov rsp, rbp
					emit!(0x58 | BP); // pop rbp
				}
				Push(op) => {
					match op {
						Reg(r) => emit!(0x50 | native_reg(r)), // push <reg>
						_ => unreachable!()
					}
				},
				Pop(op) => {
					match op {
						Reg(r) => emit!(0x58 | native_reg(r)), // pop <reg>
						_ => unreachable!()
					}
				},
				Move(dest, src) => {
					match (dest, src) {
						(Reg(rdest), Reg(rsrc)) => {
							// mov <dreg>, <sreg>
							emit!(REX_W, 0x89, MOD_REG | native_reg(rsrc) << 3 | native_reg(rdest));
						},
						(Reg(rdest), Imm32(imm)) | (Reg32(rdest), Imm32(imm)) => {
							// mov <dreg>, <imm32>
							emit!(0xb8 | native_reg(rdest));
							code.emit_imm32_le(*imm);
						},
						(Reg(rdest), Imm64(imm)) => {
							// mov <dreg>, <imm64>
							if *imm > 0 && *imm < u32::MAX as i64 {
								emit!(0xb8 | native_reg(rdest)); // movabs <rdest32>, <imm32>
								code.emit_imm32_le(*imm as i32);
							} else {
								emit!(REX_W, 0xb8 | native_reg(rdest)); // movabs <rdest>, <imm64> 
								code.emit_imm64_le(*imm);
							}
						},
						(Reg(rdest), Local(index)) => {
							// mov <dreg>, [ffp-local_off]
							if *index < 15 {
								emit!(REX_W, 0x8b, MOD_DISP8 | native_reg(rdest) << 3 | BX, -((*index as i8 + 1) * 8) as u8);
							} else {
								emit!(REX_W, 0x8b, MOD_DISP32 | native_reg(rdest) << 3 | BX);
								code.emit_imm32_le(-(*index as i32 + 1) * 8);
							}
						},
						(Reg(rdest), Global(index)) => {
							let offset = offset_map.globals() + *index as i32 * 8;
							emit!(REX_W | REX_B, 0x8b, MOD_DISP32 | native_reg(rdest) << 3 | R15); // mov <rdest>, [r15+<offset>]
							code.emit_imm32_le(offset);
						},
						(Global(index), Reg(rsrc)) => {
							let offset = offset_map.globals() + *index as i32 * 8;
							emit!(REX_W | REX_B, 0x89, MOD_DISP32 | native_reg(rsrc) << 3 | R15); // mov [r15+<offset>], <rsrc>
							code.emit_imm32_le(offset);
						},
						(Local(index), Reg(rsrc)) => {
							// mov [ffp-local_off], <sreg>
							if *index < 15 {
								emit!(REX_W, 0x89, MOD_DISP8 | native_reg(rsrc) << 3 | BX, -((*index as i8 + 1) * 8) as u8);
							} else {
								emit!(REX_W, 0x8b, MOD_DISP32 | native_reg(rsrc) << 3 | BX);
								code.emit_imm32_le(-(*index as i32 + 1) * 8);
							}
						},
						// TODO: Optimize for zero offset and short offsets
						(Memory8(offset, raddr), Reg8(rsrc)) => {
							emit!(REX_B, 0x88, MOD_DISP32 | native_reg(rsrc) << 3 | MOD_SIB, SIB1 | native_reg(raddr) << 3 | R15); // mov [r15+<raddr>*1+<offset>], <rsrc8>
							code.emit_imm32_le(*offset);
						},
						(Memory16(offset, raddr), Reg16(rsrc)) => {
							emit!(OPER_SIZE_OVR, REX_B, 0x89, MOD_DISP32 | native_reg(rsrc) << 3 | MOD_SIB, SIB1 | native_reg(raddr) << 3 | R15); // mov [r15+<raddr>*1+<offset>], <rsrc16>
							code.emit_imm32_le(*offset);
						},
						(Memory32(offset, raddr), Reg32(rsrc)) => {
							emit!(REX_B, 0x89, MOD_DISP32 | native_reg(rsrc) << 3 | MOD_SIB, SIB1 | native_reg(raddr) << 3 | R15); // mov [r15+<raddr>*1+<offset>], <rsrc32>
							code.emit_imm32_le(*offset);
						},
						(Memory64(offset, raddr), Reg(rsrc)) => {
							emit!(REX_W | REX_B, 0x89, MOD_DISP32 | native_reg(rsrc) << 3 | MOD_SIB, SIB1 | native_reg(raddr) << 3 | R15); // mov [r15+<raddr>*1+<offset>], <rsrc>
							code.emit_imm32_le(*offset);
						},
						(Reg8(rdest), Memory8(offset, raddr)) => {
							emit!(REX_B, 0x8a, MOD_DISP32 | native_reg(rdest) << 3 | MOD_SIB, SIB1 | native_reg(raddr) << 3 | R15); // mov <rdest8>, [r15+<raddr>*1+offset]
							code.emit_imm32_le(*offset);
						}
						(Reg16(rdest), Memory16(offset, raddr)) => {
							emit!(OPER_SIZE_OVR, REX_B, 0x8b, MOD_DISP32 | native_reg(rdest) << 3 | MOD_SIB, SIB1 | native_reg(raddr) << 3 | R15); // mov <rdest16>, [r15+<raddr>*1+offset]
							code.emit_imm32_le(*offset);
						}
						(Reg32(rdest), Memory32(offset, raddr)) => {
							emit!(REX_B, 0x8b, MOD_DISP32 | native_reg(rdest) << 3 | MOD_SIB, SIB1 | native_reg(raddr) << 3 | R15); // mov <rdest32>, [r15+<raddr>*1+offset]
							code.emit_imm32_le(*offset);
						}
						(Reg(rdest), Memory64(offset, raddr)) => {
							emit!(REX_W | REX_B, 0x8b, MOD_DISP32 | native_reg(rdest) << 3 | MOD_SIB, SIB1 | native_reg(raddr) << 3 | R15); // mov <rdest64>, [r15+<raddr>*1+offset]
							code.emit_imm32_le(*offset);
						}
						unk => todo!("ir Mov {:?}", unk),
					}
				},
				MoveIf(cond, dest, src) => {
					match (dest, src) {
						(Reg(rdest), Reg(rsrc)) | (Reg32(rdest), Reg32(rsrc)) => {
							emit_maybe_rexw!(matches!(dest, Reg(_)), 0x0f, 0x40 | native_cond(cond), MOD_REG | native_reg(rdest) << 3 | native_reg(rsrc)); // cmovcc <rdest32>, <rsrc32>
						},
						_ => todo!()
					}
				}
				ZeroExtend(src) => {
					match src {
						Reg8(rsrc) => emit!(0x0f, 0xb6, MOD_REG | native_reg(rsrc) << 3 | native_reg(rsrc)), // movzx <rsrc32>, <rsrc8>
						Reg16(rsrc) => emit!(0x0f, 0xb7, MOD_REG | native_reg(rsrc) << 3 | native_reg(rsrc)), // movzx <rsrc32>, <rsrc16>
						Reg32(rsrc) => emit!(0x89, MOD_REG | native_reg(rsrc) << 3 | native_reg(rsrc)), // mov <rsrc32>, <rsrc32> ; This zero-extends to 64 bits
						_ => unreachable!(),
					}
				},
				SignExtend(src) => {
					// FIXME!!!
					// When extending to i32, should not sign-extend to upper 32 bits
					// Not sure if matters but just to be on the safe side
					match src {
						Reg8(rsrc) => emit!(REX_W, 0x0f, 0xbe, MOD_REG | native_reg(rsrc) << 3 | native_reg(rsrc)), // movsx <rsrc>, <rsrc8>
						Reg16(rsrc) => emit!(REX_W, 0x0f, 0xbf, MOD_REG | native_reg(rsrc) << 3 | native_reg(rsrc)), // movsx <rsrc>, <rsrc16>
						Reg32(rsrc) => emit!(REX_W, 0x63, MOD_REG | native_reg(rsrc) << 3 | native_reg(rsrc)), // movsxd <rsrc>, <rsrc32>
						_ => unreachable!(),
					}
				},
				Add(dest, src) => {
					match (dest, src) {
						(Reg32(rdest), Reg32(rsrc)) => emit!(0x01, MOD_REG | native_reg(rsrc) << 3 | native_reg(rdest)), // add <dreg32>, <sreg32>
						(Reg(rdest), Reg(rsrc)) => emit!(REX_W, 0x01, MOD_REG | native_reg(rsrc) << 3 | native_reg(rdest)), // add <dreg32>, <sreg32>
						_ => todo!()
					}
				},
				Subtract(dest, src) => {
					match (dest, src) {
						(Reg32(rdest), Reg32(rsrc)) => emit!(0x29, MOD_REG | native_reg(rsrc) << 3 | native_reg(rdest)), // sub <dreg32>, <sreg32>
						(Reg(rdest), Reg(rsrc)) => emit!(REX_W, 0x29, MOD_REG | native_reg(rsrc) << 3 | native_reg(rdest)), // sub <dreg>, <sreg>
						_ => todo!()
					}
				},
				Multiply(dest, src) => {
					match (dest, src) {
						(Reg32(rdest), Reg32(rsrc)) => {
							if native_reg(rdest) != AX {
								if native_reg(rsrc) == AX {
									emit!(0x90 | native_reg(rdest)); // xchg <rdest32>, <rsrc32>
								} else {
									emit!(0x89, MOD_REG | native_reg(rdest) << 3 | AX); // mov eax, <rdest32>
								}
							}
							emit!(0xf7, MOD_REG | 0x5 << 3 | native_reg(rsrc)); // imul <rsrc32>
						},
						(Reg(rdest), Reg(rsrc)) => {
							if native_reg(rdest) != AX {
								if native_reg(rsrc) == AX {
									emit!(REX_W, 0x90 | native_reg(rdest)); // xchg <rdest>, <rsrc>
								} else {
									emit!(REX_W, 0x89, MOD_REG | native_reg(rdest) << 3 | AX); // mov rax, <rdest>
								}
							}
							emit!(REX_W, 0xf7, MOD_REG | 0x5 << 3 | native_reg(rsrc)); // imul <rsrc>
						},
						_ => todo!(),
					}
				},
				DivideUnsigned(dest, src) | DivideSigned(dest, src) | RemainderUnsigned(dest, src) | RemainderSigned(dest, src) => {
					match (dest, src) {
						(Reg32(rdest), Reg32(rsrc)) | (Reg(rdest), Reg(rsrc)) => {
							let is64 = matches!(dest, Reg(_));
							match (native_reg(rdest), native_reg(rsrc)) {
								(AX, CX) => (),
								(CX, AX) => emit_maybe_rexw!(is64, 0x90 | CX), // xchg {r|e}ax, {r|e}cx
								(AX, DX) => emit_maybe_rexw!(is64, 0x89, MOD_REG | DX << 3 | CX), // mov {r|e}cx, {r|e}dx
								(DX, AX) => {
									emit_maybe_rexw!(is64, 0x89, MOD_REG | AX << 3 | CX); // mov {r|e}cx, {r|e}ax
									emit_maybe_rexw!(is64, 0x89, MOD_REG | DX << 3 | AX); // mov {r|e}ax, {r|e}dx
								}
								(CX, DX) => {
									emit_maybe_rexw!(is64, 0x89, MOD_REG | CX << 3 | AX); // mov {r|e}ax, {r|e}cx
									emit_maybe_rexw!(is64, 0x89, MOD_REG | DX << 3 | CX); // mov {r|e}cx, {r|e}dx
								}
								(DX, CX) => emit_maybe_rexw!(is64, 0x89, MOD_REG | DX << 3 | AX), // mov {r|e}ax, {r|e}dx
								_ => unreachable!()
							}
							match insn {
								DivideSigned(_, _) | RemainderSigned(_, _) => {
									emit_maybe_rexw!(is64, 0x99); // {cdq|cqo}
									emit_maybe_rexw!(is64, 0xf7, MOD_REG | 0x7 << 3 | CX); // idiv {r|e}cx
								},
								DivideUnsigned(_, _) | RemainderUnsigned(_, _) => {
									emit!(0x31, MOD_REG | DX << 3 | DX); // xor edx, edx
									emit_maybe_rexw!(is64, 0xf7, MOD_REG | 0x6 << 3 | CX); // div {r|e}cx
								},
								_ => unreachable!()
							}
							if matches!(insn, RemainderUnsigned(_, _) | RemainderSigned(_, _)) {
								emit_maybe_rexw!(is64, 0x89, MOD_REG | DX << 3 | AX); // mov {r|e}ax, {r|e}dx
							}
						},
						_ => todo!(),
					}
				}
				Compare(dest, src) => {
					match (dest, src) {
						(Reg32(rdest), Reg32(rsrc)) | (Reg(rdest), Reg(rsrc)) => {
							emit_maybe_rexw!(matches!(dest, Reg(_)), 0x39, MOD_REG | native_reg(rsrc) << 3 | native_reg(rdest)); // cmp <dreg{32|64}>, <sreg{32|64}>
						},
						_ => unreachable!()
					}
				},
				SetIf(cond, dest) => {
					match dest {
						Reg(rdest) | Reg32(rdest) => {
							emit!(0x0f, 0x90 | native_cond(cond), MOD_REG | native_reg(rdest)); // setcc <dreg8>
							emit!(0x0f, 0xb6, MOD_REG | native_reg(rdest) << 3 | native_reg(rdest)); // movzx <dreg32>, <dreg8>
						},
						_ => unreachable!()
					}
				}
				And(dest, src) => {
					match (dest, src) {
						(Reg32(rdest), Reg32(rsrc)) => emit!(0x21, MOD_REG | native_reg(rsrc) << 3 | native_reg(rdest)), // and <dreg32>, <sreg32>
						(Reg(rdest), Reg(rsrc)) => emit!(REX_W, 0x21, MOD_REG | native_reg(rsrc) << 3 | native_reg(rdest)), // and <dreg>, <sreg>
						_ => todo!()
					}
				},
				Or(dest, src) => {
					match (dest, src) {
						(Reg32(rdest), Reg32(rsrc)) => emit!(0x09, MOD_REG | native_reg(rsrc) << 3 | native_reg(rdest)), // or <dreg32>, <sreg32>
						(Reg(rdest), Reg(rsrc)) => emit!(REX_W, 0x09, MOD_REG | native_reg(rsrc) << 3 | native_reg(rdest)), // or <dreg>, <sreg>
						_ => todo!()
					}
				},
				Xor(dest, src) => {
					match (dest, src) {
						(Reg32(rdest), Reg32(rsrc)) => emit!(0x31, MOD_REG | native_reg(rsrc) << 3 | native_reg(rdest)), // xor <dreg32>, <sreg32>
						(Reg(rdest), Reg(rsrc)) => emit!(REX_W, 0x31, MOD_REG | native_reg(rsrc) << 3 | native_reg(rdest)), // xor <dreg>, <sreg>
						_ => todo!()
					}
				},
				ShiftLeft(dest, cnt) | ShiftRightUnsigned(dest, cnt) | ShiftRightSigned(dest, cnt) | RotateLeft(dest, cnt) | RotateRight(dest, cnt) => {
					let is64 = matches!(dest, Reg(_));
					match (dest, cnt) {
						(Reg32(rdest), Reg32(rcnt)) | (Reg(rdest), Reg(rcnt)) => {
							let nr_dest = match (native_reg(rdest), native_reg(rcnt)) {
								(_, CX) => native_reg(rdest),
								(CX, _) => {
									emit_maybe_rexw!(is64, 0x87, MOD_REG | native_reg(rdest) << 3 | CX); // xchg <rdest{32|64}>, {r|e}cx
									native_reg(rcnt)
								},
								(_, _) => {
									emit_maybe_rexw!(is64, 0x89, MOD_REG | native_reg(rcnt) << 3 | CX); // mov {r|e}cx, <rcnt{32|64}>
									native_reg(rdest)
								}
							};

							let (opcode, add) = match insn {
								ShiftLeft(_, _) => (0xd3, 0x4),
								ShiftRightUnsigned(_, _) => (0xd3, 0x5),
								ShiftRightSigned(_, _) => (0xd3, 0x7),
								RotateLeft(_, _) => (0xd3, 0x0),
								RotateRight(_, _) => (0xd3, 0x1),
								_ => unreachable!()
							};
							emit_maybe_rexw!(is64, opcode, MOD_REG | add << 3 | nr_dest); // shl/shr/sar/rol/ror <rdest{32|64}>, cl
						},
						_ => todo!()
					}
				},
				Jump(label) => {
					// jmp near rel32 (no address just yet)
					emit!(0xe9);
					jmp_targets.push(JmpTarget(code.pc(), label.clone()));
					code.emit_imm32_le(0);
				},
				JumpIf(cond, label) => {
					// jcc near rel32 (no address just yet)
					emit!(0x0f, 0x80 | native_cond(cond));
					jmp_targets.push(JmpTarget(code.pc(), label.clone()));
					code.emit_imm32_le(0);
				},
				JumpTable(index, targets) => {
					match index {
						Reg32(rindex) => {
							// FIXME: It is implicit that this operation should preserve Sra as it
							// already contains the block return value, if any. It breaks the
							// "scratch-all" concept and should be refactored.
							//
							// BEWARE: rip-relative addressing with hardcoded offset
							emit!(REX_W, 0x8d, MOD_RM | DI << 3 | MOD_RIPREL, 0x08, 0x00, 0x00, 0x00); // lea rdi, [rip+8]
							emit!(0xc1, MOD_REG | 0x4 << 3 | native_reg(rindex), 0x03); // shl <rindex32>, 3
							emit!(REX_W, 0x01, MOD_REG | native_reg(rindex) << 3 | DI); // add rdi, <rindex32>
							emit!(0xff, MOD_RM | 0x4 << 3 | DI); // jmp [rdi]

							for target in targets {
								code.reloc(Relocation::LabelAbsoluteAddress(target.clone()));
								code.emit_imm64_le(0);
							}
						},
						_ => todo!()
					}
				}
				Call(label) => {
					let (findex, signature) = match label {
						IrLabel::AnonymousFunc(idx) | IrLabel::ExportedFunc(idx, _) | IrLabel::ImportedFunc(idx, _) => {
							let signature = if let Some(signature) = &signatures[*idx as usize] { signature } else { unreachable!() };
							(Some(*idx), signature) 
						},
						IrLabel::Indirect(_table_index, op, signature) => {
							match op {
								Reg32(op_reg) => {
									let offset = offset_map.vm_data() + codegen::VM_DATA_TMP_0 * 8;
									emit!(REX_W | REX_B, 0x89, MOD_DISP32 | native_reg(op_reg) << 3 | R15); // mov [r15+<offset>], <rsrc>
									code.emit_imm32_le(offset);
									(None, signature)
								},
								_ => todo!()
							}
						}
						_ => unreachable!(),
					};
					let n_params = signature.params;
					let n_stack_params = (n_params as usize).saturating_sub(ABI_PARAM_REGS.len());
					if n_params > 0 {
						let mut sp_off = 8 * (n_params as i32 - 1);
						for i in 0..std::cmp::min(n_params as usize, ABI_PARAM_REGS.len()) {
							emit_with_offset!(REX_W | ABI_PARAM_REGS[i].0, 0x8b ; ABI_PARAM_REGS[i].1 << 3 | SP, SIB1 | SP << 3 | SP, sp_off); // mov reg, [rsp + sp_off]
							sp_off -= 8;
						}
						if n_stack_params > 0 {
							emit!(REX_W, 0x89, MOD_REG | SP << 3 | AX); // mov rax, rsp
							emit!(REX_W, 0x83, MOD_REG | 0x0 << 3 | AX, 0x20); // add rax, 0x20 ; offset of the number of register params minus two
							emit!(REX_W, 0x83, MOD_REG | 0x4 << 3 | AX, 0xf0); // and rax, -16 ; align stack to 16 bytes, as per ABI requirements
							// At this point, rax points to the aligned bottom of the ABI frame,
							// and rsp points to the bottom of the overlapping Wasm frame. We'll
							// store the current rsp and rbp values into the space freed up after
							// populating registers with arguments to be able to get rid of the whole frame
							// when the call is returned.
							emit_with_offset!(REX_W, 0x89 ; SP << 3 | AX, n_stack_params as i32 * 8); // mov [rax + stored_sp_off], rsp
							emit_with_offset!(REX_W, 0x89 ; BP << 3 | AX, (n_stack_params + 1) as i32 * 8); // mov [rax + stored_bp_off], rbp
							emit!(REX_W, 0x89, MOD_REG | AX << 3 | BP); // mov rbp, rax
							emit!(REX_W | REX_B, 0x89, MOD_REG | BP << 3 | R11); // mov r11, rbp
							let frame_off = (n_stack_params as i32 - 1) * 8;
							if frame_off > i8::MAX as i32 { // add r11, (nsp-1)*8
								emit!(REX_W | REX_B, 0x81, MOD_REG | 0x0 << 3 | R11);
								code.emit_imm32_le(frame_off);
							} else {
								emit!(REX_W | REX_B, 0x83, MOD_REG | 0x0 << 3 | R11, frame_off as u8);
							}
							// l1:
							emit!(0x58 | AX); // pop rax
							emit!(REX_W | REX_B, 0x89, MOD_RM | AX << 3 | R11); // mov [r11], rax
							emit!(REX_W | REX_B, 0x83, MOD_REG | 0x5 << 3 | R11, 0x08); // sub r11, 8
							emit!(REX_W | REX_B, 0x39, MOD_REG | BP << 3 | R11); // cmp r11, rbp
							emit!(REX_W, 0x0f, 0x42, MOD_REG | SP << 3 | BP); // cmovb rsp, rbp
							emit!(0x72, 0x20); // jb l3
							emit!(REX_W, 0x39, MOD_REG | SP << 3 | BP); // cmp rbp, rsp
							emit!(0x75, 0xea); // jne l1
							// l2:
							emit!(REX_W, 0x8b, MOD_DISP8 | AX << 3 | BP, 0x00); // mov rax, [rbp+0]
							emit!(REX_W | REX_R | REX_B, 0x8b, MOD_RM | R10 << 3 | R11); // mov r10, [r11]
							emit!(REX_W | REX_B, 0x89, MOD_RM | AX << 3 | R11); // mov [r11], rax
							emit!(REX_W | REX_R, 0x89, MOD_DISP8 | R10 << 3 | BP, 0x00); // mov [rbp+0], r10
							emit!(REX_W | REX_B, 0x83, MOD_REG | 0x5 << 3 | R11, 0x08); // sub r11, 8
							emit!(REX_W, 0x83, MOD_REG | 0x0 << 3 | BP, 0x08); // add rbp, 8
							emit!(REX_W | REX_B, 0x39, MOD_REG | BP << 3 | R11); // cmp r11, rbp
							emit!(0x73, 0xe5); // jae l2
							// l3:
						} else {
							// No stack parameters, but stack alignment is still required
							emit!(REX_W | REX_B, 0x89, MOD_REG | SP << 3 | R12); // mov r12, rsp
							emit!(REX_W, 0x83, MOD_REG | 0x4 << 3 | SP, 0xf0); // and rsp, -16

						}
					} else {
						// No parameters, but stack alignment is still required
						emit!(REX_W | REX_B, 0x89, MOD_REG | SP << 3 | R12); // mov r12, rsp
						emit!(REX_W, 0x83, MOD_REG | 0x4 << 3 | SP, 0xf0); // and rsp, -16
					}
					match label {
						IrLabel::AnonymousFunc(_) | IrLabel::ExportedFunc(_, _) => {
							emit!(0xe8); // call near (no address yet)
							self.call_targets.push(LinkTarget { offset: code.pc(), func_index: findex.expect("Function is always `Some` for the given label type") as u32 });
							code.emit_imm32_le(0);
						},
						IrLabel::ImportedFunc(_, addr) => {
							emit!(REX_W, 0xb8); // movabs rax, ...
							code.emit_imm64_le(*addr as i64);
							emit!(0xff, MOD_REG | 0x2 << 3 | AX); // call rax
						},
						IrLabel::Indirect(table_index, _, _) => {
							let table_offset = offset_map.table(*table_index);
							let stored_func_index_offset = offset_map.vm_data() + codegen::VM_DATA_TMP_0 * 8;
							emit!(REX_W | REX_B, 0x8b, MOD_DISP32 | AX << 3 | R15); // mov rax, [r15+<offset>]
							code.emit_imm32_le(stored_func_index_offset);
							emit!(REX_W | REX_B, 0x8b, MOD_DISP32 | AX << 3 | MOD_SIB, SIB8 | AX << 3 | R15); // mov rax, [r15+rax*8+<offset>]
							code.emit_imm32_le(table_offset);
							emit!(0xff, MOD_REG | 0x2 << 3 | AX); // call rax
						}
						_ => unreachable!()
					}
					if n_params > 0 {
						if n_stack_params > 0 {
							// rsp points to the bottom of the ABI frame. Offsets to the stored
							// rsp and rbp values are known
							emit_with_offset!(REX_W, 0x8b ; BP << 3 | SP, SIB1 | SP << 3 | SP, (n_stack_params + 1) as i32 * 8); // mov rbp, [rsp + storeb_bp_off]
							emit_with_offset!(REX_W, 0x8b ; SP << 3 | SP, SIB1 | SP << 3 | SP, n_stack_params as i32 * 8); // mov rsp, [rsp + storeb_sp_off]
						} else {
							emit!(REX_W | REX_R, 0x89, MOD_REG | R12 << 3 | SP); // mov rsp, r12
						}
						emit!(REX_W, 0x83, MOD_REG | 0x0 << 3 | SP, (n_params as u8) * 8); // add rsp, n_params * 8
					} else {
						emit!(REX_W | REX_R, 0x89, MOD_REG | R12 << 3 | SP); // mov rsp, r12
					}
					if signature.results > 0 {
						emit!(0x50 | AX); // push rax
					}

				},
				Return => {
					emit!(0xc3); // ret near
				}
				Trap => {
					emit!(0x0f, 0x0b); // ud2
				}
				LeadingZeroes(src) => {
					match src {
						Reg32(rsrc) => {
							emit!(REP, 0x0f, 0xbd, MOD_REG | native_reg(rsrc) << 3 | native_reg(rsrc)); // lzcnt <rsrc32>, <rsrc32> ;; (encoded as rep bsr)
						},
						Reg(rsrc) => {
							emit!(REP, REX_W, 0x0f, 0xbd, MOD_REG | native_reg(rsrc) << 3 | native_reg(rsrc)); // lzcnt <rsrc>, <rsrc>
						},
						_ => unreachable!()
					}
				},
				TrailingZeroes(src) => {
					match src {
						Reg32(rsrc) => {
							emit!(REP, 0x0f, 0xbc, MOD_REG | native_reg(rsrc) << 3 | native_reg(rsrc)); // tzcnt <rsrc32>, <rsrc32> ;; (encoded as rep bsf)
						},
						Reg(rsrc) => {
							emit!(REP, REX_W, 0x0f, 0xbc, MOD_REG | native_reg(rsrc) << 3 | native_reg(rsrc)); // tzcnt <rsrc>, <rsrc>
						},
						_ => unreachable!()
					}
				}
				BitPopulationCount(src) => {
					match src {
						Reg32(rsrc) => {
							emit!(REP, 0x0f, 0xb8, MOD_REG | native_reg(rsrc) << 3 | native_reg(rsrc)); // popcnt <rsrc32>, <rsrc32>
						},
						Reg(rsrc) => {
							emit!(REP, REX_W, 0x0f, 0xb8, MOD_REG | native_reg(rsrc) << 3 | native_reg(rsrc)); // popcnt <rsrc32>, <rsrc32>
						},
						_ => unreachable!(),
					}
				},
				InitTablePreamble(offset) => {
					match offset {
						Reg(offset_reg) => {
							emit!(REX_W | REX_B, 0x8d, MOD_DISP32 | DI << 3 | MOD_SIB, SIB8 | native_reg(offset_reg) << 3 | R15); // lea rdi, [r15+<roffset>*8+<offset>]
							code.emit_imm32_le(offset_map.table(0)); // FIXME
							emit!(0xfc); // cld
						},
						_ => todo!()
					}
				},
				InitTableElement(func_index_op) => {
					match func_index_op {
						Imm32(func_index) => {
							emit!(REX_W, 0xb8 | AX); // movabs rax, <imm64> 
							self.abs_off_targets.push(LinkTarget { offset: code.pc(), func_index: *func_index as u32 });
							code.reloc(Relocation::FunctionAbsoluteAddress);
							code.emit_imm64_le(0);
							emit!(REX_W, 0xab); // stosq
						},
						_ => todo!()
					}
				},
				InitTablePostamble => (),
				InitMemoryFromChunk(chunk_idx, chunk_len, offset) => {
					match offset {
						Reg(offset_reg) => {
							emit!(REX_W | REX_B, 0x8d, MOD_RM | DI << 3 | MOD_SIB, SIB1 | native_reg(offset_reg) << 3 | R15); // lea rdi, [r15+<roffset>*1]
						},
						_ => todo!()
					}
					emit!(REX_W | REX_B, 0x8d, MOD_DISP32 | SI << 3 | R15); // lea rsi, [r15+<offset>]
					code.emit_imm32_le(offset_map.data_chunk(*chunk_idx));
					emit!(0xb8 | CX); // mov ecx, <imm32>
					code.emit_imm32_le(*chunk_len as i32);
					emit!(0xfc); // cld
					emit!(0xf3, 0xa4); // rep movsb
				},
				MemoryGrow(pages) => {
					match pages {
						Reg32(rpages) => {
							emit!(REX_W | REX_B, 0x8b, MOD_DISP32 | SI << 3 | R15); // mov rsi, [r15+<offset>]
							code.emit_imm32_le(offset_map.vm_data() + codegen::VM_DATA_MEM_ALLOC);
							emit!(REX_W, 0x89, MOD_REG | SI << 3 | DI); // mov rdi, rsi
							emit!(REX_W, 0x01, MOD_REG | native_reg(rpages) << 3 | SI); // add rsi, <rpages>
							emit!(REX_W | REX_B, 0x3b, MOD_DISP32 | SI << 3 | R15); // cmp rsi, [r15+<offset>]
							code.emit_imm32_le(offset_map.vm_data() + codegen::VM_DATA_MEM_TOTAL);
							emit!(0x77, 0x09); // ja fail
							emit!(REX_W | REX_B, 0x89, MOD_DISP32 | SI << 3 | R15); // mov [r15+<offset>], rsi
							code.emit_imm32_le(offset_map.vm_data() + codegen::VM_DATA_MEM_ALLOC);
							emit!(0xeb, 0x05); // jmp end
							// fail:
							emit!(0xb8 | DI, 0xff, 0xff, 0xff, 0xff); // mov edi, -1
							// end:
							emit!(0x89, MOD_REG | DI << 3 | native_reg(rpages)); // mov <rpages32>, edi
						},
						_ => unreachable!()
					}
				},
				MemorySize(dest) => {
					match dest {
						Reg32(rdest) => {
							emit!(REX_W | REX_B, 0x8b, MOD_DISP32 | native_reg(rdest) << 3 | R15); // mov <rdest>, [r15+<offset>]
							code.emit_imm32_le(offset_map.vm_data() + codegen::VM_DATA_MEM_ALLOC);
						},
						_ => unreachable!()
					}
				}
			}
		}

		for t in jmp_targets {
			if let Some(label_pc) = code.labels.get(&t.1) {
				let insn_pc = t.0 + 4;
				let offset: isize = *label_pc as isize - insn_pc as isize;
				code.patch32_le(t.0, offset as i32);
			} else {
				panic!("Unresolved label: {:?}", t.1)
			}
		}
	}

	fn link(&mut self, code: &mut CodeEmitter) {
		let mut func_offsets: Vec<usize> = Vec::new();
		for (label, offset) in code.labels_iter() {
			match label {
				IrLabel::ExportedFunc(index, _) | IrLabel::AnonymousFunc(index) => {
					if *index as usize >= func_offsets.len() {
						func_offsets.resize(*index as usize + 1, 0);
					}
					func_offsets[*index as usize] = *offset;
				}
				_ => () //todo!()
			}
		}
		println!("OFF {:?}", func_offsets);
		println!("TRG {:?}", self.call_targets);
		for target in &self.call_targets {
			let func_address = func_offsets[target.func_index as usize];
			let insn_pc = target.offset + 4;
			let offset: isize = func_address as isize - insn_pc as isize;
			code.patch32_le(target.offset, offset as i32);
		}
		for target in &self.abs_off_targets {
			let func_address = func_offsets[target.func_index as usize];
			code.patch64_le(target.offset, func_address as i64);
		}
	}
}
