use crate::{PvfError, IrPvf};
use crate::ir::{Ir, IrLabel, IrOperand::*, IrReg::*, IrCond::*, IrSignature, IrHints};
// use std::assert_matches::assert_matches;
use std::collections::HashMap;
use wasmparser::{Parser, ExternalKind, Type, Payload, Operator as Op, BlockType, Import, Encoding, TypeRef, FuncType, OperatorsReader};

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
					init_ir.preamble();
					init_ir.init_locals(0);
				},
				Payload::MemorySection(reader) => {
					hints.has_memory = true;
					let mem = reader.into_iter().next().expect("Memory section contains a single memory entry").expect("Memory section parsed successfully");
					assert!(!mem.memory64);
					assert!(!mem.shared);
					mem_initial = mem.initial as u32;
					mem_max = if let Some(max) = mem.maximum { max as u32 } else { mem_initial };
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
								ir.compare($cond, $reg($dest), $reg($src));
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
					ir.preamble();
					ir.push(Reg(Ffp));
					ir.push(Reg(Bfp));
					ir.r#move(Reg(Ffp), Reg(Stp));
					ir.r#move(Reg(Bfp), Reg(Stp));
					ir.init_locals(n_locals);

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
							Op::I32Add => {
								ir.pop(Reg(Sra));
								ir.pop(Reg(Srd));
								ir.add(Reg32(Sra), Reg32(Srd));
								ir.push(Reg(Sra));
							},
							Op::I64Add => {
								ir.pop(Reg(Sra));
								ir.pop(Reg(Srd));
								ir.add(Reg(Sra), Reg(Srd));
								ir.push(Reg(Sra));
							},
							Op::I32Sub => {
								ir.pop(Reg(Srd));
								ir.pop(Reg(Sra));
								ir.sub(Reg32(Sra), Reg32(Srd));
								ir.push(Reg(Sra));
							},
							Op::I64Sub => {
								ir.pop(Reg(Srd));
								ir.pop(Reg(Sra));
								ir.sub(Reg(Sra), Reg(Srd));
								ir.push(Reg(Sra));
							},
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
								ir.check_if_zero(Reg32(Sra));
								ir.push(Reg(Sra));
							},
							Op::I64Eqz => {
								ir.pop(Reg(Sra));
								ir.check_if_zero(Reg(Sra));
								ir.push(Reg(Sra));
							},
							Op::I32And => {
								ir.pop(Reg(Sra));
								ir.pop(Reg(Srd));
								ir.and(Reg32(Sra), Reg32(Srd));
								ir.push(Reg(Sra));
							},
							Op::I32Or => {
								ir.pop(Reg(Sra));
								ir.pop(Reg(Srd));
								ir.or(Reg32(Sra), Reg32(Srd));
								ir.push(Reg(Sra));
							},
							Op::I32Xor => {
								ir.pop(Reg(Sra));
								ir.pop(Reg(Srd));
								ir.xor(Reg32(Sra), Reg32(Srd));
								ir.push(Reg(Sra));
							},
							Op::I64And => {
								ir.pop(Reg(Sra));
								ir.pop(Reg(Srd));
								ir.and(Reg(Sra), Reg(Srd));
								ir.push(Reg(Sra));
							},
							Op::I64Or => {
								ir.pop(Reg(Sra));
								ir.pop(Reg(Srd));
								ir.or(Reg(Sra), Reg(Srd));
								ir.push(Reg(Sra));
							},
							Op::I64Xor => {
								ir.pop(Reg(Sra));
								ir.pop(Reg(Srd));
								ir.xor(Reg(Sra), Reg(Srd));
								ir.push(Reg(Sra));
							},
							Op::Block { blockty } => {
								self.block_index += 1;
								let has_retval = match blockty {
									BlockType::Empty => false,
									BlockType::Type(_) => true,
									BlockType::FuncType(_) => todo!(),
								};
								cstack.push(ControlFrame { cftype: ControlFrameType::Block, block_index: self.block_index, has_retval });
								ir.push(Reg(Bfp));
								ir.r#move(Reg(Bfp), Reg(Stp));
							},
							Op::Loop { blockty } => {
								self.block_index += 1;
								let has_retval = match blockty {
									BlockType::Empty => false,
									BlockType::Type(_) => true,
									BlockType::FuncType(_) => todo!(),
								};
								cstack.push(ControlFrame { cftype: ControlFrameType::Loop, block_index: self.block_index, has_retval });
								ir.push(Reg(Bfp));
								ir.r#move(Reg(Bfp), Reg(Stp));
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
									ir.r#move(Reg(Stp), Reg(Bfp));
									ir.pop(Reg(Bfp));
								}
								ir.jump(IrLabel::BranchTarget(target_frame.block_index));

								if matches!(op, Op::BrIf { .. }) {
									ir.label(IrLabel::LocalLabel(else_label));
								}
							},
							Op::End => {
								if let Some(frame) = cstack.pop() {
									match frame.cftype {
										ControlFrameType::Func => {
											if ftype.results().len() > 0 {
												ir.pop(Reg(Sra));
											}
											ir.r#move(Reg(Stp), Reg(Ffp));
											ir.pop(Reg(Bfp));
											ir.pop(Reg(Ffp));
											ir.postamble();
											ir.r#return();
										},
										ControlFrameType::Block | ControlFrameType::Loop => {
											if frame.has_retval {
												ir.pop(Reg(Sra));
											}
											if matches!(frame.cftype, ControlFrameType::Block) {
												ir.label(IrLabel::BranchTarget(frame.block_index));
											}
											ir.r#move(Reg(Stp), Reg(Bfp));
											ir.pop(Reg(Bfp));
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
							Op::Unreachable => todo!(),
							Op::Nop => (),
							Op::If { blockty } => todo!(),
							Op::Else => todo!(),
							Op::BrTable { targets } => todo!(),
							Op::Return => {
								if ftype.results().len() > 0 {
									ir.pop(Reg(Sra));
								}
								ir.r#move(Reg(Stp), Reg(Ffp));
								ir.pop(Reg(Bfp));
								ir.pop(Reg(Ffp));
								ir.postamble();
								ir.r#return();
							},
							Op::CallIndirect { type_index, table_index, table_byte } => todo!(),
							Op::Drop => {
								ir.pop(Reg(Sra));
							},
							Op::Select => {
								ir.pop(Reg(Src));
								ir.pop(Reg(Sra));
								ir.pop(Reg(Srd));
								ir.select(Reg32(Src), Reg(Sra), Reg(Srd), Reg(Sra));
								ir.push(Reg(Sra));
							},
							Op::MemorySize { mem, mem_byte } => todo!(),
							Op::MemoryGrow { mem, mem_byte } => todo!(),
							Op::I32Clz => {
								ir.pop(Reg(Sra));
								ir.leading_zeroes(Reg32(Sra));
								ir.push(Reg(Sra));
							},
							Op::I32Ctz => {
								ir.pop(Reg(Sra));
								ir.trailing_zeroes(Reg32(Sra));
								ir.push(Reg(Sra));
							},
							Op::I32Popcnt => {
								ir.pop(Reg(Sra));
								ir.bit_population_count(Reg32(Sra));
								ir.push(Reg(Sra));
							},
							Op::I32Mul => todo!(),
							Op::I32DivS => todo!(),
							Op::I32DivU => todo!(),
							Op::I32RemS => todo!(),
							Op::I32RemU => todo!(),
							Op::I32Shl => todo!(),
							Op::I32ShrS => todo!(),
							Op::I32ShrU => todo!(),
							Op::I32Rotl => todo!(),
							Op::I32Rotr => todo!(),
							Op::I64Clz => {
								ir.pop(Reg(Sra));
								ir.leading_zeroes(Reg(Sra));
								ir.push(Reg(Sra));
							},
							Op::I64Ctz => {
								ir.pop(Reg(Sra));
								ir.trailing_zeroes(Reg(Sra));
								ir.push(Reg(Sra));
							},
							Op::I64Popcnt => {
								ir.pop(Reg(Sra));
								ir.bit_population_count(Reg(Sra));
								ir.push(Reg(Sra));
							},
							Op::I64Mul => todo!(),
							Op::I64DivS => todo!(),
							Op::I64DivU => todo!(),
							Op::I64RemS => todo!(),
							Op::I64RemU => todo!(),
							Op::I64Shl => todo!(),
							Op::I64ShrS => todo!(),
							Op::I64ShrU => todo!(),
							Op::I64Rotl => todo!(),
							Op::I64Rotr => todo!(),
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
				Payload::TableSection(_) => todo!(),
				Payload::TagSection(_) => todo!(),
				Payload::StartSection { func, range } => todo!(),
				Payload::ElementSection(_) => todo!(),
				Payload::DataCountSection { count, range } => todo!(),
				Payload::DataSection(_) => todo!(),
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
				// _other => {
				// 	println!("STUB: Section {:?}", _other);
				// }
			}
		}

		init_ir.postamble();
		init_ir.r#return();
		ir_pvf.add_func(findex, init_ir, IrSignature { params: 0, results: 0 });

		println!("IR: {:?}", ir_pvf);
		Ok(ir_pvf)
	}
}
