use crate::{CodeGenerator, codegen::CodeEmitter, ir::{Ir, IrReg, IrReg::*, IrCp::*, IrOperand::*, IrLabel}};

pub struct IntelX64Compiler;

const AX: u8 = 0;
const CX: u8 = 1;
const DX: u8 = 2;
const BX: u8 = 3;
const SP: u8 = 4;
const BP: u8 = 5;

const REX_B: u8 = 0x41;
const REX_X: u8 = 0x42;
const REX_R: u8 = 0x44;
const REX_W: u8 = 0x48;

const MOD_REG: u8 = 0xc0;


fn native_reg(r: &IrReg) -> u8 {
	match r {
		Sra => AX,
		Src => CX,
		Srd => DX,
		Ffp => BX,
		Bfp => BP,
		Stp => SP,
		// _ => todo!(),
	}
}

struct JmpTarget(usize, IrLabel);

impl CodeGenerator for IntelX64Compiler {
	fn compile_func(&self, code: &mut CodeEmitter, body: Ir) {
		let mut jmp_targets = Vec::new();

		for insn in body.code() {
			match insn {
				Label(label) => code.label(label.clone()),
				Push(op) => {
					match op {
						Reg(r) => code.emit(0x50 | native_reg(r)), // push <reg>
						Reg8(_) | Reg16(_) | Reg32(_) => unreachable!(),
						// Imm32(imm) => {
						// 	// mov eax, <imm32>
						// 	code.emit(0xb8 | AX);
						// 	code.emit_imm32(imm);
						// 	// push rax
						// 	code.emit(0x50 | AX);
						// },
						// Imm64(imm) => {
						// 	// mov rax, <imm64>
						// 	code.emit(REX_W);
						// 	code.emit(0xb8 | AX);
						// 	code.emit_imm64(imm);
						// 	// push rax
						// 	code.emit(0x50 | AX);
						// },
						_ => todo!()
					}
				},
				Pop(op) => {
					match op {
						Reg(r) => code.emit(0x58 | native_reg(r)), // pop <reg>
						Reg8(_) | Reg16(_) | Reg32(_) => unreachable!(),
						_ => todo!()
					}
				},
				Mov(dest, src) => {
					match (dest, src) {
						(Reg(rdest), Reg(rsrc)) => {
							// mov <dreg>, <sreg>
							code.emit(REX_W);
							code.emit(0x89);
							code.emit(0xc0 | native_reg(rsrc) << 3 | native_reg(rdest));
						},
						(Reg(rdest), Imm32(imm)) => {
							// mov <dreg>, <imm32>
							code.emit(0xb8 | native_reg(rdest));
							code.emit_imm32_le(*imm);
						},
						(Reg(rdest), Imm64(imm)) => {
							// mov <dreg>, <imm64>
							code.emit(REX_W);
							code.emit(0xb8 | native_reg(rdest));
							code.emit_imm64_le(*imm);
						},
						unk => todo!("ir Mov {:?}", unk),
					}
				},
				And(dest, src) => {
					match (dest, src) {
						(Reg32(rdest), Reg32(rsrc)) => {
							// and <dreg>, <sreg>
							code.emit(0x21);
							code.emit(MOD_REG | native_reg(rsrc) << 3 | native_reg(rdest));
						},
						_ => todo!()
					}
				},
				Jmp(label) => {
					// jmp near rel32 (no address just yet)
					code.emit(0xe9);
					jmp_targets.push(JmpTarget(code.pc(), label.clone()));
					code.emit_imm32_le(0);
				}
				Ret => code.emit(0xc3), // ret near
				_ => todo!(),
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
}
