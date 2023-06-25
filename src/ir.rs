use crate::{CodeGenerator, codegen::CodeEmitter, PreparedPvf};

#[derive(Debug, Clone, PartialEq)]
pub enum IrReg {
    Sra,
    Src,
    Srd,
    Bfp,
    Ffp,
    Stp,
}

#[derive(Debug, Clone)]
pub enum IrOperand {
	Reg(IrReg),
	Reg8(IrReg),
	Reg16(IrReg),
	Reg32(IrReg),
	Memory8(i32, IrReg),
	Memory16(i32, IrReg),
	Memory32(i32, IrReg),
	Memory64(i32, IrReg),
	// MemIndirect(IrReg, IrReg, u8, u32),
	Imm32(i32),
	Imm64(i64),
    Local(u32),
    Global(u32),
}

#[derive(Debug, Clone)]
pub enum IrCp {
    Label(IrLabel),
    Preamble,
    InitLocals(u32),
    Push(IrOperand),
    Pop(IrOperand),
    Move(IrOperand, IrOperand),
    ZeroExtend(IrOperand),
    SignExtend(IrOperand),
    Compare(IrCond, IrOperand, IrOperand),
    CheckIfZero(IrOperand),
    Select(IrOperand, IrOperand, IrOperand, IrOperand),
    Add(IrOperand, IrOperand),
    Sub(IrOperand, IrOperand),
    And(IrOperand, IrOperand),
    Or(IrOperand, IrOperand),
    Xor(IrOperand, IrOperand),
    Jump(IrLabel),
    JumpIf(IrCond, IrLabel),
    Call(IrLabel),
    Return,
    Trap,
    Postamble,
}

#[derive(Eq, PartialEq, Hash, Debug, Clone)]
pub enum IrLabel {
    ExportedFunc(u32, String),
    AnonymousFunc(u32),
    ImportedFunc(u32, *const u8),
    BranchTarget(u64),
    LocalLabel(u32),
}

#[derive(Debug, Clone)]
pub enum IrCond {
    Zero,
    Equal,
    NotEqual,
    LessSigned,
    LessUnsigned,
    GreaterSigned,
    GreaterUnsigned,
    LessOrEqualSigned,
    LessOrEqualUnsigned,
    GreaterOrEqualSigned,
    GreaterOrEqualUnsigned,
}

#[derive(Clone)]
pub struct Ir(Vec<IrCp>);

impl Ir {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    pub fn code(&self) -> &[IrCp] {
        &self.0
    }

    pub fn append(&mut self, other: &mut Ir) {
    	self.0.append(&mut other.0);
    }

    pub fn label(&mut self, l: IrLabel) {
        self.0.push(IrCp::Label(l));
    }

    pub fn preamble(&mut self) {
    	self.0.push(IrCp::Preamble);
    }

    pub fn init_locals(&mut self, n_locals: u32) {
        self.0.push(IrCp::InitLocals(n_locals));
    }

    pub fn push(&mut self, src: IrOperand) {
        self.0.push(IrCp::Push(src));
    }

    pub fn pop(&mut self, dest: IrOperand) {
        self.0.push(IrCp::Pop(dest));
    }

    pub fn r#move(&mut self, dest: IrOperand, src: IrOperand) {
        self.0.push(IrCp::Move(dest, src));
    }

    pub fn zero_extend(&mut self, src: IrOperand) {
        self.0.push(IrCp::ZeroExtend(src));
    }

    pub fn sign_extend(&mut self, src: IrOperand) {
        self.0.push(IrCp::SignExtend(src));
    }

    pub fn add(&mut self, dest: IrOperand, src: IrOperand) {
        self.0.push(IrCp::Add(dest, src));
    }

    pub fn sub(&mut self, dest: IrOperand, src: IrOperand) {
        self.0.push(IrCp::Sub(dest, src));
    }

    pub fn compare(&mut self, cond: IrCond, dest: IrOperand, src: IrOperand) {
        self.0.push(IrCp::Compare(cond, dest, src));
    }

    pub fn check_if_zero(&mut self, op: IrOperand) {
    	self.0.push(IrCp::CheckIfZero(op));
    }

    pub fn select(&mut self, check: IrOperand, if_zero: IrOperand, if_not_zero: IrOperand, result: IrOperand) {
    	self.0.push(IrCp::Select(check, if_zero, if_not_zero, result));
    }

    pub fn and(&mut self, dest: IrOperand, src: IrOperand) {
        self.0.push(IrCp::And(dest, src));
    }

    pub fn or(&mut self, dest: IrOperand, src: IrOperand) {
        self.0.push(IrCp::Or(dest, src));
    }

    pub fn xor(&mut self, dest: IrOperand, src: IrOperand) {
        self.0.push(IrCp::Xor(dest, src));
    }

    pub fn jump(&mut self, target: IrLabel) {
        self.0.push(IrCp::Jump(target));
    }

    pub fn jump_if(&mut self, cond: IrCond, target: IrLabel) {
        self.0.push(IrCp::JumpIf(cond, target));
    }

    pub fn call(&mut self, target: IrLabel) {
        self.0.push(IrCp::Call(target));
    }

    pub fn trap(&mut self) {
        self.0.push(IrCp::Trap);
    }

    pub fn postamble(&mut self) {
    	self.0.push(IrCp::Postamble);
    }

    pub fn r#return(&mut self) {
    	self.0.push(IrCp::Return);
    }
}

#[derive(Debug, Clone)]
enum IrFunc {
	Import(*const u8),
	Function(Ir),
}

#[derive(Debug)]
pub struct IrPvf {
	hints: IrHints,
    funcs: Vec<Option<IrFunc>>,
    // init_index: usize,
    signatures: Vec<Option<IrSignature>>,
    memory: (u32, u32),
}

impl std::fmt::Debug for Ir {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        writeln!(f)?;
        for cp in &self.0 {
            writeln!(f, "\t{:?}", cp)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct IrSignature {
    pub(crate) params: u32,
    pub(crate) results: u32,
}

#[derive(Debug, Clone, Default)]
pub struct IrHints {
	pub(crate) has_globals: bool,
	pub(crate) has_memory: bool,
	pub(crate) has_tables: bool,
}

impl IrPvf {
    pub(crate) fn new() -> Self {
        Self { hints: IrHints::default(), funcs: Vec::new(), signatures: Vec::new(), memory: (0, 0),  }
    }

    fn ensure_func_vec_size(&mut self, index: u32) {
        if index as usize >= self.funcs.len() {
            self.funcs.resize(index as usize + 1, None);
            self.signatures.resize(index as usize + 1, None);
        }
    }

    pub(crate) fn add_func(&mut self, index: u32, body: Ir, signature: IrSignature) {
    	self.ensure_func_vec_size(index);
        self.funcs[index as usize] = Some(IrFunc::Function(body));
        self.signatures[index as usize] = Some(signature);
    }

    pub(crate) fn add_func_import(&mut self, index: u32, addr: *const u8, signature: IrSignature) {
    	self.ensure_func_vec_size(index);
        self.funcs[index as usize] = Some(IrFunc::Import(addr));
        self.signatures[index as usize] = Some(signature);
    }

    pub(crate) fn set_memory(&mut self, min: u32, max: u32) {
        self.memory = (min, max);
    }

    pub(crate) fn set_hints(&mut self, hints: IrHints) {
    	self.hints = hints;
    }

    pub fn compile(self, codegen: &mut dyn CodeGenerator) -> PreparedPvf {
        let mut code = CodeEmitter::new();

        for (func_idx, maybe_ir) in self.funcs.into_iter().enumerate() {
            if let Some(IrFunc::Function(ir)) = maybe_ir {
                codegen.compile_func(&mut code, func_idx as u32, ir, &self.signatures);
            }
        }
        codegen.link(&mut code);

        println!("CODE: {:02X?}", code.code);

        PreparedPvf { code: code.code, labels: code.labels, relocs: code.relocs, memory: self.memory }
    }
}
