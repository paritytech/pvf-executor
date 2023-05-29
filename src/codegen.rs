use std::collections::HashMap;
use crate::ir::{Ir, IrLabel};

pub struct CodeEmitter {
	pub(crate) code: Vec<u8>,
	pub(crate) labels: HashMap<IrLabel, usize>,
}

impl CodeEmitter {
	pub(crate) fn new() -> Self {
		Self { code: Vec::new(), labels: HashMap::new() }
	}

	pub(crate) fn emit(&mut self, b: u8) {
		self.code.push(b);
	}

	pub(crate) fn emit_imm32_le(&mut self, imm: i32) {
		imm.to_le_bytes().into_iter().for_each(|b| self.code.push(b))
	}

	pub(crate) fn emit_imm64_le(&mut self, imm: i64) {
		imm.to_le_bytes().into_iter().for_each(|b| self.code.push(b))
	}

	pub(crate) fn patch32_le(&mut self, pos: usize, imm: i32) {
		self.code[pos..pos+4].copy_from_slice(&imm.to_le_bytes()[..])
	}

	pub(crate) fn label(&mut self, label: IrLabel) {
		self.labels.insert(label, self.code.len());
	}

	pub(crate) fn pc(&self) -> usize {
		self.code.len()
	}
}

// fn resolve_func_label(findex: u32) -> IrLabel {

// }

pub trait CodeGenerator {
	fn compile_func(&self, code: &mut CodeEmitter, body: Ir);
}
