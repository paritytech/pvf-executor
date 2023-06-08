use crate::{RawPvf, IntelX64Compiler, PvfInstance, instance::{WasmResultType, WasmParams}};

fn wat(code: &str) -> Vec<u8> {
	wat::parse_str(code).unwrap()
}

fn test<P: WasmParams, R: WasmResultType>(code: Vec<u8>, params: P) -> R {
    let raw = RawPvf::from_bytes(&code);
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
fn i32_bitwise() {
	assert_eq!(test::<_, u32>(wat(r#"(module (func (export "test") (result i32) (i32.and (i32.const 298) (i32.const 63))))"#), ()), 42);
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
fn local() {
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
