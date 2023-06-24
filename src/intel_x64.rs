use crate::{CodeGenerator, codegen::{CodeEmitter, Relocation}, ir::{Ir, IrReg, IrReg::*, IrCp::*, IrOperand::*, IrCond, IrCond::*, IrLabel, IrSignature}};

pub struct IntelX64Compiler {
	call_targets: Vec<CallTarget>,
}

impl IntelX64Compiler {
	pub fn new() -> Self {
		Self { call_targets: Vec::new() }
	}
}

#[derive(Debug)]
struct CallTarget {
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
const MOD_SIB: u8 = 0x04;

const SIB1: u8 = 0x00;
const SIB2: u8 = 0x40;
const SIB4: u8 = 0x80;
const SIB8: u8 = 0xc0;

const OPER_SIZE_OVR: u8 = 0x66;

const ABI_PARAM_REGS: [(u8, u8); 6] = [(0, DI), (0, SI), (0, DX), (0, CX), (REX_R, R8), (REX_R, R9)];

const fn native_reg(r: &IrReg) -> u8 {
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

const fn native_cond(cond: &IrCond) -> u8 {
	match cond {
		Zero => 0x04,
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

const fn ffp() -> u8 { native_reg(&Ffp) }

struct JmpTarget(usize, IrLabel);

impl CodeGenerator for IntelX64Compiler {
	fn compile_func(&mut self, code: &mut CodeEmitter, index: u32, body: Ir, signatures: &Vec<Option<IrSignature>>) {
		macro_rules! emit {
			($($e:expr),*) => { { $(code.emit($e));* } }
		}

		let mut jmp_targets = Vec::new();
		println!("S {:?}", signatures);
		let self_signature = signatures[index as usize].as_ref().expect("Self signature available");

		for insn in body.code() {
			match insn {
				Label(label) => code.label(label.clone()),
				Preamble => {
					emit!(REX_W, 0xb8 | AX); // movabs rax, imm64
					code.reloc(Relocation::MemoryAbsolute64);
					code.emit_imm64_le(0);

					let temp_storage_off = -0x20000i32;
					emit!(REX_W | REX_R, 0x89, MOD_DISP32 | R12 << 3 | AX); // mov [rax+<offset>], r12
					code.emit_imm32_le(temp_storage_off + 12 * 8);
					emit!(REX_W | REX_R, 0x89, MOD_DISP32 | R15 << 3 | AX); // mov [rax+<offset>], r15
					code.emit_imm32_le(temp_storage_off + 15 * 8);

					emit!(REX_W | REX_B, 0x89, MOD_REG | AX << 3 | R15); // mov r15, rax
				}
				InitLocals(n_locals) => {
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
							// After the last off-stack argument, the return address, old ffp and
							// old bfp were pushed
							let mut caller_frame_off = 24u8;

							for _ in 0..n_stack_params {
								// FIXME: Long offsets
								emit!(REX_W, 0x8b, MOD_DISP8 | native_reg(&Ffp), caller_frame_off); // mov rax, [ffp+off]
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
				},
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
						(Reg(rdest), Imm32(imm)) => {
							// mov <dreg>, <imm32>
							emit!(0xb8 | native_reg(rdest));
							code.emit_imm32_le(*imm);
						},
						(Reg(rdest), Imm64(imm)) => {
							// mov <dreg>, <imm64>
							if *imm > 0 && *imm < u32::MAX as i64 {
								emit!(0xb8 | native_reg(rdest));
								code.emit_imm32_le(*imm as i32);
							} else {
								emit!(REX_W, 0xb8 | native_reg(rdest));
								code.emit_imm64_le(*imm);
							}
						},
						(Reg(rdest), Local(index)) => {
							// mov <dreg>, [ffp-local_off]
							if *index < 15 {
								emit!(REX_W, 0x8b, MOD_DISP8 | native_reg(rdest) << 3 | ffp(), -((*index as i8 + 1) * 8) as u8);
							} else {
								emit!(REX_W, 0x8b, MOD_DISP32 | native_reg(rdest) << 3 | ffp());
								code.emit_imm32_le(-(*index as i32 + 1) * 8);
							}
						},
						(Reg(rdest), Global(index)) => {
							let offset = -0x10000i32 + *index as i32 * 8; // FIXME: Account tables
							emit!(REX_W | REX_B, 0x8b, MOD_DISP32 | native_reg(rdest) << 3 | R15); // mov <rdest>, [r15+<offset>]
							code.emit_imm32_le(offset);
						},
						(Global(index), Reg(rsrc)) => {
							let offset = -0x10000i32 + *index as i32 * 8; // FIXME: Account tables
							emit!(REX_W | REX_B, 0x89, MOD_DISP32 | native_reg(rsrc) << 3 | R15); // mov [r15+<offset>], <rsrc>
							code.emit_imm32_le(offset);
						},
						(Local(index), Reg(rsrc)) => {
							// mov [ffp-local_off], <sreg>
							if *index < 15 {
								emit!(REX_W, 0x89, MOD_DISP8 | native_reg(rsrc) << 3 | ffp(), -((*index as i8 + 1) * 8) as u8);
							} else {
								emit!(REX_W, 0x8b, MOD_DISP32 | native_reg(rsrc) << 3 | ffp());
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
				ZeroExtend(src) => {
					match src {
						Reg8(rsrc) => emit!(0x0f, 0xb6, MOD_REG | native_reg(rsrc) << 3 | native_reg(rsrc)), // movzx <rsrc32>, <rsrc8>
						Reg16(rsrc) => emit!(0x0f, 0xb7, MOD_REG | native_reg(rsrc) << 3 | native_reg(rsrc)), // movzx <rsrc32>, <rsrc16>
						Reg32(_) => (),
						_ => unreachable!(),
					}
				},
				SignExtend(src) => {
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
				Sub(dest, src) => {
					match (dest, src) {
						(Reg32(rdest), Reg32(rsrc)) => emit!(0x29, MOD_REG | native_reg(rsrc) << 3 | native_reg(rdest)), // sub <dreg32>, <sreg32>
						(Reg(rdest), Reg(rsrc)) => emit!(REX_W, 0x29, MOD_REG | native_reg(rsrc) << 3 | native_reg(rdest)), // sub <dreg>, <sreg>
						_ => todo!()
					}
				},
				Compare(cond, dest, src) => {
					match (dest, src) {
						(Reg32(rdest), Reg32(rsrc)) => {
							emit!(0x39, MOD_REG | native_reg(rsrc) << 3 | native_reg(rdest)); // cmp <dreg32>, <sreg32>
							emit!(0x0f, 0x90 | native_cond(cond), MOD_REG | native_reg(rdest)); // setcc <dreg8>
							emit!(0x0f, 0xb6, MOD_REG | native_reg(rdest) << 3 | native_reg(rdest)); // movzx <dreg32>, <dreg8>
						},
						(Reg(rdest), Reg(rsrc)) => {
							emit!(REX_W, 0x39, MOD_REG | native_reg(rsrc) << 3 | native_reg(rdest)); // cmp <dreg32>, <sreg32>
							emit!(0x0f, 0x90 | native_cond(cond), MOD_REG | native_reg(rdest)); // setcc <dreg8>
							emit!(0x0f, 0xb6, MOD_REG | native_reg(rdest) << 3 | native_reg(rdest)); // movzx <dreg32>, <dreg8>
						},
						_ => unreachable!()
					}
				},
				CheckIfZero(src) => {
					match src {
						Reg(rsrc) | Reg32(rsrc) => {
							if matches!(src, Reg(_)) {
								emit!(REX_W)
							}
							emit!(0x85, MOD_REG | native_reg(rsrc) << 3 | native_reg(rsrc)); // test <sreg[32]>, <sreg[32]>
							emit!(0x0f, 0x94 | native_cond(&Zero), MOD_REG | native_reg(rsrc)); // setz <sreg8>
							emit!(0x0f, 0xb6, MOD_REG | native_reg(rsrc) << 3 | native_reg(rsrc)); // movzx <sreg32>, <sreg8>
						},
						_ => unreachable!()
					}
				},
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
				Call(label) => {
					let findex = *match label {
						IrLabel::AnonymousFunc(idx) => idx,
						IrLabel::ExportedFunc(idx, _) => idx,
						IrLabel::ImportedFunc(idx, _addr) => idx,
						_ => unreachable!(),
					} as usize;
					let signature = if let Some(signature) = &signatures[findex] { signature } else { unreachable!() }; 
					let n_params = signature.params;
					let n_stack_params = (n_params as usize).saturating_sub(ABI_PARAM_REGS.len());
					if n_params > 0 {
						let mut sp_off = 8 * (n_params as i8 - 1);
						for i in 0..std::cmp::min(n_params as usize, ABI_PARAM_REGS.len()) {
							// FIXME: With MOD_DISP8, it's max. 16 arguments per function
							emit!(REX_W | ABI_PARAM_REGS[i].0, 0x8b, MOD_DISP8 | ABI_PARAM_REGS[i].1 << 3 | SP, SIB1 | SP << 3 | SP, sp_off as u8); // mov reg, [rsp + sp_off]
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
							// FIXME: DISP8
							emit!(REX_W, 0x89, MOD_DISP8 | SP << 3 | AX, n_stack_params as u8 * 8); // mov [rax + stored_sp_off], rsp
							emit!(REX_W, 0x89, MOD_DISP8 | BP << 3 | AX, (n_stack_params + 1) as u8 * 8); // mov [rax + stored_bp_off], rbp
							emit!(REX_W, 0x89, MOD_REG | AX << 3 | BP); // mov rbp, rax
							emit!(REX_W | REX_B, 0x89, MOD_REG | BP << 3 | R11); // mov r11, rbp
							emit!(REX_W | REX_B, 0x83, MOD_REG | 0x0 << 3 | R11, (n_stack_params - 1) as u8 * 8); // add r11, (nsp-1)*8
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
							self.call_targets.push(CallTarget { offset: code.pc(), func_index: findex as u32 });
							code.emit_imm32_le(0);
						},
						IrLabel::ImportedFunc(_, addr) => {
							emit!(REX_W, 0xb8); // movabs rax, ...
							code.emit_imm64_le(*addr as i64);
							emit!(0xff, MOD_REG | 0x2 << 3 | AX); // call rax
						},
						_ => unreachable!()
					}
					if n_params > 0 {
						if n_stack_params > 0 {
							// rsp points to the bottom of the ABI frame. Offsets to the stored
							// rsp and rbp values are known
							emit!(REX_W, 0x8b, MOD_DISP8 | BP << 3 | SP, SIB1 | SP << 3 | SP, (n_stack_params + 1) as u8 * 8); // mov rbp, [rsp + storeb_bp_off]
							emit!(REX_W, 0x8b, MOD_DISP8 | SP << 3 | SP, SIB1 | SP << 3 | SP, n_stack_params as u8 * 8); // mov rsp, [rsp + storeb_sp_off]
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
				Postamble => {
					let temp_storage_off = -0x20000i32;
					emit!(REX_W | REX_R | REX_B, 0x8b, MOD_DISP32 | R12 << 3 | R15); // mov r12, [r15+<offset>]
					code.emit_imm32_le(temp_storage_off + 12 * 8);
					emit!(REX_W | REX_R | REX_B, 0x8b, MOD_DISP32 | R15 << 3 | R15); // mov r15, [r15+<offset>]
					code.emit_imm32_le(temp_storage_off + 15 * 8);
				}
				Ret => {
					// emit!(REX_B, 0x58 | R15); // pop r15
					emit!(0xc3); // ret near
				}
				// _ => todo!(),
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
			// assert!(func_address > 0); // FIXME! Zero is legit
			let insn_pc = target.offset + 4;
			let offset: isize = func_address as isize - insn_pc as isize;
			code.patch32_le(target.offset, offset as i32);
		}
	}
}
