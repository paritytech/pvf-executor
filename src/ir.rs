use crate::{CodeGenerator, codegen::CodeEmitter, PreparedPvf};

#[derive(Debug, Clone)]
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
	Memory32(i32, IrReg),
	// MemIndirect(IrReg, IrReg, u8, u32),
	Imm32(i32),
	Imm64(i64),
    Local(u32),
}

#[derive(Debug, Clone)]
pub enum IrCp {
    Label(IrLabel),
    InitLocals(u32),
    Push(IrOperand),
    Pop(IrOperand),
    Mov(IrOperand, IrOperand),
    ZeroExtend(IrOperand),
    Add(IrOperand, IrOperand),
    Sub(IrOperand, IrOperand),
    And(IrOperand, IrOperand),
    Jmp(IrLabel),
    JmpIf(IrCond, IrLabel),
    Call(IrLabel),
    Ret,
}

#[derive(Eq, PartialEq, Hash, Debug, Clone)]
pub enum IrLabel {
    ExportedFunc(u32, String),
    AnonymousFunc(u32),
    ImportedFunc(u32),
    BranchTarget(u64),
    LocalLabel(u32),
}

#[derive(Debug, Clone)]
pub enum IrCond {
    Zero
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

    pub fn label(&mut self, l: IrLabel) {
        self.0.push(IrCp::Label(l));
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

    pub fn mov(&mut self, dest: IrOperand, src: IrOperand) {
        self.0.push(IrCp::Mov(dest, src));
    }

    pub fn zx(&mut self, src: IrOperand) {
        self.0.push(IrCp::ZeroExtend(src));
    }

    pub fn add(&mut self, dest: IrOperand, src: IrOperand) {
        self.0.push(IrCp::Add(dest, src));
    }

    pub fn sub(&mut self, dest: IrOperand, src: IrOperand) {
        self.0.push(IrCp::Sub(dest, src));
    }

    pub fn and(&mut self, dest: IrOperand, src: IrOperand) {
        self.0.push(IrCp::And(dest, src));
    }

    pub fn jmp(&mut self, target: IrLabel) {
        self.0.push(IrCp::Jmp(target));
    }

    pub fn jmp_if(&mut self, cond: IrCond, target: IrLabel) {
        self.0.push(IrCp::JmpIf(cond, target));
    }

    pub fn call(&mut self, target: IrLabel) {
        self.0.push(IrCp::Call(target));
    }

    pub fn ret(&mut self) {
    	self.0.push(IrCp::Ret);
    }
}

#[derive(Debug)]
pub struct IrPvf {
    funcs: Vec<Option<Ir>>,
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

impl IrPvf {
    pub(crate) fn new() -> Self {
        Self { funcs: Vec::new(), signatures: Vec::new(), memory: (0, 0) }
    }

    pub(crate) fn add_func(&mut self, index: u32, body: Ir, signature: IrSignature) {
        if index as usize >= self.funcs.len() {
            self.funcs.resize(index as usize + 1, None);
            self.signatures.resize(index as usize + 1, None);
        }
        self.funcs[index as usize] = Some(body);
        self.signatures[index as usize] = Some(signature);
    }

    pub(crate) fn set_memory(&mut self, min: u32, max: u32) {
        self.memory = (min, max);
    }

    pub fn compile(self, codegen: &mut dyn CodeGenerator) -> PreparedPvf {
        let mut code = CodeEmitter::new();

        for (func_idx, maybe_ir) in self.funcs.into_iter().enumerate() {
            if let Some(ir) = maybe_ir {
                codegen.compile_func(&mut code, func_idx as u32, ir, &self.signatures);
            }
        }
        codegen.link(&mut code);

        println!("CODE: {:02X?}", code.code);

        PreparedPvf { code: code.code, labels: code.labels, relocs: code.relocs, memory: self.memory }
    }
}
