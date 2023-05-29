use std::collections::HashMap;
use memmap::{MmapMut, Mmap};
use crate::{PreparedPvf, PvfError};

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
	entry_points: HashMap<String, usize>
}

impl PvfInstance {
	pub fn instantiate(pvf: &PreparedPvf) -> Self {
		let len = (pvf.code_len() | 0xfff) + 1;
		let mut mmap = match MmapMut::map_anon(len) {
			Ok(mmap) => mmap,
			Err(e) => panic!("Cannot create memory map: {:?}", e)
		};
		(&mut mmap[..pvf.code_len()]).copy_from_slice(pvf.code());
		let mmap = match mmap.make_exec() {
			Ok(mmap) => mmap,
			Err(e) => panic!("Cannot make mmap executable: {:?}", e)
		};
		Self { codeseg: mmap, entry_points: pvf.exported_funcs() }
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
