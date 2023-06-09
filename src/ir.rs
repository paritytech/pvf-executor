use crate::{CodeGenerator, codegen::CodeEmitter, PreparedPvf};

#[derive(Debug, Copy, Clone, PartialEq)]
#[derive(Eq)]
#[derive(Hash)]
pub enum IrReg {
    Sra,
    Src,
    Srd,
}

#[derive(Debug, Clone)]
#[derive(PartialEq, Eq, Hash)]
pub enum IrOperand {
	Reg(IrReg),
	Reg8(IrReg),
	Reg16(IrReg),
	Reg32(IrReg),
	Memory8(i32, IrReg),
	Memory16(i32, IrReg),
	Memory32(i32, IrReg),
	Memory64(i32, IrReg),
	Imm32(i32),
	Imm64(i64),
    Local(u32),
    Global(u32),
}

#[derive(Debug, Clone)]
pub enum IrCp {
    Label(IrLabel),
    EnterFunction(u32),
    LeaveFunction,
    EnterBlock,
    LeaveBlock,
    InitTablePreamble(IrOperand),
    InitTableElement(IrOperand),
    InitTablePostamble,
    InitMemoryFromChunk(u32, u32, IrOperand),
    Push(IrOperand),
    Pop(IrOperand),
    Move(IrOperand, IrOperand),
    MoveIf(IrCond, IrOperand, IrOperand),
    ZeroExtend(IrOperand),
    SignExtend(IrOperand),
    Compare(IrOperand, IrOperand),
    SetIf(IrCond, IrOperand),
    Add(IrOperand, IrOperand),
    Subtract(IrOperand, IrOperand),
    Multiply(IrOperand, IrOperand),
    DivideUnsigned(IrOperand, IrOperand),
    DivideSigned(IrOperand, IrOperand),
    RemainderUnsigned(IrOperand, IrOperand),
    RemainderSigned(IrOperand, IrOperand),
    And(IrOperand, IrOperand),
    Or(IrOperand, IrOperand),
    Xor(IrOperand, IrOperand),
    ShiftLeft(IrOperand, IrOperand),
    ShiftRightUnsigned(IrOperand, IrOperand),
    ShiftRightSigned(IrOperand, IrOperand),
    RotateLeft(IrOperand, IrOperand),
    RotateRight(IrOperand, IrOperand),
    LeadingZeroes(IrOperand),
    TrailingZeroes(IrOperand),
    BitPopulationCount(IrOperand),
    Jump(IrLabel),
    JumpIf(IrCond, IrLabel),
    JumpTable(IrOperand, Vec<IrLabel>),
    Call(IrLabel),
    MemoryGrow(IrOperand),
    MemorySize(IrOperand),
    Return,
    Trap,
}

#[derive(Eq, PartialEq, Hash, Debug, Clone)]
pub enum IrLabel {
    ExportedFunc(u32, String),
    AnonymousFunc(u32),
    ImportedFunc(u32, *const u8),
    BranchTarget(u64),
    LocalLabel(u32),
    Indirect(u32, IrOperand, IrSignature),
}

#[derive(Debug, Clone)]
pub enum IrCond {
    Zero,
    NotZero,
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

    pub fn enter_function(&mut self, n_locals: u32) {
        self.0.push(IrCp::EnterFunction(n_locals));
    }

    pub fn leave_function(&mut self) {
        self.0.push(IrCp::LeaveFunction);
    }

    pub fn enter_block(&mut self) {
        self.0.push(IrCp::EnterBlock);
    }

    pub fn leave_block(&mut self) {
        self.0.push(IrCp::LeaveBlock);
    }

    pub fn init_table_preamble(&mut self, offset_reg: IrOperand) {
        self.0.push(IrCp::InitTablePreamble(offset_reg));
    }

    pub fn init_table_element(&mut self, element: IrOperand) {
        self.0.push(IrCp::InitTableElement(element));
    }

    pub fn init_table_postamble(&mut self) {
        self.0.push(IrCp::InitTablePostamble);
    }

    pub fn init_memory_from_chunk(&mut self, chunk_idx: u32, chunk_len: u32, offset_reg: IrOperand) {
        self.0.push(IrCp::InitMemoryFromChunk(chunk_idx, chunk_len, offset_reg));
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

    pub fn move_if(&mut self, cond: IrCond, dest: IrOperand, src: IrOperand) {
        self.0.push(IrCp::MoveIf(cond, dest, src));
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

    pub fn subtract(&mut self, dest: IrOperand, src: IrOperand) {
        self.0.push(IrCp::Subtract(dest, src));
    }

    pub fn multiply(&mut self, dest: IrOperand, src: IrOperand) {
        self.0.push(IrCp::Multiply(dest, src));
    }

    pub fn divide_unsigned(&mut self, dest: IrOperand, src: IrOperand) {
        self.0.push(IrCp::DivideUnsigned(dest, src));
    }

    pub fn divide_signed(&mut self, dest: IrOperand, src: IrOperand) {
        self.0.push(IrCp::DivideSigned(dest, src));
    }

    pub fn remainder_unsigned(&mut self, dest: IrOperand, src: IrOperand) {
        self.0.push(IrCp::RemainderUnsigned(dest, src));
    }

    pub fn remainder_signed(&mut self, dest: IrOperand, src: IrOperand) {
        self.0.push(IrCp::RemainderSigned(dest, src));
    }

    pub fn compare(&mut self, dest: IrOperand, src: IrOperand) {
        self.0.push(IrCp::Compare(dest, src));
    }

    pub fn set_if(&mut self, cond: IrCond, dest: IrOperand) {
        self.0.push(IrCp::SetIf(cond, dest));
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

    pub fn shift_left(&mut self, dest: IrOperand, cnt: IrOperand) {
        self.0.push(IrCp::ShiftLeft(dest, cnt));
    }

    pub fn shift_right_unsigned(&mut self, dest: IrOperand, cnt: IrOperand) {
        self.0.push(IrCp::ShiftRightUnsigned(dest, cnt));
    }

    pub fn shift_right_signed(&mut self, dest: IrOperand, cnt: IrOperand) {
        self.0.push(IrCp::ShiftRightSigned(dest, cnt));
    }

    pub fn rotate_left(&mut self, dest: IrOperand, cnt: IrOperand) {
        self.0.push(IrCp::RotateLeft(dest, cnt));
    }

    pub fn rotate_right(&mut self, dest: IrOperand, cnt: IrOperand) {
        self.0.push(IrCp::RotateRight(dest, cnt));
    }

    pub fn leading_zeroes(&mut self, src: IrOperand) {
        self.0.push(IrCp::LeadingZeroes(src));
    }

    pub fn trailing_zeroes(&mut self, src: IrOperand) {
        self.0.push(IrCp::TrailingZeroes(src));
    }

    pub fn bit_population_count(&mut self, src: IrOperand) {
        self.0.push(IrCp::BitPopulationCount(src));
    }

    pub fn jump(&mut self, target: IrLabel) {
        self.0.push(IrCp::Jump(target));
    }

    pub fn jump_if(&mut self, cond: IrCond, target: IrLabel) {
        self.0.push(IrCp::JumpIf(cond, target));
    }

    pub fn jump_table(&mut self, index: IrOperand, targets: Vec<IrLabel>) {
        self.0.push(IrCp::JumpTable(index, targets));
    }

    pub fn call(&mut self, target: IrLabel) {
        self.0.push(IrCp::Call(target));
    }

    pub fn trap(&mut self) {
        self.0.push(IrCp::Trap);
    }

    pub fn memory_grow(&mut self, pages: IrOperand) {
        self.0.push(IrCp::MemoryGrow(pages));
    }

    pub fn memory_size(&mut self, dest: IrOperand) {
        self.0.push(IrCp::MemorySize(dest));
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

#[derive(Debug, Clone)]
pub enum IrTable {
    Import(*const u8),
    Table(u32),
}

// They are called "data segments" in the Wasm spec. However, "data chunk" term is used
// throughout the code to avoid confusion with the data segment of the OS process.
#[derive(Debug, Clone)]
pub struct IrDataChunk {
    data: Vec<u8>
}

impl IrDataChunk {
    pub(crate) fn data_len(&self) -> usize {
        self.data.len()
    }
}

#[derive(Debug)]
pub struct IrPvf {
	hints: IrHints,
    funcs: Vec<Option<IrFunc>>,
    // init_index: usize,
    signatures: Vec<Option<IrSignature>>,
    memory: (u32, u32),
    tables: Vec<IrTable>,
    data_chunks: Vec<IrDataChunk>,
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
#[derive(Eq)]
#[derive(PartialEq)]
#[derive(Hash)]
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
        Self { hints: IrHints::default(), funcs: Vec::new(), signatures: Vec::new(), memory: (0, 0), tables: Vec::new(), data_chunks: Vec::new() }
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

    pub(crate) fn add_table(&mut self, max_size: u32) {
        // TODO: Imported tables are not supported yet
        // TODO: Adding tables in random order is not supported
        self.tables.push(IrTable::Table(max_size));
    }

    pub(crate) fn add_data_chunk(&mut self, data: &[u8]) {
        self.data_chunks.push(IrDataChunk { data: data.to_vec() });
    }

    pub(crate) fn set_memory(&mut self, min: u32, max: u32) {
        self.memory = (min, max);
    }

    pub(crate) fn set_hints(&mut self, hints: IrHints) {
    	self.hints = hints;
    }

    pub fn optimize(&mut self) {
        for maybe_ir in self.funcs.iter_mut() {
            if let Some(IrFunc::Function(ref mut ir)) = maybe_ir {
                if ir.0.len() == 0 {
                    continue;
                }
                let mut opt = Vec::new();
                let mut pc = 0;
                while pc < ir.0.len() - 1 {
                    match (&ir.0[pc], &ir.0[pc + 1]) {
                        (IrCp::Push(IrOperand::Reg(push_reg)), IrCp::Pop(IrOperand::Reg(pop_reg))) => {
                            if push_reg != pop_reg {
                                opt.push(IrCp::Move(IrOperand::Reg(*pop_reg), IrOperand::Reg(*push_reg)));
                            }
                            pc += 2;
                        },
                        _ => {
                            opt.push(ir.0[pc].clone());
                            pc += 1;
                        }
                    }
                }
                opt.push(ir.0[pc].clone());
                ir.0 = opt;
            }
        }
        println!("OPT IR: {:?}", self);
    }

    pub fn compile(self, codegen: &mut dyn CodeGenerator) -> PreparedPvf {
        let mut code = CodeEmitter::new();
        let offset_map = codegen.build_offset_map(&self.tables, &self.data_chunks);

        for (func_idx, maybe_ir) in self.funcs.into_iter().enumerate() {
            if let Some(IrFunc::Function(ir)) = maybe_ir {
                codegen.compile_func(&mut code, func_idx as u32, ir, &self.signatures, &offset_map);
            }
        }
        codegen.link(&mut code);

        println!("CODE: {:02X?}", code.code);

        PreparedPvf {
            code: code.code, labels: code.labels, relocs: code.relocs, memory: self.memory, tables_pages: offset_map.get_tables_pages(),
            data_chunks: self.data_chunks.into_iter().map(|s| s.data).collect(), offset_map,
         }
    }
}
