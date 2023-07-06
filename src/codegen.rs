use std::collections::HashMap;
use crate::ir::{Ir, IrLabel, IrSignature, IrTable};

pub enum Relocation {
	MemoryAbsolute64,
}

pub struct CodeEmitter {
	pub(crate) code: Vec<u8>,
	pub(crate) labels: HashMap<IrLabel, usize>,
	pub(crate) relocs: Vec<(Relocation, usize)>,
}

impl CodeEmitter {
	pub(crate) fn new() -> Self {
		Self { code: Vec::new(), labels: HashMap::new(), relocs: Vec::new() }
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

	pub(crate) fn patch64_le(&mut self, pos: usize, imm: i64) {
		self.code[pos..pos+8].copy_from_slice(&imm.to_le_bytes()[..])
	}

	pub(crate) fn label(&mut self, label: IrLabel) {
		self.labels.insert(label, self.code.len());
	}

	pub(crate) fn reloc(&mut self, reloc: Relocation) {
		self.relocs.push((reloc, self.code.len()));
	}

	pub(crate) fn pc(&self) -> usize {
		self.code.len()
	}

	pub(crate) fn labels_iter(&self) -> std::collections::hash_map::Iter<'_, IrLabel, usize> {
		self.labels.iter()
	}
}

pub trait CodeGenerator {
	fn build_offset_map(&self, ir_tables: &Vec<IrTable>) -> OffsetMap {
		let mut map = OffsetMap::new();
		for table in ir_tables {
			match table {
				IrTable::Table(max_size) => {
					let aligned_byte_size = max_size * 8 | 0xffff + 1;
					let num_pages = aligned_byte_size >> 16;
					println!("Reserving {} page(s) for table", num_pages);
					map.add_table(num_pages);
				},
				IrTable::Import(_) => {
					todo!("Imported tables");
				}
			}
		}
		map
	}
	fn compile_func(&mut self, code: &mut CodeEmitter, index: u32, body: Ir, signatures: &Vec<Option<IrSignature>>, offset_map: &OffsetMap);
	fn link(&mut self, code: &mut CodeEmitter);
	// fn apply_relocs(&m)
}

pub struct OffsetMap {
	top: isize,
	globals: isize,
	vm_data: isize,
	tables: Vec<isize>,
}

impl OffsetMap {
	pub fn new() -> Self {
		Self { top: -0x20000, globals: -0x20000, vm_data: -0x10000, tables: Vec::new() }
	}

	fn add_table(&mut self, num_pages: u32) {
		self.top -= num_pages as isize * 0x10000;
		self.tables.push(self.top);
	}

	pub fn get_tables_pages(&self) -> u32 {
		((-self.top - -self.globals) >> 16) as u32
	}
}
