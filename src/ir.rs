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
	MemDirect(IrLabel),
	MemIndirect(IrReg, IrReg, u8, u32),
	Imm32(i32),
	Imm64(i64),
}

#[derive(Debug, Clone)]
pub enum IrCp {
    Label(IrLabel),
    Push(IrOperand),
    Pop(IrOperand),
    Mov(IrOperand, IrOperand),
    And(IrOperand, IrOperand),
    Jmp(IrLabel),
    Ret,
}

#[derive(Eq, PartialEq, Hash, Debug, Clone)]
pub enum IrLabel {
    ExportedFunc(u32, String),
    AnonymousFunc(u32),
    ImportedFunc(u32),
    BranchTarget(u64),
}

#[derive(Debug, Clone)]
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

    pub fn push(&mut self, src: IrOperand) {
        self.0.push(IrCp::Push(src));
    }

    pub fn pop(&mut self, dest: IrOperand) {
        self.0.push(IrCp::Pop(dest));
    }

    pub fn mov(&mut self, dest: IrOperand, src: IrOperand) {
        self.0.push(IrCp::Mov(dest, src));
    }

    pub fn and(&mut self, dest: IrOperand, src: IrOperand) {
        self.0.push(IrCp::And(dest, src));
    }

    pub fn jmp(&mut self, target: IrLabel) {
        self.0.push(IrCp::Jmp(target));
    }

    pub fn ret(&mut self) {
    	self.0.push(IrCp::Ret);
    }
}

#[derive(Debug)]
pub struct IrPvf {
    funcs: Vec<Option<Ir>>,
}

impl IrPvf {
    pub(crate) fn new() -> Self {
        Self { funcs: Vec::new() }
    }

    pub(crate) fn add_func(&mut self, index: u32, body: Ir) {
        if index as usize >= self.funcs.len() {
            self.funcs.resize(index as usize + 1, None);
        }
        self.funcs[index as usize] = Some(body);
    }

    pub fn compile(self, codegen: &dyn CodeGenerator) -> PreparedPvf {
        let mut code = CodeEmitter::new();

        for ir in self.funcs.into_iter().flatten() {
            codegen.compile_func(&mut code, ir);
        }

        println!("CODE: {:02X?}", code.code);

        PreparedPvf { code: code.code, labels: code.labels }
    }
}
