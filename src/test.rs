use crate::{RawPvf, IntelX64Compiler, PvfInstance, instance::{WasmResultType, WasmParams}};

fn wat(code: &str) -> Vec<u8> {
	wat::parse_str(code).unwrap()
}

fn test<P: WasmParams, R: WasmResultType>(code: Vec<u8>, params: P) -> R {
    let raw = RawPvf::from_bytes(&code);
    let ir = raw.translate().unwrap();
    let codegen = IntelX64Compiler;
    let pvf = ir.compile(&codegen);
    let instance = PvfInstance::instantiate(&pvf);
    unsafe { instance.call::<_, _, R>("test", params) }.unwrap()
}

#[test]
fn i32_const() {
	assert_eq!(test::<(), i32>(wat(r#"(module (func (export "test") (result i32) i32.const 42))"#), ()), 42);
	assert_eq!(test::<(), i32>(wat(r#"(module (func (export "test") (result i32) i32.const -42))"#), ()), -42);
	assert_eq!(test::<(), u32>(wat(r#"(module (func (export "test") (result i32) i32.const -42))"#), ()), 4294967254);
}

#[test]
fn i32_bitwise() {
	assert_eq!(test::<(), u32>(wat(r#"(module (func (export "test") (result i32) (i32.and (i32.const 298) (i32.const 63))))"#), ()), 42);
}
