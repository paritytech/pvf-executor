use crate::{PvfError, IrPvf};
use crate::ir::{Ir, IrLabel, IrOperand::*, IrReg::*};
use std::collections::HashMap;
use wasmparser::{Parser, ExternalKind, Type, Payload, Operator as Op};

enum ControlFrameType {
    Func,
    Block,
    Loop,
    If,
}

struct ControlFrame {
    cftype: ControlFrameType
}

pub struct RawPvf {
	wasm_code: Vec<u8>
}

impl RawPvf {
	pub fn from_bytes(bytes: &[u8]) -> Self {
		Self { wasm_code: Vec::from(&bytes[..]) }
	}

	pub fn from_file<P: AsRef<std::path::Path>>(path: P) -> Result<Self, PvfError> {
		let wasm_code = std::fs::read(path).map_err(PvfError::FilesystemError)?;
		Ok(Self { wasm_code })
	}

	pub fn translate(self) -> Result<IrPvf, PvfError> {
	    let mut types = Vec::new();
	    let mut imports;
	    // let mut exports;
	    let mut findex = 0u32;
	    let mut nimports = 0u32;
	    let mut func_export: HashMap<u32, &str> = HashMap::new();
	    // let mut irs = Vec::new();
	    let mut ir_pvf = IrPvf::new();

	    for payload in Parser::new(0).parse_all(&self.wasm_code) {
	        match payload? {
	            Payload::TypeSection(reader) => {
	                types = reader.into_iter().flatten().collect::<Vec<_>>();
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
	                let mut reader = fbody.get_operators_reader()?;
	                let mut ir = Ir::new();
	                let mut cstack = Vec::new();

	                cstack.push(ControlFrame { cftype: ControlFrameType::Func });

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

	                let Type::Func(ftype) = &types[findex as usize];

	                while !reader.eof() {
	                    let op = reader.read()?;
	                    match op {
	                        Op::I32Const { value: v } => {
	                            ir.mov(Reg(Sra), Imm32(v));
	                            ir.push(Reg(Sra));
	                        },
	                        Op::I32And => {
	                        	ir.pop(Reg(Sra));
	                        	ir.pop(Reg(Src));
	                        	ir.and(Reg32(Sra), Reg32(Src));
	                        	ir.push(Reg(Sra));
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
	                                    _ => todo!()        
	                                }
	                            }
	                        },
	                        unk => todo!("opcode {:?}", unk)
	                    }
	                }

	                // irs.push(ir);
	                ir_pvf.add_func(findex, ir);
	                findex += 1;
	            },
	            _other => {
	                println!("STUB: Section {:?}", _other);
	            }
	        }
	    }
	    Ok(ir_pvf)
	}
}
