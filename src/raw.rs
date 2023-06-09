use crate::{PvfError, IrPvf};
use crate::ir::{Ir, IrLabel, IrOperand::*, IrReg::*, IrCond::*, IrSignature, IrHints};
// use std::assert_matches::assert_matches;
use std::collections::HashMap;
use wasmparser::{Parser, ExternalKind, Type, Payload, Operator as Op, BlockType, Import, Encoding, TypeRef, TableInit, FuncType, OperatorsReader, ElementKind, ElementItems, DataKind};

enum ControlFrameType {
	Func,
	Block,
	Loop,
	If,
}

struct ControlFrame {
	cftype: ControlFrameType,
	block_index: u64,
	has_retval: bool,
}

type ImportResolver = fn(&str, &str, &Type) -> Result<*const u8, PvfError>;

enum GlobalRef {
	Own { init_ir: Ir },
	Imported,
}

fn parse_const_expr(mut reader: OperatorsReader, globals: &Vec<GlobalRef>) -> Result<Ir, PvfError> {
	let mut ir = Ir::new();
	while !reader.eof() {
		let op = reader.read()?;
		match op {
			Op::I32Const { value: v } => {
				ir.r#move(Reg(Sra), Imm32(v));
				ir.push(Reg(Sra));
			},
			Op::I64Const { value: v } => {
				ir.r#move(Reg(Sra), Imm64(v));
				ir.push(Reg(Sra));
			},
			Op::End => return Ok(ir),
			_ => todo!() // global.get is allowed, but for imported constants only
		}
	}

	Err(PvfError::ValidationError("Constant expression must end with `end` opcode".to_owned()))
}

pub struct RawPvf {
	wasm_code: Vec<u8>,
	block_index: u64,
	import_resolver: Option<ImportResolver>,
}

impl RawPvf {
	pub fn from_bytes(bytes: &[u8]) -> Self {
		Self { wasm_code: Vec::from(&bytes[..]), block_index: 0, import_resolver: None }
	}

	pub fn from_file<P: AsRef<std::path::Path>>(path: P) -> Result<Self, PvfError> {
		let wasm_code = std::fs::read(path).map_err(PvfError::FilesystemError)?;
		Ok(Self { wasm_code, block_index: 0, import_resolver: None })
	}

	pub fn set_import_resolver(&mut self, resolver: ImportResolver) {
		self.import_resolver = Some(resolver);
	}

	pub fn translate(mut self) -> Result<IrPvf, PvfError> {
		let mut types = Vec::new();
		// let mut imports;
		// let mut exports;
		let mut findex = 0u32;
		let mut nimports = 0u32;
		let mut func_imports = Vec::new();
		let mut func_export: HashMap<u32, &str> = HashMap::new();
		// let mut irs = Vec::new();
		let mut ir_pvf = IrPvf::new();
		let mut init_ir = Ir::new();
		let mut functypes = Vec::new();
		let mut local_label_index = 0u32;
		let mut mem_initial = 0;
		let mut mem_max = 0;
		let mut globals = Vec::new();
		let mut hints = IrHints::default();
		let mut data_chunk_cnt = 0;

		for payload in Parser::new(0).parse_all(&self.wasm_code) {
			match payload? {
				Payload::TypeSection(reader) => {
					types = reader.into_iter().flatten().collect::<Vec<_>>();
					println!("TYPES {:?}", types);
				},
				Payload::ImportSection(reader) => {
					for import in reader.into_iter() {
						let import = import.unwrap();
						match import.ty {
							TypeRef::Func(ti) => {
								if let Some(resolver) = self.import_resolver {
									let funcref = resolver(import.module, import.name, &types[ti as usize]).map_err(|_| PvfError::UnresolvedImport(import.module.to_owned() + "::" + import.name))?;
									let Type::Func(functype) = &types[ti as usize];
									let signature = IrSignature { params: functype.params().len() as u32, results: functype.results().len() as u32 };
									ir_pvf.add_func_import(findex, funcref, signature);
									func_imports.push(funcref);
									functypes.push(ti);
									nimports = func_imports.len() as u32;
									findex = nimports;
								} else {
									panic!("Import is requested but no import resolver was specified");
								}
							},
							TypeRef::Global(_) => {
								globals.push(GlobalRef::Imported)
							},
							_ => todo!()
						}
					}
				},
				Payload::FunctionSection(reader) => {
					functypes.extend(reader.into_iter().flatten());
					println!("FUNCTYPES {:?}", functypes);
					let init_index = functypes.len();
					init_ir.label(IrLabel::ExportedFunc(init_index as u32, "_pvf_init".to_owned()));
					init_ir.enter_function(0);
				},
				Payload::MemorySection(reader) => {
					hints.has_memory = true;
					let mem = reader.into_iter().next().expect("Memory section contains a single memory entry").expect("Memory section parsed successfully");
					assert!(!mem.memory64);
					assert!(!mem.shared);
					mem_initial = mem.initial as u32;
					mem_max = if let Some(max) = mem.maximum { max as u32 } else { mem_initial + 128 };
					ir_pvf.set_memory(mem_initial, mem_max);
				}
				Payload::ExportSection(reader) => {
					for export in reader.into_iter() {
						let export = export.unwrap();
						if export.kind == ExternalKind::Func {
							func_export.insert(export.index, export.name);
						}
					}
				},
				Payload::GlobalSection(reader) => {
					hints.has_globals = true;
					for global in reader.into_iter() {
						let global = global.unwrap();
						let global_init_ir = parse_const_expr(global.init_expr.get_operators_reader(), &globals )?;
						init_ir.append(&mut global_init_ir.clone());
						init_ir.pop(Reg(Sra));
						init_ir.r#move(Global(globals.len() as u32), Reg(Sra));
						globals.push(GlobalRef::Own { init_ir: global_init_ir })
					}
				}
				Payload::CodeSectionEntry(fbody) => {
					let locals_reader = fbody.get_locals_reader()?;
					let n_locals = locals_reader.into_iter().flatten().fold(0, |a, (n, _)| a + n);

					let mut reader = fbody.get_operators_reader()?;
					let mut ir = Ir::new();
					let mut cstack = Vec::new();
					let typeidx = functypes[findex as usize];
					let Type::Func(ftype) = &types[typeidx as usize];

					macro_rules! impl_compare {
						($cond:expr, $reg:ident, $dest:expr, $src:expr) => {
							{
								ir.pop(Reg($src));
								ir.pop(Reg($dest));
								ir.compare($reg($dest), $reg($src));
								ir.set_if($cond, $reg($dest));
								ir.push(Reg($dest));
							}
						};
					}

					macro_rules! impl_unary {
						($reg:ident, $src:expr, $op:ident) => {
							{
								ir.pop(Reg($src));
								ir.$op($reg($src));
								ir.push(Reg($src));
							}
						};
					}

					// Commutative ops are better optimized if their arguments are changed places
					macro_rules! impl_comm_binary {
						($reg:ident, $dest:expr, $src:expr, $op:ident) => {
							{
								ir.pop(Reg($dest));
								ir.pop(Reg($src));
								ir.$op($reg($dest), $reg($src));
								ir.push(Reg($dest));
							}
						};
					}

					macro_rules! impl_noncomm_binary {
						($reg:ident, $dest:expr, $src:expr, $op:ident) => {
							{
								ir.pop(Reg($src));
								ir.pop(Reg($dest));
								ir.$op($reg($dest), $reg($src));
								ir.push(Reg($dest));
							}
						};
					}

					cstack.push(ControlFrame { cftype: ControlFrameType::Func, block_index: 0, has_retval: ftype.results().len() > 0 });

					ir.label(
						if let Some(export) = func_export.get(&findex) {
							IrLabel::ExportedFunc(findex, export.to_string())
						} else {
							IrLabel::AnonymousFunc(findex)
						}
					);
					ir.enter_function(n_locals);

					while !reader.eof() {
						let op = reader.read()?;
						match op {
							Op::I32Const { value: v } => {
								ir.r#move(Reg(Sra), Imm32(v));
								ir.push(Reg(Sra));
							},
							Op::I64Const { value: v } => {
								ir.r#move(Reg(Sra), Imm64(v));
								ir.push(Reg(Sra));
							}
							Op::I32Add => impl_comm_binary!(Reg32, Sra, Srd, add),
							Op::I64Add => impl_comm_binary!(Reg, Sra, Srd, add),
							Op::I32Sub => impl_noncomm_binary!(Reg32, Sra, Srd, subtract),
							Op::I64Sub => impl_noncomm_binary!(Reg, Sra, Srd, subtract),
							Op::I32Eq =>  impl_compare!(Equal, Reg32, Sra, Srd),
							Op::I32Ne =>  impl_compare!(NotEqual, Reg32, Sra, Srd),
							Op::I32LtS => impl_compare!(LessSigned, Reg32, Sra, Srd),
							Op::I32LtU => impl_compare!(LessUnsigned, Reg32, Sra, Srd),
							Op::I32GtS => impl_compare!(GreaterSigned, Reg32, Sra, Srd),
							Op::I32GtU => impl_compare!(GreaterUnsigned, Reg32, Sra, Srd),
							Op::I32LeS => impl_compare!(LessOrEqualSigned, Reg32, Sra, Srd),
							Op::I32LeU => impl_compare!(LessOrEqualUnsigned, Reg32, Sra, Srd),
							Op::I32GeS => impl_compare!(GreaterOrEqualSigned, Reg32, Sra, Srd),
							Op::I32GeU => impl_compare!(GreaterOrEqualUnsigned, Reg32, Sra, Srd),
							Op::I64Eq =>  impl_compare!(Equal, Reg, Sra, Srd),
							Op::I64Ne =>  impl_compare!(NotEqual, Reg, Sra, Srd),
							Op::I64LtS => impl_compare!(LessSigned, Reg, Sra, Srd),
							Op::I64LtU => impl_compare!(LessUnsigned, Reg, Sra, Srd),
							Op::I64GtS => impl_compare!(GreaterSigned, Reg, Sra, Srd),
							Op::I64GtU => impl_compare!(GreaterUnsigned, Reg, Sra, Srd),
							Op::I64LeS => impl_compare!(LessOrEqualSigned, Reg, Sra, Srd),
							Op::I64LeU => impl_compare!(LessOrEqualUnsigned, Reg, Sra, Srd),
							Op::I64GeS => impl_compare!(GreaterOrEqualSigned, Reg, Sra, Srd),
							Op::I64GeU => impl_compare!(GreaterOrEqualUnsigned, Reg, Sra, Srd),
							Op::I32Eqz => {
								ir.pop(Reg(Sra));
								ir.and(Reg32(Sra), Reg32(Sra));
								ir.set_if(Zero, Reg32(Sra));
								ir.push(Reg(Sra));
							}
							Op::I64Eqz => {
								ir.pop(Reg(Sra));
								ir.and(Reg(Sra), Reg(Sra));
								ir.set_if(Zero, Reg32(Sra));
								ir.push(Reg(Sra));
							}
							Op::I32And => impl_comm_binary!(Reg32, Sra, Srd, and),
							Op::I32Or => impl_comm_binary!(Reg32, Sra, Srd, or),
							Op::I32Xor => impl_comm_binary!(Reg32, Sra, Srd, xor),
							Op::I64And => impl_comm_binary!(Reg, Sra, Srd, and),
							Op::I64Or => impl_comm_binary!(Reg, Sra, Srd, or),
							Op::I64Xor => impl_comm_binary!(Reg, Sra, Srd, xor),
							Op::Block { blockty } => {
								self.block_index += 1;
								let has_retval = match blockty {
									BlockType::Empty => false,
									BlockType::Type(_) => true,
									BlockType::FuncType(_) => todo!(),
								};
								cstack.push(ControlFrame { cftype: ControlFrameType::Block, block_index: self.block_index, has_retval });
								ir.enter_block();
							},
							Op::Loop { blockty } => {
								self.block_index += 1;
								let has_retval = match blockty {
									BlockType::Empty => false,
									BlockType::Type(_) => true,
									BlockType::FuncType(_) => todo!(),
								};
								cstack.push(ControlFrame { cftype: ControlFrameType::Loop, block_index: self.block_index, has_retval });
								ir.enter_block();
								ir.label(IrLabel::BranchTarget(self.block_index));
							},
							Op::Br { relative_depth } | Op::BrIf { relative_depth } => {
								let target_frame = &cstack[cstack.len() - relative_depth as usize - 1];
								let mut else_label = 0;

								if matches!(op, Op::BrIf { .. }) {
									ir.pop(Reg(Sra));
									ir.and(Reg32(Sra), Reg32(Sra));
									else_label = local_label_index;
									local_label_index += 1;
									ir.jump_if(Zero, IrLabel::LocalLabel(else_label));
								}

								if target_frame.has_retval {
									ir.pop(Reg(Sra));
								}
								for _ in 0..relative_depth {
									ir.leave_block();
								}
								ir.jump(IrLabel::BranchTarget(target_frame.block_index));

								if matches!(op, Op::BrIf { .. }) {
									ir.label(IrLabel::LocalLabel(else_label));
								}
							},
							Op::BrTable { targets } => {
								let default_frame = &cstack[cstack.len() - targets.default() as usize - 1];
								let mut br_targets = targets.targets().collect::<Result<Vec<_>, _>>()?;
								br_targets.push(targets.default());
								ir.pop(Reg(Srd)); // Branch target index
								ir.r#move(Reg32(Sra), Imm32(br_targets.len() as i32 - 1));
								ir.compare(Reg32(Srd), Reg32(Sra));
								ir.move_if(GreaterUnsigned, Reg32(Srd), Reg32(Sra));

								if default_frame.has_retval {
									ir.pop(Reg(Sra));
								}

								let mut exit_labels = Vec::new();
								for _ in 0..br_targets.len() {
									exit_labels.push(IrLabel::LocalLabel(local_label_index));
									local_label_index += 1;
								}

								ir.jump_table(Reg32(Srd), exit_labels.clone());

								for (i, target) in br_targets.iter().enumerate() {
									let frame = &cstack[cstack.len() - *target as usize - 1];
									ir.label(exit_labels[i].clone());
									for _ in 0..*target {
										ir.leave_block();
									}
									ir.jump(IrLabel::BranchTarget(frame.block_index));
								}
							},
							Op::End => {
								if let Some(frame) = cstack.pop() {
									match frame.cftype {
										ControlFrameType::Func => {
											if ftype.results().len() > 0 {
												ir.pop(Reg(Sra));
											}
											ir.leave_function();
											ir.r#return();
										},
										ControlFrameType::Block | ControlFrameType::Loop => {
											if frame.has_retval {
												ir.pop(Reg(Sra));
											}
											if matches!(frame.cftype, ControlFrameType::Block) {
												ir.label(IrLabel::BranchTarget(frame.block_index));
											}
											ir.leave_block();
											if frame.has_retval {
												ir.push(Reg(Sra));
											}
										},
										_ => todo!()
									}
								} else {
									unreachable!();
								}
							},
							Op::Call { function_index } => {
								ir.call(if function_index < nimports {
									IrLabel::ImportedFunc(function_index, func_imports[function_index as usize])
								} else {
									IrLabel::AnonymousFunc(function_index)
								});
							},
							Op::LocalGet { local_index } => {
								ir.r#move(Reg(Sra), Local(local_index));
								ir.push(Reg(Sra));
							}
							Op::LocalSet { local_index } => {
								ir.pop(Reg(Sra));
								ir.r#move(Local(local_index), Reg(Sra));
							}
							Op::LocalTee { local_index } => {
								ir.pop(Reg(Sra));
								ir.r#move(Local(local_index), Reg(Sra));
								ir.push(Reg(Sra));
							},
							Op::GlobalGet { global_index } => {
								ir.r#move(Reg(Sra), Global(global_index));
								ir.push(Reg(Sra));
							}
							Op::GlobalSet { global_index } => {
								ir.pop(Reg(Sra));
								ir.r#move(Global(global_index), Reg(Sra));
							}
							Op::I32Load { memarg } => {
								ir.pop(Reg(Srd));
								ir.r#move(Reg32(Sra), Memory32(memarg.offset as i32, Srd));
								ir.zero_extend(Reg32(Sra));
								ir.push(Reg(Sra));
							},
							Op::I64Load { memarg } => {
								ir.pop(Reg(Srd));
								ir.r#move(Reg(Sra), Memory64(memarg.offset as i32, Srd));
								ir.push(Reg(Sra));
							},
							Op::I32Load8U { memarg } => {
								ir.pop(Reg(Srd));
								ir.r#move(Reg8(Sra), Memory8(memarg.offset as i32, Srd));
								ir.zero_extend(Reg8(Sra));
								ir.push(Reg(Sra));
							}
							Op::I32Load8S { memarg } => {
								ir.pop(Reg(Srd));
								ir.r#move(Reg8(Sra), Memory8(memarg.offset as i32, Srd));
								ir.sign_extend(Reg8(Sra));
								ir.push(Reg(Sra));
							}
							Op::I32Load16S { memarg } => {
								ir.pop(Reg(Srd));
								ir.r#move(Reg16(Sra), Memory16(memarg.offset as i32, Srd));
								ir.sign_extend(Reg16(Sra));
								ir.push(Reg(Sra));
							},
							Op::I32Load16U { memarg } => {
								ir.pop(Reg(Srd));
								ir.r#move(Reg16(Sra), Memory16(memarg.offset as i32, Srd));
								ir.zero_extend(Reg16(Sra));
								ir.push(Reg(Sra));
							},
							Op::I64Load8S { memarg } => {
								ir.pop(Reg(Srd));
								ir.r#move(Reg8(Sra), Memory8(memarg.offset as i32, Srd));
								ir.sign_extend(Reg8(Sra));
								ir.push(Reg(Sra));
							},
							Op::I64Load8U { memarg } => {
								ir.pop(Reg(Srd));
								ir.r#move(Reg8(Sra), Memory8(memarg.offset as i32, Srd));
								ir.zero_extend(Reg8(Sra));
								ir.push(Reg(Sra));
							},
							Op::I64Load16S { memarg } => {
								ir.pop(Reg(Srd));
								ir.r#move(Reg16(Sra), Memory16(memarg.offset as i32, Srd));
								ir.sign_extend(Reg16(Sra));
								ir.push(Reg(Sra));
							},
							Op::I64Load16U { memarg } => {
								ir.pop(Reg(Srd));
								ir.r#move(Reg16(Sra), Memory16(memarg.offset as i32, Srd));
								ir.zero_extend(Reg16(Sra));
								ir.push(Reg(Sra));
							},
							Op::I64Load32S { memarg } => {
								ir.pop(Reg(Srd));
								ir.r#move(Reg32(Sra), Memory32(memarg.offset as i32, Srd));
								ir.sign_extend(Reg32(Sra));
								ir.push(Reg(Sra));
							},
							Op::I64Load32U { memarg } => {
								ir.pop(Reg(Srd));
								ir.r#move(Reg32(Sra), Memory32(memarg.offset as i32, Srd));
								ir.zero_extend(Reg32(Sra));
								ir.push(Reg(Sra));
							},
							Op::I64Store { memarg } => {
								ir.pop(Reg(Sra));
								ir.pop(Reg(Srd));
								ir.r#move(Memory64(memarg.offset as i32, Srd), Reg(Sra));
							},
							Op::I32Store8 { memarg } | Op::I64Store8 { memarg }=> {
								ir.pop(Reg(Sra));
								ir.pop(Reg(Srd));
								ir.r#move(Memory8(memarg.offset as i32, Srd), Reg8(Sra));
							},
							Op::I32Store16 { memarg } | Op::I64Store16 { memarg }=> {
								ir.pop(Reg(Sra));
								ir.pop(Reg(Srd));
								ir.r#move(Memory16(memarg.offset as i32, Srd), Reg16(Sra));
							},
							Op::I32Store { memarg } | Op::I64Store32 { memarg }=> {
								ir.pop(Reg(Sra));
								ir.pop(Reg(Srd));
								ir.r#move(Memory32(memarg.offset as i32, Srd), Reg32(Sra));
							},
							Op::Unreachable => ir.trap(),
							Op::Nop => (),
							Op::If { blockty } => todo!(),
							Op::Else => todo!(),
							Op::Return => {
								if ftype.results().len() > 0 {
									ir.pop(Reg(Sra));
								}
								ir.leave_function();
								ir.r#return();
							},
							Op::CallIndirect { type_index, table_index, table_byte } => {
								assert_eq!(table_byte, 0); // Reference types are not supported yet
								ir.pop(Reg(Sra));
								let Type::Func(functype) = &types[type_index as usize];
								let signature = IrSignature { params: functype.params().len() as u32, results: functype.results().len() as u32 };
								ir.call(IrLabel::Indirect(table_index, Reg32(Sra), signature));
							},
							Op::Drop => {
								ir.pop(Reg(Sra));
							},
							Op::Select => {
								ir.pop(Reg(Sra));
								ir.and(Reg(Sra), Reg(Sra));
								ir.pop(Reg(Sra));
								ir.pop(Reg(Srd));
								ir.move_if(NotZero, Reg(Sra), Reg(Srd));
								ir.push(Reg(Sra));
							},
							Op::MemorySize { mem: _, mem_byte } => {
								assert_eq!(mem_byte, 0);
								ir.memory_size(Reg32(Sra));
								ir.push(Reg(Sra));
							},
							Op::MemoryGrow { mem: _, mem_byte } => {
								assert_eq!(mem_byte, 0);
								ir.pop(Reg(Sra));
								ir.memory_grow(Reg32(Sra));
								ir.push(Reg(Sra));
							},
							Op::I32Clz => impl_unary!(Reg32, Sra, leading_zeroes),
							Op::I32Ctz => impl_unary!(Reg32, Sra, trailing_zeroes),
							Op::I32Popcnt => impl_unary!(Reg32, Sra, bit_population_count),
							Op::I32Mul => impl_comm_binary!(Reg32, Sra, Srd, multiply),
							Op::I32DivS => impl_noncomm_binary!(Reg32, Sra, Src, divide_signed),
							Op::I32DivU => impl_noncomm_binary!(Reg32, Sra, Src, divide_unsigned),
							Op::I32RemS => impl_noncomm_binary!(Reg32, Sra, Src, remainder_signed),
							Op::I32RemU => impl_noncomm_binary!(Reg32, Sra, Src, remainder_unsigned),
							Op::I32Shl => impl_noncomm_binary!(Reg32, Sra, Src, shift_left),
							Op::I32ShrU => impl_noncomm_binary!(Reg32, Sra, Src, shift_right_unsigned),
							Op::I32ShrS => impl_noncomm_binary!(Reg32, Sra, Src, shift_right_signed),
							Op::I32Rotl => impl_noncomm_binary!(Reg32, Sra, Src, rotate_left),
							Op::I32Rotr => impl_noncomm_binary!(Reg32, Sra, Src, rotate_right),
							Op::I64Clz => impl_unary!(Reg, Sra, leading_zeroes),
							Op::I64Ctz => impl_unary!(Reg, Sra, trailing_zeroes),
							Op::I64Popcnt => impl_unary!(Reg, Sra, bit_population_count),
							Op::I64Mul => impl_comm_binary!(Reg, Sra, Srd, multiply),
							Op::I64DivS => impl_noncomm_binary!(Reg, Sra, Src, divide_signed),
							Op::I64DivU => impl_noncomm_binary!(Reg, Sra, Src, divide_unsigned),
							Op::I64RemS => impl_noncomm_binary!(Reg, Sra, Src, remainder_signed),
							Op::I64RemU => impl_noncomm_binary!(Reg, Sra, Src, remainder_unsigned),
							Op::I64Shl => impl_noncomm_binary!(Reg, Sra, Src, shift_left),
							Op::I64ShrU => impl_noncomm_binary!(Reg, Sra, Src, shift_right_unsigned),
							Op::I64ShrS => impl_noncomm_binary!(Reg, Sra, Src, shift_right_signed),
							Op::I64Rotl => impl_noncomm_binary!(Reg, Sra, Src, rotate_left),
							Op::I64Rotr => impl_noncomm_binary!(Reg, Sra, Src, rotate_right),
							Op::I32WrapI64 | Op::I64ExtendI32U => {
								ir.pop(Reg(Sra));
								ir.zero_extend(Reg32(Sra));
								ir.push(Reg(Sra));
							},
							Op::I64ExtendI32S => {
								ir.pop(Reg(Sra));
								ir.sign_extend(Reg32(Sra));
								ir.push(Reg(Sra));
							},
							unk => todo!("opcode {:?}", unk)
						}
					}

					let Type::Func(signature) = &types[functypes[findex as usize] as usize];
					let signature = IrSignature { params: signature.params().len() as u32, results: signature.results().len() as u32 };
					ir_pvf.add_func(findex, ir, signature);
					findex += 1;
				},
				Payload::Version { num, encoding, range } => {
					assert_eq!(num, 1);
					if !matches!(encoding, Encoding::Module) {
						panic!("Only modules are supported");
					}
				},
				Payload::TableSection(reader) => {
					for table in reader.into_iter() {
						let table = table.unwrap();
						if !matches!(table.init, TableInit::RefNull) {
							todo!("Table initialization mode {:?}", table.init);
						}
						let table_size = if let Some(maximum) = table.ty.maximum { maximum } else { table.ty.initial };
						ir_pvf.add_table(table_size);
					}
				},
				Payload::TagSection(_) => todo!(),
				Payload::StartSection { func, range } => todo!(),
				Payload::ElementSection(reader) => {
					for element in reader.into_iter() {
						let element = element.unwrap();
						if let ElementKind::Active { table_index, offset_expr } = element.kind {
							let mut init_offset_ir = parse_const_expr(offset_expr.get_operators_reader(), &globals)?;
							init_ir.append(&mut init_offset_ir);
							init_ir.pop(Reg(Sra));
							init_ir.init_table_preamble(Reg(Sra));
							if let ElementItems::Functions(reader) = element.items {
								for function_index in reader.into_iter() {
									let function_index = function_index.unwrap();
									init_ir.init_table_element(Imm32(function_index as i32));
								}
							} else {
								todo!("Element items are not functions");
							}
							init_ir.init_table_postamble();
						} else {
							todo!("Element kind is not active");
						}
					}
				},
				Payload::DataSection(reader) => {
					for data in reader.into_iter() {
						let data = data.unwrap();
						if let DataKind::Active { memory_index, offset_expr } = data.kind {
							assert_eq!(memory_index, 0); // WASM MVP only supports single memory

							ir_pvf.add_data_chunk(data.data);

							let mut init_offset_ir = parse_const_expr(offset_expr.get_operators_reader(), &globals)?;
							init_ir.append(&mut init_offset_ir);
							init_ir.pop(Reg(Sra));
							init_ir.init_memory_from_chunk(data_chunk_cnt, data.data.len() as u32, Reg(Sra));
							data_chunk_cnt += 1;
						} else {
							todo!("Passive data segment");
						}
					}
				},
				Payload::DataCountSection { count, range } => todo!(),
				Payload::CodeSectionStart { count, range, size } => (), // FIXME
				Payload::ModuleSection { parser, range } => todo!(),
				Payload::InstanceSection(_) => todo!(),
				Payload::CoreTypeSection(_) => todo!(),
				Payload::ComponentSection { parser, range } => todo!(),
				Payload::ComponentInstanceSection(_) => todo!(),
				Payload::ComponentAliasSection(_) => todo!(),
				Payload::ComponentTypeSection(_) => todo!(),
				Payload::ComponentCanonicalSection(_) => todo!(),
				Payload::ComponentStartSection { start, range } => todo!(),
				Payload::ComponentImportSection(_) => todo!(),
				Payload::ComponentExportSection(_) => todo!(),
				Payload::CustomSection(_) => (),
				Payload::UnknownSection { id, contents, range } => todo!(),
				Payload::End(_) => (), // FIXME
			}
		}

		init_ir.leave_function();
		init_ir.r#return();
		ir_pvf.add_func(findex, init_ir, IrSignature { params: 0, results: 0 });

		println!("IR: {:?}", ir_pvf);
		Ok(ir_pvf)
	}
}
