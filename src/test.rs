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
	assert_eq!(test::<_, i32>(wat(r#"(module (func (export "test") (result i32) (i32.const 42) (i32.const 41) (drop)))"#), ()), 42);
}

#[test]
fn i64_const() {
	assert_eq!(test::<_, i64>(wat(r#"(module (func (export "test") (result i64) i64.const 42))"#), ()), 42);
	assert_eq!(test::<_, i64>(wat(r#"(module (func (export "test") (result i64) i64.const -42))"#), ()), -42);
	assert_eq!(test::<_, u64>(wat(r#"(module (func (export "test") (result i64) i64.const -42))"#), ()), 18446744073709551574);
	assert_eq!(test::<_, i64>(wat(r#"(module (func (export "test") (result i64) (i64.const 42) (i64.const 41) (drop)))"#), ()), 42);
}

#[test]
fn i32_bitwise() {
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.and (i32.const 298) (i32.const 63))))"#), ()), 42);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.or (i32.const 40) (i32.const 2))))"#), ()), 42);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.xor (i32.const 127) (i32.const 85))))"#), ()), 42);

	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.shl (i32.const 42) (i32.const 2))))"#), ()), 168);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.shl (i32.const 42) (i32.const 31))))"#), ()), 0);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.shl (i32.const 42) (i32.const 34))))"#), ()), 168);

	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.shr_u (i32.const 42) (i32.const 1))))"#), ()), 21);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.shr_u (i32.const -1) (i32.const 7))))"#), ()), 33554431);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.shr_u (i32.const -1) (i32.const 31))))"#), ()), 1);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.shr_u (i32.const -1) (i32.const 39))))"#), ()), 33554431);

	assert_eq!(test::<_, i32>(wat(r#"(module (func (export "test") (result i32) (i32.shr_s (i32.const  42) (i32.const 1))))"#), ()), 21);
	assert_eq!(test::<_, i32>(wat(r#"(module (func (export "test") (result i32) (i32.shr_s (i32.const -42) (i32.const 1))))"#), ()), -21);
	assert_eq!(test::<_, i32>(wat(r#"(module (func (export "test") (result i32) (i32.shr_s (i32.const  -1) (i32.const 1))))"#), ()), -1);
	assert_eq!(test::<_, i32>(wat(r#"(module (func (export "test") (result i32) (i32.shr_s (i32.const  -1) (i32.const 31))))"#), ()), -1);

	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.rotl (i32.const 42) (i32.const 2))))"#), ()), 168);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.rotl (i32.const 42) (i32.const 31))))"#), ()), 21);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.rotl (i32.const 42) (i32.const 34))))"#), ()), 168);

	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.rotr (i32.const 42) (i32.const 1))))"#), ()), 21);
	assert_eq!(test::<_, i32>(wat(r#"(module (func (export "test") (result i32) (i32.rotr (i32.const 42) (i32.const 2))))"#), ()), -2147483638);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.rotr (i32.const 42) (i32.const 31))))"#), ()), 84);
	assert_eq!(test::<_, i32>(wat(r#"(module (func (export "test") (result i32) (i32.rotr (i32.const 42) (i32.const 34))))"#), ()), -2147483638);

	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.clz (i32.const 0x00080000))))"#), ()), 12);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.clz (i32.const 0))))"#), ()), 32);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.clz (i32.const 0x80000000))))"#), ()), 0);

	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.ctz (i32.const 0x00001000))))"#), ()), 12);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.ctz (i32.const 0))))"#), ()), 32);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.ctz (i32.const 1))))"#), ()), 0);

	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.popcnt (i32.const 0xAAAAAAAA))))"#), ()), 16);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.popcnt (i32.const -1))))"#), ()), 32);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.popcnt (i32.const 0))))"#), ()), 0);
}

#[test]
fn i64_bitwise() {
	assert_eq!(test::<_, u64>(wat(r#"(module (func (export "test") (result i64) (i64.and (i64.const 0xF2F2F2F2F2F2F2F2) (i64.const 0x4F4F4F4F4F4F4F4F))))"#), ()), 0x4242424242424242);
	assert_eq!(test::<_, u64>(wat(r#"(module (func (export "test") (result i64) (i64.or  (i64.const 0x4040404040404040) (i64.const 0x0202020202020202))))"#), ()), 0x4242424242424242);
	assert_eq!(test::<_, u64>(wat(r#"(module (func (export "test") (result i64) (i64.xor (i64.const 0xAAAAAAAAAAAAAAAA) (i64.const 0xE8E8E8E8E8E8E8E8))))"#), ()), 0x4242424242424242);

	assert_eq!(test::<_, u64>(wat(r#"(module (func (export "test") (result i64) (i64.shl   (i64.const  42) (i64.const 2))))"#), ()), 168);
	assert_eq!(test::<_, u64>(wat(r#"(module (func (export "test") (result i64) (i64.shl   (i64.const  42) (i64.const 63))))"#), ()), 0);
	assert_eq!(test::<_, u64>(wat(r#"(module (func (export "test") (result i64) (i64.shl   (i64.const  42) (i64.const 66))))"#), ()), 168);

	assert_eq!(test::<_, u64>(wat(r#"(module (func (export "test") (result i64) (i64.shr_u (i64.const  42) (i64.const 1))))"#), ()), 21);
	assert_eq!(test::<_, u64>(wat(r#"(module (func (export "test") (result i64) (i64.shr_u (i64.const  -1) (i64.const 7))))"#), ()), 0x1ffffffffffffff);
	assert_eq!(test::<_, u64>(wat(r#"(module (func (export "test") (result i64) (i64.shr_u (i64.const  -1) (i64.const 63))))"#), ()), 1);
	assert_eq!(test::<_, u64>(wat(r#"(module (func (export "test") (result i64) (i64.shr_u (i64.const  -1) (i64.const 71))))"#), ()), 0x1ffffffffffffff);

	assert_eq!(test::<_, i64>(wat(r#"(module (func (export "test") (result i64) (i64.shr_s (i64.const  42) (i64.const 1))))"#), ()), 21);
	assert_eq!(test::<_, i64>(wat(r#"(module (func (export "test") (result i64) (i64.shr_s (i64.const -42) (i64.const 1))))"#), ()), -21);
	assert_eq!(test::<_, i64>(wat(r#"(module (func (export "test") (result i64) (i64.shr_s (i64.const  -1) (i64.const 1))))"#), ()), -1);
	assert_eq!(test::<_, i64>(wat(r#"(module (func (export "test") (result i64) (i64.shr_s (i64.const  -1) (i64.const 31))))"#), ()), -1);

	assert_eq!(test::<_, u64>(wat(r#"(module (func (export "test") (result i64) (i64.rotl (i64.const 42) (i64.const 2))))"#), ()), 168);
	assert_eq!(test::<_, u64>(wat(r#"(module (func (export "test") (result i64) (i64.rotl (i64.const 42) (i64.const 63))))"#), ()), 21);
	assert_eq!(test::<_, u64>(wat(r#"(module (func (export "test") (result i64) (i64.rotl (i64.const 42) (i64.const 66))))"#), ()), 168);

	assert_eq!(test::<_, u64>(wat(r#"(module (func (export "test") (result i64) (i64.rotr (i64.const 42) (i64.const 1))))"#), ()), 21);
	assert_eq!(test::<_, u64>(wat(r#"(module (func (export "test") (result i64) (i64.rotr (i64.const 42) (i64.const 2))))"#), ()), 0x800000000000000a);
	assert_eq!(test::<_, u64>(wat(r#"(module (func (export "test") (result i64) (i64.rotr (i64.const 42) (i64.const 63))))"#), ()), 84);
	assert_eq!(test::<_, u64>(wat(r#"(module (func (export "test") (result i64) (i64.rotr (i64.const 42) (i64.const 66))))"#), ()), 0x800000000000000a);

	assert_eq!(test::<_, u64>(wat(r#"(module (func (export "test") (result i64) (i64.clz (i64.const 0x0000000000200000))))"#), ()), 42);
	assert_eq!(test::<_, u64>(wat(r#"(module (func (export "test") (result i64) (i64.clz (i64.const 0))))"#), ()), 64);
	assert_eq!(test::<_, u64>(wat(r#"(module (func (export "test") (result i64) (i64.clz (i64.const 0x8000000000000000))))"#), ()), 0);

	assert_eq!(test::<_, u64>(wat(r#"(module (func (export "test") (result i64) (i64.ctz (i64.const 0x0000040000000000))))"#), ()), 42);
	assert_eq!(test::<_, u64>(wat(r#"(module (func (export "test") (result i64) (i64.ctz (i64.const 0))))"#), ()), 64);
	assert_eq!(test::<_, u64>(wat(r#"(module (func (export "test") (result i64) (i64.ctz (i64.const 1))))"#), ()), 0);

	assert_eq!(test::<_, u64>(wat(r#"(module (func (export "test") (result i64) (i64.popcnt (i64.const 0xAAAAAAAAAAAAAAAA))))"#), ()), 32);
	assert_eq!(test::<_, u64>(wat(r#"(module (func (export "test") (result i64) (i64.popcnt (i64.const -1))))"#), ()), 64);
	assert_eq!(test::<_, u64>(wat(r#"(module (func (export "test") (result i64) (i64.popcnt (i64.const 0))))"#), ()), 0);
}

#[test]
fn i32_math() {
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.add (i32.const 11) (i32.const 31))))"#), ()), 42);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.add (i32.const 86) (i32.const -44))))"#), ()), 42);
	assert_eq!(test::<_, i32>(wat(r#"(module (func (export "test") (result i32) (i32.add (i32.const -22) (i32.const -20))))"#), ()), -42);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.sub (i32.const 86) (i32.const 44))))"#), ()), 42);
	assert_eq!(test::<_, i32>(wat(r#"(module (func (export "test") (result i32) (i32.sub (i32.const -22) (i32.const 20))))"#), ()), -42);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.sub (i32.const -11) (i32.const -53))))"#), ()), 42);
	assert_eq!(test::<_, i32>(wat(r#"(module (func (export "test") (result i32) (i32.mul (i32.const 2) (i32.const 21))))"#), ()), 42);
	assert_eq!(test::<_, i32>(wat(r#"(module (func (export "test") (result i32) (i32.mul (i32.const -2) (i32.const -21))))"#), ()), 42);
	assert_eq!(test::<_, i32>(wat(r#"(module (func (export "test") (result i32) (i32.mul (i32.const -2) (i32.const 21))))"#), ()), -42);
	assert_eq!(test::<_, i32>(wat(r#"(module (func (export "test") (result i32) (i32.mul (i32.const 0x55555555) (i32.const 7))))"#), ()), 0x55555553);
	assert_eq!(test::<_, i32>(wat(r#"(module (func (export "test") (result i32) (i32.div_u (i32.const 126) (i32.const 3))))"#), ()), 42);
	assert_eq!(test::<_, i32>(wat(r#"(module (func (export "test") (result i32) (i32.div_u (i32.const -1) (i32.const 5893))))"#), ()), 728825);
	assert_eq!(test::<_, i32>(wat(r#"(module (func (export "test") (result i32) (i32.div_u (i32.const 42) (i32.const 42))))"#), ()), 1);
	assert_eq!(test::<_, i32>(wat(r#"(module (func (export "test") (result i32) (i32.div_u (i32.const 42) (i32.const -1))))"#), ()), 0);
	assert_eq!(test::<_, i32>(wat(r#"(module (func (export "test") (result i32) (i32.div_s (i32.const 126) (i32.const 3))))"#), ()), 42);
	assert_eq!(test::<_, i32>(wat(r#"(module (func (export "test") (result i32) (i32.div_s (i32.const 126) (i32.const -3))))"#), ()), -42);
	assert_eq!(test::<_, i32>(wat(r#"(module (func (export "test") (result i32) (i32.div_s (i32.const -1) (i32.const 5893))))"#), ()), 0);
	assert_eq!(test::<_, i32>(wat(r#"(module (func (export "test") (result i32) (i32.div_s (i32.const 42) (i32.const 42))))"#), ()), 1);
	assert_eq!(test::<_, i32>(wat(r#"(module (func (export "test") (result i32) (i32.div_s (i32.const 42) (i32.const -1))))"#), ()), -42);
	assert_eq!(test::<_, i32>(wat(r#"(module (func (export "test") (result i32) (i32.rem_u (i32.const 42) (i32.const 42))))"#), ()), 0);
	assert_eq!(test::<_, i32>(wat(r#"(module (func (export "test") (result i32) (i32.rem_u (i32.const 42) (i32.const 3))))"#), ()), 0);
	assert_eq!(test::<_, i32>(wat(r#"(module (func (export "test") (result i32) (i32.rem_u (i32.const 42) (i32.const 5))))"#), ()), 2);
	assert_eq!(test::<_, i32>(wat(r#"(module (func (export "test") (result i32) (i32.rem_u (i32.const 42) (i32.const -5))))"#), ()), 42);
	assert_eq!(test::<_, i32>(wat(r#"(module (func (export "test") (result i32) (i32.rem_s (i32.const 42) (i32.const 5))))"#), ()), 2);
	assert_eq!(test::<_, i32>(wat(r#"(module (func (export "test") (result i32) (i32.rem_s (i32.const 42) (i32.const -5))))"#), ()), 2);
	assert_eq!(test::<_, i32>(wat(r#"(module (func (export "test") (result i32) (i32.rem_s (i32.const -42) (i32.const 5))))"#), ()), -2);
}

#[test]
fn i64_math() {
	assert_eq!(test::<_, u64>(wat(r#"(module (func (export "test") (result i64) (i64.add (i64.const 4242424242424200) (i64.const 42))))"#), ()), 4242424242424242);
	assert_eq!(test::<_, u64>(wat(r#"(module (func (export "test") (result i64) (i64.add (i64.const 4242424242424284) (i64.const -42))))"#), ()), 4242424242424242);
	assert_eq!(test::<_, i64>(wat(r#"(module (func (export "test") (result i64) (i64.add (i64.const -4242424242424200) (i64.const -42))))"#), ()), -4242424242424242);
	assert_eq!(test::<_, u64>(wat(r#"(module (func (export "test") (result i64) (i64.sub (i64.const 8686868686868686) (i64.const 4444444444444444))))"#), ()), 4242424242424242);
	assert_eq!(test::<_, i64>(wat(r#"(module (func (export "test") (result i64) (i64.sub (i64.const -2222222222222222) (i64.const 2020202020202020))))"#), ()), -4242424242424242);
	assert_eq!(test::<_, u64>(wat(r#"(module (func (export "test") (result i64) (i64.sub (i64.const -1111111111111111) (i64.const -5353535353535353))))"#), ()), 4242424242424242);
	assert_eq!(test::<_, i64>(wat(r#"(module (func (export "test") (result i64) (i64.mul (i64.const -2) (i64.const -21))))"#), ()), 42);
	assert_eq!(test::<_, i64>(wat(r#"(module (func (export "test") (result i64) (i64.mul (i64.const -2) (i64.const 21))))"#), ()), -42);
	assert_eq!(test::<_, i64>(wat(r#"(module (func (export "test") (result i64) (i64.mul (i64.const 0x5555555555) (i64.const 7))))"#), ()), 0x25555555553);
	assert_eq!(test::<_, i64>(wat(r#"(module (func (export "test") (result i64) (i64.mul (i64.const 0x5555555555555555) (i64.const 7))))"#), ()), 0x5555555555555553);
	assert_eq!(test::<_, i64>(wat(r#"(module (func (export "test") (result i64) (i64.div_u (i64.const 126) (i64.const 3))))"#), ()), 42);
	assert_eq!(test::<_, i64>(wat(r#"(module (func (export "test") (result i64) (i64.div_u (i64.const -1) (i64.const 283934641))))"#), ()), 64968275828);
	assert_eq!(test::<_, i64>(wat(r#"(module (func (export "test") (result i64) (i64.div_u (i64.const 42) (i64.const 42))))"#), ()), 1);
	assert_eq!(test::<_, i64>(wat(r#"(module (func (export "test") (result i64) (i64.div_u (i64.const 42) (i64.const -1))))"#), ()), 0);
	assert_eq!(test::<_, i64>(wat(r#"(module (func (export "test") (result i64) (i64.div_s (i64.const 126) (i64.const 3))))"#), ()), 42);
	assert_eq!(test::<_, i64>(wat(r#"(module (func (export "test") (result i64) (i64.div_s (i64.const 126) (i64.const -3))))"#), ()), -42);
	assert_eq!(test::<_, i64>(wat(r#"(module (func (export "test") (result i64) (i64.div_s (i64.const -1) (i64.const 283934641))))"#), ()), 0);
	assert_eq!(test::<_, i64>(wat(r#"(module (func (export "test") (result i64) (i64.div_s (i64.const 42) (i64.const 42))))"#), ()), 1);
	assert_eq!(test::<_, i64>(wat(r#"(module (func (export "test") (result i64) (i64.div_s (i64.const 42) (i64.const -1))))"#), ()), -42);
	assert_eq!(test::<_, i64>(wat(r#"(module (func (export "test") (result i64) (i64.rem_u (i64.const 42) (i64.const 42))))"#), ()), 0);
	assert_eq!(test::<_, i64>(wat(r#"(module (func (export "test") (result i64) (i64.rem_u (i64.const 42) (i64.const 3))))"#), ()), 0);
	assert_eq!(test::<_, i64>(wat(r#"(module (func (export "test") (result i64) (i64.rem_u (i64.const 42) (i64.const 5))))"#), ()), 2);
	assert_eq!(test::<_, i64>(wat(r#"(module (func (export "test") (result i64) (i64.rem_u (i64.const 42) (i64.const -5))))"#), ()), 42);
	assert_eq!(test::<_, i64>(wat(r#"(module (func (export "test") (result i64) (i64.rem_s (i64.const 42) (i64.const 5))))"#), ()), 2);
	assert_eq!(test::<_, i64>(wat(r#"(module (func (export "test") (result i64) (i64.rem_s (i64.const 42) (i64.const -5))))"#), ()), 2);
	assert_eq!(test::<_, i64>(wat(r#"(module (func (export "test") (result i64) (i64.rem_s (i64.const -42) (i64.const 5))))"#), ()), -2);
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
						(local.set 1 (i32.add (local.get 1) (i32.const 10)))
						(i32.sub (i32.const 1) (local.get 2))
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
					(local.tee 0 (i32.const 21))
					(i32.add (local.get 0))
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
					
					(global.set 1 (i32.const 12))
					(i32.add (global.get 1) (global.get 0))
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

	// 444 445 446 447 448 449 450 451
	// AA  BB  CC  DD  11  22  33  44
	// --  --  ------  --------------
	assert_eq!(
		test::<_, i32>(wat(r#"
			(module
				(func (export "test") (result i32)
					(i64.store offset=32 (i32.const 444) (i64.const 0x44332211DDCCBBAA))
					(i32.load8_u offset=32 (i32.const 444))
					(i32.load8_u offset=32 (i32.const 445))
					(i32.load16_u offset=32 (i32.const 446))
					(i32.load offset=32 (i32.const 448))
					(i32.sub (i32.add (i32.add (i32.add))) (i32.const 0x44340118))
				)
				(memory 1)
				(export "memory" (memory 0))
			)"#),
			()
		),
		42
	);

	assert_eq!(
		test::<_, i32>(wat(r#"
			(module
				(func (export "test") (result i32)
					(i64.store offset=32 (i32.const 444) (i64.const 214))
					(i32.load8_s offset=32 (i32.const 444))
					(i32.add (i32.const 84))
				)
				(memory 1)
				(export "memory" (memory 0))
			)"#),
			()
		),
		42
	);

	// TODO: Rest of store/load opcodes
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

#[test]
fn select() {
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (select (i32.const 10) (i32.const 42) (i32.const 0))))"#), ()), 42);
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (select (i32.const 42) (i32.const 10) (i32.const 1))))"#), ()), 42);
}

#[test]
fn i32_i64_conv() {
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.wrap_i64 (i64.const 0x44332211DDCCBBAA))))"#), ()), 0xDDCCBBAA);
	assert_eq!(test::<_, i64>(wat(r#"(module (func (export "test") (result i64) (i64.extend_i32_u (i32.const 42))))"#), ()), 42);
	assert_eq!(test::<_, i64>(wat(r#"(module (func (export "test") (result i64) (i64.extend_i32_u (i32.const -42))))"#), ()), 0xFFFFFFD6);
	assert_eq!(test::<_, i64>(wat(r#"(module (func (export "test") (result i64) (i64.extend_i32_s (i32.const 42))))"#), ()), 42);
	assert_eq!(test::<_, i64>(wat(r#"(module (func (export "test") (result i64) (i64.extend_i32_s (i32.const -42))))"#), ()), -42);
}

#[test]
fn call_indirect() {
	assert_eq!(
		test_with_imports::<_, i32>(wat(r#"
			(module
				(func $fn1 (result i32)
					i32.const 41
				)
				(func $fn2 (result i32)
					i32.const 42
				)
				(func $fn3 (result i32)
					i32.const 43
				)
				(func (export "test") (result i32)
					i32.const 2
					call_indirect (type 0)
				)
				(table 4 4 funcref)
				(elem (i32.const 1) $fn1 $fn2 $fn3)
			)"#),
			()
		),
		42
	);
}
