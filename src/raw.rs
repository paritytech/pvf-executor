use crate::{PvfError, IrPvf};
use crate::ir::{Ir, IrLabel, IrOperand::*, IrReg::*, IrCond::*, IrSignature};
use std::collections::HashMap;
use wasmparser::{Parser, ExternalKind, Type, Payload, Operator as Op, BlockType};

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

pub struct RawPvf {
	wasm_code: Vec<u8>,
	block_index: u64,
}

impl RawPvf {
	pub fn from_bytes(bytes: &[u8]) -> Self {
		Self { wasm_code: Vec::from(&bytes[..]), block_index: 0 }
	}

	pub fn from_file<P: AsRef<std::path::Path>>(path: P) -> Result<Self, PvfError> {
		let wasm_code = std::fs::read(path).map_err(PvfError::FilesystemError)?;
		Ok(Self { wasm_code, block_index: 0 })
	}

	pub fn translate(mut self) -> Result<IrPvf, PvfError> {
	    let mut types = Vec::new();
	    let mut imports;
	    // let mut exports;
	    let mut findex = 0u32;
	    let mut nimports = 0u32;
	    let mut func_export: HashMap<u32, &str> = HashMap::new();
	    // let mut irs = Vec::new();
	    let mut ir_pvf = IrPvf::new();
	    let mut functypes = Vec::new();
	    let mut local_label_index = 0u32;

	    for payload in Parser::new(0).parse_all(&self.wasm_code) {
	        match payload? {
	            Payload::TypeSection(reader) => {
	                types = reader.into_iter().flatten().collect::<Vec<_>>();
	            },
	            Payload::FunctionSection(reader) => {
	            	functypes = reader.into_iter().flatten().collect::<Vec<_>>();
	            },
	            Payload::ImportSection(reader) => {
	                imports = reader.into_iter().flatten().collect::<Vec<_>>();
	                findex = imports.len() as u32;
	                nimports = findex;
	            },
	            Payload::ExportSection(reader) => {
	                // exports = reader.into_iter().collect::<Vec<_>>();
	                for export in reader.into_iter() {
	                    let export = export.unwrap();
	                    if export.kind == ExternalKind::Func {
	                        func_export.insert(export.index, export.name);
	                    }
	                }
	            },
	            Payload::CodeSectionEntry(fbody) => {
	            	let locals_reader = fbody.get_locals_reader()?;
	            	let n_locals = locals_reader.into_iter().flatten().fold(0, |a, (n, _)| a + n);

	                let mut reader = fbody.get_operators_reader()?;
	                let mut ir = Ir::new();
	                let mut cstack = Vec::new();
	                let typeidx = functypes[findex as usize];
	                let Type::Func(ftype) = &types[typeidx as usize];

	                cstack.push(ControlFrame { cftype: ControlFrameType::Func, block_index: 0, has_retval: ftype.results().len() > 0 });

	                ir.label(
	                	if let Some(export) = func_export.get(&findex) {
	                		IrLabel::ExportedFunc(findex, export.to_string())
	                	} else {
	                		IrLabel::AnonymousFunc(findex)
	                	}
	                );
	                ir.push(Reg(Ffp));
	                ir.push(Reg(Bfp));
	                ir.mov(Reg(Ffp), Reg(Stp));
	                ir.mov(Reg(Bfp), Reg(Stp));
	                ir.init_locals(n_locals);

	                while !reader.eof() {
	                    let op = reader.read()?;
	                    match op {
	                        Op::I32Const { value: v } => {
	                            ir.mov(Reg(Sra), Imm32(v));
	                            ir.push(Reg(Sra));
	                        },
	                        Op::I32Add => {
	                        	ir.pop(Reg(Sra));
	                        	ir.pop(Reg(Srd));
	                        	ir.add(Reg32(Sra), Reg32(Srd));
	                        	ir.push(Reg(Sra));
	                        },
	                        Op::I32Sub => {
	                        	ir.pop(Reg(Srd));
	                        	ir.pop(Reg(Sra));
	                        	ir.sub(Reg32(Sra), Reg32(Srd));
	                        	ir.push(Reg(Sra));
	                        },
	                        Op::I32And => {
	                        	ir.pop(Reg(Sra));
	                        	ir.pop(Reg(Srd));
	                        	ir.and(Reg32(Sra), Reg32(Srd));
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
	                        	ir.mov(Reg(Bfp), Reg(Stp));
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
	                        	ir.mov(Reg(Bfp), Reg(Stp));
	                        	ir.label(IrLabel::BranchTarget(self.block_index));
	                        },
	                        Op::Br { relative_depth } | Op::BrIf { relative_depth } => {
	                        	let target_frame = &cstack[cstack.len() - relative_depth as usize - 1];
	                        	let mut else_label = 0;

	                        	if matches!(op, Op::BrIf { .. }) {
	                        		ir.pop(Reg(Sra));
	                        		ir.and(Reg32(Sra), Reg32(Sra));
	                        		else_label = local_label_index;
	                        		ir.jmp_if(Zero, IrLabel::LocalLabel(else_label));
	                        	}

	                        	if target_frame.has_retval {
	                        		ir.pop(Reg(Sra));
	                        	}
	                        	for _ in 0..relative_depth {
	                        		ir.mov(Reg(Stp), Reg(Bfp));
	                        		ir.pop(Reg(Bfp));
	                        	}
	                        	ir.jmp(IrLabel::BranchTarget(target_frame.block_index));

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
	                                        ir.mov(Reg(Stp), Reg(Ffp));
	                                        ir.pop(Reg(Bfp));
	                                        ir.pop(Reg(Ffp));
	                                        ir.ret();
	                                    },
	                                    ControlFrameType::Block | ControlFrameType::Loop => {
	                                    	if frame.has_retval {
	                                    		ir.pop(Reg(Sra));
	                                    	}
	                                    	if matches!(frame.cftype, ControlFrameType::Block) {
	                                    		ir.label(IrLabel::BranchTarget(frame.block_index));
	                                    	}
	                                    	ir.mov(Reg(Stp), Reg(Bfp));
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
	                        	ir.call(IrLabel::AnonymousFunc(function_index));
	                        },
	                        Op::LocalGet { local_index } => {
	                        	ir.mov(Reg(Sra), Local(local_index));
	                        	ir.push(Reg(Sra));
	                        }
	                        Op::LocalSet { local_index } => {
	                        	ir.pop(Reg(Sra));
	                        	ir.mov(Local(local_index), Reg(Sra));
	                        }
	                        Op::LocalTee { local_index } => {
	                        	ir.pop(Reg(Sra));
	                        	ir.mov(Local(local_index), Reg(Sra));
	                        	ir.push(Reg(Sra));
	                        }
	                        unk => todo!("opcode {:?}", unk)
	                    }
	                }

	                let signature = if let Type::Func(signature) = &types[functypes[findex as usize] as usize] {
	                	IrSignature { params: signature.params().len() as u32, results: signature.results().len() as u32 }
	                } else {
	                	unreachable!()
	                };
	                ir_pvf.add_func(findex, ir, signature);
	                findex += 1;
	            },
	            _other => {
	                println!("STUB: Section {:?}", _other);
	            }
	        }
	    }
	    println!("IR: {:?}", ir_pvf);
	    Ok(ir_pvf)
	}
}
