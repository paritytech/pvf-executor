use std::collections::HashMap;
use memmap::{MmapMut, Mmap};
use crate::{PreparedPvf, PvfError, codegen::Relocation};

trait WasmType: Send {}
impl WasmType for i32 {}
impl WasmType for u32 {}
impl WasmType for i64 {}
impl WasmType for u64 {}

pub trait WasmResultType {}
impl<T: WasmType> WasmResultType for T {}
impl WasmResultType for () {}

pub unsafe trait WasmParams: Send {
	unsafe fn invoke<R: WasmResultType>(func: *const u8, args: Self) -> R;
}

unsafe impl<T: WasmType> WasmParams for T {
	unsafe fn invoke<R: WasmResultType>(func: *const u8, args: Self) -> R {
		<(T,) as WasmParams>::invoke::<R>(func, (args,))
	}
}

macro_rules! impl_wasm_params {
    ($($t:ident)*) => {
        unsafe impl<$($t: WasmType,)*> WasmParams for ($($t,)*) {
        	#[allow(non_snake_case)]
            unsafe fn invoke<R: WasmResultType>(func: *const u8, args: Self) -> R {
                let fnptr: unsafe extern "C" fn($($t,)*) -> R = std::mem::transmute(func);
                let ($($t,)*) = args;
               	fnptr($($t,)*)
            }
        }
    };
}

impl_wasm_params!();
impl_wasm_params!(A0);
impl_wasm_params!(A0 A1);
impl_wasm_params!(A0 A1 A2);
impl_wasm_params!(A0 A1 A2 A3);
impl_wasm_params!(A0 A1 A2 A3 A4);
impl_wasm_params!(A0 A1 A2 A3 A4 A5);
impl_wasm_params!(A0 A1 A2 A3 A4 A5 A6);
impl_wasm_params!(A0 A1 A2 A3 A4 A5 A6 A7);
impl_wasm_params!(A0 A1 A2 A3 A4 A5 A6 A7 A8);
impl_wasm_params!(A0 A1 A2 A3 A4 A5 A6 A7 A8 A9);
impl_wasm_params!(A0 A1 A2 A3 A4 A5 A6 A7 A8 A9 A10);
impl_wasm_params!(A0 A1 A2 A3 A4 A5 A6 A7 A8 A9 A10 A11);

fn offset_by(base: usize, offset: i32) -> usize {
	if offset.is_negative() {
		base - offset.abs() as usize
	} else {
		base + offset as usize
	}
}

pub struct PvfInstance {
	codeseg: Mmap,
	memseg: MmapMut,
	entry_points: HashMap<String, usize>
}

impl PvfInstance {
	pub fn instantiate(pvf: &PreparedPvf) -> Self {
		let mut memsize = 2 + pvf.tables_pages + pvf.data_segments_pages();

		if pvf.memory.0 > 0 {
			memsize += pvf.memory.1;
		}

		let mut memseg_mmap = MmapMut::map_anon(memsize as usize * 0x10000).expect("Memory mmap do not fail to create");
		let memaddr = (memseg_mmap[..]).as_ptr() as usize;
		let membase = memaddr + (2 + pvf.tables_pages as usize + pvf.data_segments_pages() as usize) * 0x10000;

		for (idx, chunk) in pvf.data_chunks.iter().enumerate() {
			let chunk_offset = offset_by(membase - memaddr, pvf.offset_map.data_chunk(idx as u32));
			(&mut memseg_mmap[chunk_offset..chunk_offset + chunk.len()]).copy_from_slice(&chunk[..]);
		}

		let len = (pvf.code_len() | 0xfff) + 1;
		let mut codeseg_mmap = match MmapMut::map_anon(len) {
			Ok(mmap) => mmap,
			Err(e) => panic!("Cannot create memory map: {:?}", e)
		};
		(&mut codeseg_mmap[..pvf.code_len()]).copy_from_slice(pvf.code());

		for (reloc, off) in &pvf.relocs {
			match reloc {
				Relocation::MemoryAbsolute64 => {
					(&mut codeseg_mmap[*off..*off + 8]).copy_from_slice(&membase.to_le_bytes()[..]);
				},
				Relocation::FunctionAbsoluteAddress => {
					let offset = usize::from_le_bytes(codeseg_mmap[*off..*off + 8][..].try_into().expect("Length is constant"));
					let addr = (&codeseg_mmap[..]).as_ptr() as usize + offset;
					(&mut codeseg_mmap[*off..*off + 8]).copy_from_slice(&addr.to_le_bytes()[..]);
				},
				Relocation::LabelAbsoluteAddress(label) => {
					let offset = *pvf.labels.get(label).expect("Unresolved label");
					let addr = (&codeseg_mmap[..]).as_ptr() as usize + offset;
					(&mut codeseg_mmap[*off..*off + 8]).copy_from_slice(&addr.to_le_bytes()[..]);
				}
			}
		}

        println!("ICODE: {:02X?}", &codeseg_mmap[..pvf.code_len()]);

		let codeseg_mmap = match codeseg_mmap.make_exec() {
			Ok(mmap) => mmap,
			Err(e) => panic!("Cannot make mmap executable: {:?}", e)
		};

		println!("CODE SEGMENT AT {:X?}, DATA SEGMENT AT {:X?}", &codeseg_mmap[..].as_ptr(), &memseg_mmap[..].as_ptr());

		let entry_points = pvf.exported_funcs();
		let init_off = entry_points.get("_pvf_init").expect("Init function found");
		println!("INIT OFFEST: {}", init_off);
		unsafe {
			// SAFERY: Init function was generated by codegen and is known to be safe
			let init_fn: extern "C" fn() = std::mem::transmute(((&codeseg_mmap[..]).as_ptr() as usize + init_off) as *const u8);
			init_fn();
		}
		println!("INIT DONE");

		Self { codeseg: codeseg_mmap, memseg: memseg_mmap, entry_points }
	}

	pub unsafe fn call<F, P, R>(&self, func: F, params: P) -> Result<R, PvfError>
		where F: AsRef<str> + std::fmt::Display, P: WasmParams, R: WasmResultType
	{
		if let Some(offset) = self.entry_points.get(&func.to_string()) {
			println!("CALL OFFSET {}", *offset);
			let func_ptr = ((&self.codeseg[..]).as_ptr() as usize + *offset) as *const u8;
			// Ok(P::invoke::<R>(func_ptr, params))
			let res = P::invoke::<R>(func_ptr, params);
			println!("CALL DONE");
			Ok(res)
		} else {
			Err(PvfError::ExportNotFound)
		}
	}
}
