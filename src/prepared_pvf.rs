use crate::{ir::IrLabel, codegen::Relocation};
use std::collections::HashMap;

pub struct PreparedPvf {
	pub(crate) code: Vec<u8>,
	pub(crate) labels: HashMap<IrLabel, usize>,
	pub(crate) relocs: Vec<(Relocation, usize)>,
	pub(crate) memory: (u32, u32),
	pub(crate) tables_pages: u32,
}

impl PreparedPvf {
	pub fn code_len(&self) -> usize {
		self.code.len()
	}

	pub fn code(&self) -> &[u8] {
		&self.code
	}

	pub fn exported_funcs(&self) -> HashMap<String, usize> {
		let mut res = HashMap::new();
		for (label, offset) in self.labels.iter() {
			if let IrLabel::ExportedFunc(_, name) = label {
				res.insert(name.clone(), *offset);
			}
		}
		res
	}
}
