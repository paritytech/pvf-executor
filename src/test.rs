use crate::{RawPvf, IntelX64Compiler, PvfInstance, instance::{WasmResultType, WasmParams}, PvfError};

fn wat(code: &str) -> Vec<u8> {
	wat::parse_str(code).unwrap()
}

fn test<P: WasmParams, R: WasmResultType>(code: Vec<u8>, params: P) -> R {
    let raw: RawPvf = RawPvf::from_bytes(&code);
    let ir = raw.translate().unwrap();
    let mut codegen = IntelX64Compiler::new();
    let pvf = ir.compile(&mut codegen);
    let instance = PvfInstance::instantiate(&pvf);
    unsafe { instance.call::<_, _, R>("test", params) }.unwrap()
}

#[no_mangle]
extern "C" fn add2(x: i32) -> i32 {
	// The underlying implementation of `println!` on x64 uses `movaps` aligned moves to access
	// its arguments and thus will segfault on unaligned stack. So it's called here to test the
	// proper ABI stack alignment along with other checks.
	println!("Adding 2");
	x + 2
}

fn test_with_imports<P: WasmParams, R: WasmResultType>(code: Vec<u8>, params: P) -> R {
    let mut raw = RawPvf::from_bytes(&code);
    raw.set_import_resolver(|module, name, _ty| {
    	if module == "env" {
    		match name {
    			"add2" => Ok(add2 as *const u8),
    			_ => Err(PvfError::UnresolvedImport(name.to_owned())),
    		}
    	} else {
    		Err(PvfError::UnresolvedImport(name.to_owned()))
    	}
    });
    let ir = raw.translate().unwrap();
    let mut codegen = IntelX64Compiler::new();
    let pvf = ir.compile(&mut codegen);
    let instance = PvfInstance::instantiate(&pvf);
    unsafe { instance.call::<_, _, R>("test", params) }.unwrap()
}

#[test]
fn i32_const() {
	assert_eq!(test::<_, i32>(wat(r#"(module (func (export "test") (result i32) i32.const 42))"#), ()), 42);
	assert_eq!(test::<_, i32>(wat(r#"(module (func (export "test") (result i32) i32.const -42))"#), ()), -42);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) i32.const -42))"#), ()), 4294967254);
}

#[test]
fn i64_const() {
	assert_eq!(test::<_, i64>(wat(r#"(module (func (export "test") (result i64) i64.const 42))"#), ()), 42);
	assert_eq!(test::<_, i64>(wat(r#"(module (func (export "test") (result i64) i64.const -42))"#), ()), -42);
	assert_eq!(test::<_, u64>(wat(r#"(module (func (export "test") (result i64) i64.const -42))"#), ()), 18446744073709551574);
}

#[test]
fn i32_bitwise() {
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.and (i32.const 298) (i32.const 63))))"#), ()), 42);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.or (i32.const 40) (i32.const 2))))"#), ()), 42);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.xor (i32.const 127) (i32.const 85))))"#), ()), 42);
}

#[test]
fn i32_math() {
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.add (i32.const 11) (i32.const 31))))"#), ()), 42);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.add (i32.const 86) (i32.const -44))))"#), ()), 42);
	assert_eq!(test::<_, i32>(wat(r#"(module (func (export "test") (result i32) (i32.add (i32.const -22) (i32.const -20))))"#), ()), -42);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.sub (i32.const 86) (i32.const 44))))"#), ()), 42);
	assert_eq!(test::<_, i32>(wat(r#"(module (func (export "test") (result i32) (i32.sub (i32.const -22) (i32.const 20))))"#), ()), -42);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.sub (i32.const -11) (i32.const -53))))"#), ()), 42);
}

#[test]
fn i64_math() {
	assert_eq!(test::<_, u64>(wat(r#"(module (func (export "test") (result i64) (i64.add (i64.const 4242424242424200) (i64.const 42))))"#), ()), 4242424242424242);
	assert_eq!(test::<_, u64>(wat(r#"(module (func (export "test") (result i64) (i64.add (i64.const 4242424242424284) (i64.const -42))))"#), ()), 4242424242424242);
	assert_eq!(test::<_, i64>(wat(r#"(module (func (export "test") (result i64) (i64.add (i64.const -4242424242424200) (i64.const -42))))"#), ()), -4242424242424242);
	assert_eq!(test::<_, u64>(wat(r#"(module (func (export "test") (result i64) (i64.sub (i64.const 8686868686868686) (i64.const 4444444444444444))))"#), ()), 4242424242424242);
	assert_eq!(test::<_, i64>(wat(r#"(module (func (export "test") (result i64) (i64.sub (i64.const -2222222222222222) (i64.const 2020202020202020))))"#), ()), -4242424242424242);
	assert_eq!(test::<_, u64>(wat(r#"(module (func (export "test") (result i64) (i64.sub (i64.const -1111111111111111) (i64.const -5353535353535353))))"#), ()), 4242424242424242);
}

#[test]
fn i32_cmp() {
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.eq (i32.const 42) (i32.const 42))))"#), ()), 1);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.eq (i32.const 42) (i32.const 40))))"#), ()), 0);

	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.ne (i32.const 42) (i32.const 42))))"#), ()), 0);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.ne (i32.const 42) (i32.const 40))))"#), ()), 1);

	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.lt_u (i32.const 40) (i32.const 42))))"#), ()), 1);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.lt_u (i32.const 42) (i32.const 40))))"#), ()), 0);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.lt_u (i32.const 42) (i32.const -42))))"#), ()), 1);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.lt_u (i32.const 42) (i32.const 42))))"#), ()), 0);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.lt_s (i32.const -42) (i32.const 42))))"#), ()), 1);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.lt_s (i32.const 40) (i32.const 42))))"#), ()), 1);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.lt_s (i32.const 42) (i32.const -42))))"#), ()), 0);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.lt_s (i32.const 42) (i32.const 40))))"#), ()), 0);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.lt_s (i32.const 42) (i32.const 42))))"#), ()), 0);

	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.gt_u (i32.const 40) (i32.const 42))))"#), ()), 0);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.gt_u (i32.const 42) (i32.const 40))))"#), ()), 1);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.gt_u (i32.const 42) (i32.const -42))))"#), ()), 0);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.gt_u (i32.const 42) (i32.const 42))))"#), ()), 0);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.gt_s (i32.const -42) (i32.const 42))))"#), ()), 0);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.gt_s (i32.const 40) (i32.const 42))))"#), ()), 0);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.gt_s (i32.const 42) (i32.const -42))))"#), ()), 1);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.gt_s (i32.const 42) (i32.const 40))))"#), ()), 1);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.gt_s (i32.const 42) (i32.const 42))))"#), ()), 0);

	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.le_u (i32.const 40) (i32.const 42))))"#), ()), 1);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.le_u (i32.const 42) (i32.const 40))))"#), ()), 0);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.le_u (i32.const 42) (i32.const -42))))"#), ()), 1);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.le_u (i32.const 42) (i32.const 42))))"#), ()), 1);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.le_s (i32.const -42) (i32.const 42))))"#), ()), 1);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.le_s (i32.const 40) (i32.const 42))))"#), ()), 1);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.le_s (i32.const 42) (i32.const -42))))"#), ()), 0);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.le_s (i32.const 42) (i32.const 40))))"#), ()), 0);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.le_s (i32.const 42) (i32.const 42))))"#), ()), 1);

	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.ge_u (i32.const 40) (i32.const 42))))"#), ()), 0);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.ge_u (i32.const 42) (i32.const 40))))"#), ()), 1);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.ge_u (i32.const 42) (i32.const -42))))"#), ()), 0);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.ge_u (i32.const 42) (i32.const 42))))"#), ()), 1);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.ge_s (i32.const -42) (i32.const 42))))"#), ()), 0);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.ge_s (i32.const 40) (i32.const 42))))"#), ()), 0);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.ge_s (i32.const 42) (i32.const -42))))"#), ()), 1);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.ge_s (i32.const 42) (i32.const 40))))"#), ()), 1);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.ge_s (i32.const 42) (i32.const 42))))"#), ()), 1);

	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.eqz (i32.sub (i32.const 42) (i32.const 42)))))"#), ()), 1);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.eqz (i32.sub (i32.const 40) (i32.const 42)))))"#), ()), 0);
}

#[test]
fn i64_cmp() {
	// 4242424242424242 == 0x000F_1276_5DF4_C9B2
	//       1576323506 == 0x0000_0000_5DF4_C9B2
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i64.eq (i64.const 4242424242424242) (i64.const 4242424242424242))))"#), ()), 1);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i64.eq (i64.const 4242424242424242) (i64.const 1576323506))))"#), ()), 0);

	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i64.ne (i64.const 4242424242424242) (i64.const 4242424242424242))))"#), ()), 0);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i64.ne (i64.const 4242424242424242) (i64.const 1576323506))))"#), ()), 1);

	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i64.lt_u (i64.const 4040404040404040) (i64.const 4242424242424242))))"#), ()), 1);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i64.lt_u (i64.const 4242424242424242) (i64.const 4040404040404040))))"#), ()), 0);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i64.lt_u (i64.const 4242424242424242) (i64.const -4242424242424242))))"#), ()), 1);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i64.lt_u (i64.const 4242424242424242) (i64.const 4242424242424242))))"#), ()), 0);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i64.lt_s (i64.const -4242424242424242) (i64.const 4242424242424242))))"#), ()), 1);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i64.lt_s (i64.const 4040404040404040) (i64.const 4242424242424242))))"#), ()), 1);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i64.lt_s (i64.const 4242424242424242) (i64.const -4242424242424242))))"#), ()), 0);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i64.lt_s (i64.const 4242424242424242) (i64.const 4040404040404040))))"#), ()), 0);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i64.lt_s (i64.const 4242424242424242) (i64.const 4242424242424242))))"#), ()), 0);

	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i64.gt_u (i64.const 4040404040404040) (i64.const 4242424242424242))))"#), ()), 0);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i64.gt_u (i64.const 4242424242424242) (i64.const 4040404040404040))))"#), ()), 1);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i64.gt_u (i64.const 4242424242424242) (i64.const -4242424242424242))))"#), ()), 0);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i64.gt_u (i64.const 4242424242424242) (i64.const 4242424242424242))))"#), ()), 0);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i64.gt_s (i64.const -4242424242424242) (i64.const 4242424242424242))))"#), ()), 0);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i64.gt_s (i64.const 4040404040404040) (i64.const 4242424242424242))))"#), ()), 0);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i64.gt_s (i64.const 4242424242424242) (i64.const -4242424242424242))))"#), ()), 1);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i64.gt_s (i64.const 4242424242424242) (i64.const 4040404040404040))))"#), ()), 1);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i64.gt_s (i64.const 4242424242424242) (i64.const 4242424242424242))))"#), ()), 0);

	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i64.le_u (i64.const 4040404040404040) (i64.const 4242424242424242))))"#), ()), 1);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i64.le_u (i64.const 4242424242424242) (i64.const 4040404040404040))))"#), ()), 0);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i64.le_u (i64.const 4242424242424242) (i64.const -4242424242424242))))"#), ()), 1);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i64.le_u (i64.const 4242424242424242) (i64.const 4242424242424242))))"#), ()), 1);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i64.le_s (i64.const -4242424242424242) (i64.const 4242424242424242))))"#), ()), 1);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i64.le_s (i64.const 4040404040404040) (i64.const 4242424242424242))))"#), ()), 1);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i64.le_s (i64.const 4242424242424242) (i64.const -4242424242424242))))"#), ()), 0);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i64.le_s (i64.const 4242424242424242) (i64.const 4040404040404040))))"#), ()), 0);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i64.le_s (i64.const 4242424242424242) (i64.const 4242424242424242))))"#), ()), 1);

	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i64.ge_u (i64.const 4040404040404040) (i64.const 4242424242424242))))"#), ()), 0);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i64.ge_u (i64.const 4242424242424242) (i64.const 4040404040404040))))"#), ()), 1);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i64.ge_u (i64.const 4242424242424242) (i64.const -4242424242424242))))"#), ()), 0);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i64.ge_u (i64.const 4242424242424242) (i64.const 4242424242424242))))"#), ()), 1);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i64.ge_s (i64.const -4242424242424242) (i64.const 4242424242424242))))"#), ()), 0);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i64.ge_s (i64.const 4040404040404040) (i64.const 4242424242424242))))"#), ()), 0);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i64.ge_s (i64.const 4242424242424242) (i64.const -4242424242424242))))"#), ()), 1);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i64.ge_s (i64.const 4242424242424242) (i64.const 4040404040404040))))"#), ()), 1);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i64.ge_s (i64.const 4242424242424242) (i64.const 4242424242424242))))"#), ()), 1);

	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i64.eqz (i64.sub (i64.const 4242424242424242) (i64.const 4242424242424242)))))"#), ()), 1);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i64.eqz (i64.sub (i64.const 4040404040404040) (i64.const 4242424242424242)))))"#), ()), 0);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i64.eqz (i64.sub (i64.const 4242424242424242) (i64.const 1576323506)))))"#), ()), 0);
}

#[test]
fn block() {
	assert_eq!(
		test::<_, i32>(wat(r#"
			(module
				(func (export "test") (result i32)
					(block (result i32)
						(block (result i32)
							i32.const 42
						)
					)
				)
			)"#),
			()
		),
		42
	);
	assert_eq!(
		test::<_, i32>(wat(r#"
			(module
				(func (export "test") (result i32)
					(block (result i32)
						(block (result i32)
							i32.const 40
							i32.const 41
							i32.const 42
							br 1
						)
					)
				)
			)"#),
			()
		),
		42
	);
	assert_eq!(
		test::<_, i32>(wat(r#"
			(module
				(func (export "test") (param i32) (result i32) (local i32 i32)
					i32.const 3
					local.set 2
					(loop (result i32)
						i32.const 10
						local.get 1
						i32.add
						local.set 1
						i32.const 1
						local.get 2
						i32.sub
						local.tee 2
						br_if 0
						local.get 1
					)
					local.get 0
					i32.add
				)
			)"#),
			(12,)
		),
		42
	);
}

#[test]
fn locals() {
	assert_eq!(
		test::<_, i32>(wat(r#"
			(module
				(func (export "test") (result i32) (local i32)
					i32.const 21
					local.tee 0
					local.get 0
					i32.add
				)
			)"#),
			()
		),
		42
	);
}

#[test]
fn globals() {
	assert_eq!(
		test::<_, i32>(wat(r#"
			(module
				(func (export "test") (result i32)
					i32.const 12
					global.set 1
					global.get 0
					global.get 1
					i32.add
				)
				(global i32 (i32.const 30))
				(global i32 (i32.const 0))
			)"#),
			()
		),
		42
	);
}

#[test]
fn call() {
	assert_eq!(
		test::<_, i32>(wat(r#"
			(module
				(func $i42 (result i32) i32.const 42)
				(func (export "test") (result i32) call $i42)
			)"#),
			()
		),
		42
	);
	assert_eq!(
		test::<_, i32>(wat(r#"
			(module
				(func $add2 (param i32) (result i32) (i32.add (local.get 0) (i32.const 2)))
				(func (export "test") (result i32) (i32.const 40) (call $add2))
			)"#),
			()
		),
		42
	);
	assert_eq!(
		test::<_, i32>(wat(r#"
			(module
				(func $param6 (param i32 i32 i32 i32 i32 i32) (result i32) 
					(i32.add (i32.sub (i32.add (i32.sub (i32.add (local.get 0) (local.get 1)) (local.get 2)) (local.get 3)) (local.get 4)) (local.get 5))
				)
				(func (export "test") (result i32)
					(i32.add (call $param6 (i32.const 1) (i32.const 10) (i32.const 15) (i32.const 22) (i32.const 13) (i32.const 32)) (i32.const 5))
				)
			)"#),
			()
		),
		42
	);
	assert_eq!(
		test::<_, i32>(wat(r#"
			(module
				(func $param9 (param i32 i32 i32 i32 i32 i32 i32 i32 i32) (result i32) 
					(i32.sub (i32.add (i32.sub (i32.add (i32.sub (i32.add (i32.sub (i32.add (local.get 0) (local.get 1)) (local.get 2)) (local.get 3)) (local.get 4)) (local.get 5)) (local.get 6)) (local.get 7)) (local.get 8))
				)
				(func (export "test") (result i32)
					(i32.add (call $param9 (i32.const 1) (i32.const 10) (i32.const 15) (i32.const 22) (i32.const 13) (i32.const 32) (i32.const 54) (i32.const 100) (i32.const 48)) (i32.const 7))
				)
			)"#),
			()
		),
		42
	);
	// FIXME: This requires long offset fixes
	// assert_eq!(
	// 	test::<_, i32>(wat(r#"
	// 		(module
	// 			(func $param20 (param i32 i32 i32 i32 i32 i32 i32 i32 i32 i32 i32 i32 i32 i32 i32 i32 i32 i32 i32 i32) (result i32) 
	// 				(i32.add (i32.sub (i32.add (i32.sub (i32.add (i32.sub (i32.add (i32.sub (i32.add (i32.sub (i32.add (i32.sub (i32.add (i32.sub (i32.add (i32.sub (i32.add (i32.sub (i32.add (local.get 0) (local.get 1))
	// 				(local.get 2)) (local.get 3)) (local.get 4)) (local.get 5)) (local.get 6)) (local.get 7)) (local.get 8)) (local.get 9))	(local.get 10)) (local.get 11)) (local.get 12)) (local.get 13)) (local.get 14)) (local.get 15)) (local.get 16))
	// 				(local.get 17)) (local.get 18)) (local.get 19))
	// 			)
	// 			(func (export "test") (result i32)
	// 				(i32.add (call $param20 (i32.const 1) (i32.const 10) (i32.const 15) (i32.const 22) (i32.const 13) (i32.const 32) (i32.const 54) (i32.const 100) (i32.const 48)
	// 				(i32.const 9) (i32.const 54) (i32.const 88) (i32.const 65) (i32.const 115) (i32.const 87) (i32.const 2) (i32.const 91) (i32.const 14) (i32.const 63) (i32.const 138) (i32.const 9) 
	// 				) 
	// 				(i32.const 10))
	// 			)
	// 		)"#),
	// 		()
	// 	),
	// 	42
	// );
}

#[test]
fn memory() {
	assert_eq!(
		test::<_, i32>(wat(r#"
			(module
				(func (export "test") (result i32)
					i32.const 352
					i32.const 589965607
					i32.store offset=16
					i32.const 354
					i32.load8_u offset=16
				)
				(memory 1)
				(export "memory" (memory 0))
			)"#),
			()
		),
		42
	);

}

#[test]
fn import_func() {
	assert_eq!(
		test_with_imports::<_, i32>(wat(r#"
			(module
				(import "env" "add2" (func $add2 (param i32) (result i32)))
				(func (export "test") (result i32)
					i32.const 40
					call $add2
				)
			)"#),
			()
		),
		42
	);
	// Use misaligned WASM stack to test proper alignment of machine stack
	assert_eq!(
		test_with_imports::<_, i32>(wat(r#"
			(module
				(import "env" "add2" (func $add2 (param i32) (result i32)))
				(func (export "test") (result i32)
					i32.const 38
					i32.const 40
					call $add2
				)
			)"#),
			()
		),
		42
	);
}
