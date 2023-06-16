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

pub struct PvfInstance {
	codeseg: Mmap,
	memseg: Option<MmapMut>,
	entry_points: HashMap<String, usize>
}

impl PvfInstance {
	pub fn instantiate(pvf: &PreparedPvf) -> Self {
		let mut memseg = None;
		if pvf.memory.0 > 0 {
			let mem_len = pvf.memory.1 * 0x10000;
			let memseg_mmap = MmapMut::map_anon(mem_len as usize).expect("Memory mmap did not fail to create");
			memseg = Some(memseg_mmap);
		}
		let len = (pvf.code_len() | 0xfff) + 1;
		let mut codeseg_mmap = match MmapMut::map_anon(len) {
			Ok(mmap) => mmap,
			Err(e) => panic!("Cannot create memory map: {:?}", e)
		};
		(&mut codeseg_mmap[..pvf.code_len()]).copy_from_slice(pvf.code());

		for (reloc, off) in &pvf.relocs {
			match reloc {
				Relocation::MemoryAbsolute64 if memseg.is_some() => { // FIXME: Do not generate relocations if no memory
					let memaddr = (memseg.as_ref().expect("Memory initialized")[..]).as_ptr() as usize;
					(&mut codeseg_mmap[*off..*off + 8]).copy_from_slice(&memaddr.to_le_bytes()[..]);
				},
				_ => () // FIXME
			}
		}

        println!("ICODE: {:02X?}", &codeseg_mmap[..pvf.code_len()]);

		let codeseg_mmap = match codeseg_mmap.make_exec() {
			Ok(mmap) => mmap,
			Err(e) => panic!("Cannot make mmap executable: {:?}", e)
		};
		Self { codeseg: codeseg_mmap, memseg, entry_points: pvf.exported_funcs() }
	}

	pub unsafe fn call<F, P, R>(&self, func: F, params: P) -> Result<R, PvfError>
		where F: AsRef<str> + std::fmt::Display, P: WasmParams, R: WasmResultType
	{
		if let Some(offset) = self.entry_points.get(&func.to_string()) {
			let func_ptr = ((&self.codeseg[..]).as_ptr() as usize + *offset) as *const u8;
			Ok(P::invoke::<R>(func_ptr, params))
		} else {
			Err(PvfError::ExportNotFound)
		}
	}
}
