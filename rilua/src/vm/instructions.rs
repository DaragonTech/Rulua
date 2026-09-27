//! Opcode definitions: PUC-Rio's 38 register-based opcodes.
//!
//! Instructions are encoded as `u32` values with three formats:
//! - **iABC**: `[B:9][C:9][A:8][Op:6]`
//! - **iABx**: `[Bx:18][A:8][Op:6]`
//! - **iAsBx**: `[sBx:18][A:8][Op:6]` (signed via excess-K encoding)
//!
//! The opcode occupies the 6 least-significant bits. Field A is always 8 bits.
//! Fields B and C are 9 bits each, or combined as 18-bit Bx/sBx.

use std::fmt;

// -- Bit field sizes --
const SIZE_OP: u32 = 6;
const SIZE_A: u32 = 8;
const SIZE_B: u32 = 9;
const SIZE_C: u32 = 9;
const SIZE_BX: u32 = SIZE_B + SIZE_C; // 18

// -- Bit field positions --
const POS_OP: u32 = 0;
const POS_A: u32 = POS_OP + SIZE_OP; // 6
const POS_C: u32 = POS_A + SIZE_A; // 14
const POS_B: u32 = POS_C + SIZE_C; // 23
const POS_BX: u32 = POS_C; // 14 (same as C)

// -- Bit field masks --
const MASK_OP: u32 = (1 << SIZE_OP) - 1; // 0x3F
const MASK_A: u32 = (1 << SIZE_A) - 1; // 0xFF
const MASK_B: u32 = (1 << SIZE_B) - 1; // 0x1FF
const MASK_C: u32 = (1 << SIZE_C) - 1; // 0x1FF
const MASK_BX: u32 = (1 << SIZE_BX) - 1; // 0x3FFFF

/// Maximum value for unsigned Bx field (2^18 - 1).
pub const MAXARG_BX: u32 = MASK_BX;

/// Maximum value for signed sBx field (2^17 - 1, excess-K encoding).
pub const MAXARG_SBX: i32 = (MAXARG_BX >> 1) as i32; // 131071

/// Maximum value for A field (2^8 - 1).
pub const MAXARG_A: u32 = MASK_A;

/// Maximum value for B field (2^9 - 1).
pub const MAXARG_B: u32 = MASK_B;

/// Maximum value for C field (2^9 - 1).
pub const MAXARG_C: u32 = MASK_C;

/// Maximum stack size per function.
pub const MAXSTACK: u32 = 250;

/// Bit flag marking a constant index in RK fields.
pub const BITRK: u32 = 1 << (SIZE_B - 1); // 256

/// Maximum constant index encodable in an RK field.
pub const MAXINDEXRK: u32 = BITRK - 1; // 255

/// Sentinel: invalid register.
pub const NO_REG: u32 = MAXARG_A; // 255

/// Sentinel: end of jump list.
pub const NO_JUMP: i32 = -1;

/// Number of array elements flushed per SETLIST batch.
pub const LFIELDS_PER_FLUSH: u32 = 50;

/// Maximum number of local variables per function.
pub const LUAI_MAXVARS: u32 = 200;

/// Maximum number of upvalues per function.
pub const LUAI_MAXUPVALUES: u32 = 60;

/// Tests whether an RK value refers to a constant (bit 256 set).
#[must_use]
pub const fn is_k(x: u32) -> bool {
    x & BITRK != 0
}

/// Encodes a constant pool index as an RK value (sets bit 256).
#[must_use]
pub const fn rk_as_k(idx: u32) -> u32 {
    idx | BITRK
}

/// Extracts the constant pool index from an RK value (clears bit 256).
#[must_use]
pub const fn index_k(rk: u32) -> u32 {
    rk & !BITRK
}

/// PUC-Rio's 38 opcodes in their original order.
///
/// Each variant documents its format and semantics using PUC-Rio notation:
/// - `R(x)` = register x
/// - `Kst(x)` = constant pool entry x
/// - `RK(x)` = register x or constant `Kst(x - 256)` if bit 256 set
/// - `Gbl[x]` = global table keyed by string constant x
/// - `UpValue[x]` = upvalue at index x
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum OpCode {
    /// `R(A) := R(B)` — iABC
    Move = 0,
    /// `R(A) := Kst(Bx)` — iABx
    LoadK = 1,
    /// `R(A) := (Bool)B; if (C) pc++` — iABC
    LoadBool = 2,
    /// `R(A) := ... := R(B) := nil` — iABC
    LoadNil = 3,
    /// `R(A) := UpValue[B]` — iABC
    GetUpval = 4,
    /// `R(A) := Gbl[Kst(Bx)]` — iABx
    GetGlobal = 5,
    /// `R(A) := R(B)[RK(C)]` — iABC
    GetTable = 6,
    /// `Gbl[Kst(Bx)] := R(A)` — iABx
    SetGlobal = 7,
    /// `UpValue[B] := R(A)` — iABC
    SetUpval = 8,
    /// `R(A)[RK(B)] := RK(C)` — iABC
    SetTable = 9,
    /// `R(A) := {} (size = B,C)` — iABC
    NewTable = 10,
    /// `R(A+1) := R(B); R(A) := R(B)[RK(C)]` — iABC
    OpSelf = 11,
    /// `R(A) := RK(B) + RK(C)` — iABC
    Add = 12,
    /// `R(A) := RK(B) - RK(C)` — iABC
    Sub = 13,
    /// `R(A) := RK(B) * RK(C)` — iABC
    Mul = 14,
    /// `R(A) := RK(B) / RK(C)` — iABC
    Div = 15,
    /// `R(A) := RK(B) % RK(C)` — iABC
    Mod = 16,
    /// `R(A) := RK(B) ^ RK(C)` — iABC
    Pow = 17,
    /// `R(A) := -R(B)` — iABC
    Unm = 18,
    /// `R(A) := not R(B)` — iABC
    Not = 19,
    /// `R(A) := length of R(B)` — iABC
    Len = 20,
    /// `R(A) := R(B).. ... ..R(C)` — iABC
    Concat = 21,
    /// `pc += sBx` — iAsBx
    Jmp = 22,
    /// `if ((RK(B) == RK(C)) ~= A) then pc++` — iABC
    Eq = 23,
    /// `if ((RK(B) < RK(C)) ~= A) then pc++` — iABC
    Lt = 24,
    /// `if ((RK(B) <= RK(C)) ~= A) then pc++` — iABC
    Le = 25,
    /// `if not (R(A) <=> C) then pc++` — iABC
    Test = 26,
    /// `if (R(B) <=> C) then R(A) := R(B) else pc++` — iABC
    TestSet = 27,
    /// `R(A), ..., R(A+C-2) := R(A)(R(A+1), ..., R(A+B-1))` — iABC
    Call = 28,
    /// `return R(A)(R(A+1), ..., R(A+B-1))` — iABC
    TailCall = 29,
    /// `return R(A), ..., R(A+B-2)` — iABC
    Return = 30,
    /// `R(A)+=R(A+2); if R(A) <?= R(A+1) then { pc+=sBx; R(A+3)=R(A) }` — iAsBx
    ForLoop = 31,
    /// `R(A)-=R(A+2); pc+=sBx` — iAsBx
    ForPrep = 32,
    /// `R(A+3),...,R(A+2+C) := R(A)(R(A+1), R(A+2)); if R(A+3) ~= nil then R(A+2)=R(A+3) else pc++` — iABC
    TForLoop = 33,
    /// `R(A)[(C-1)*FPF+i] := R(A+i), 1 <= i <= B` — iABC
    SetList = 34,
    /// `close all variables in stack up to (>=) R(A)` — iABC
    Close = 35,
    /// `R(A) := closure(KPROTO[Bx], R(A), ..., R(A+n))` — iABx
    Closure = 36,
    /// `R(A), R(A+1), ..., R(A+B-1) = vararg` — iABC
    VarArg = 37,
}

/// Total number of opcodes.
pub const NUM_OPCODES: u32 = 38;

/// Instruction format (matches PUC-Rio `enum OpMode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpMode {
    /// iABC format: three fields A(8), B(9), C(9).
    IABC,
    /// iABx format: two fields A(8), Bx(18).
    IABx,
    /// iAsBx format: two fields A(8), sBx(18 signed).
    IAsBx,
}

/// Operand significance (matches PUC-Rio `enum OpArgMask`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpArgMask {
    /// Argument is not used.
    N,
    /// Argument is used (arbitrary value).
    U,
    /// Argument is a register or a jump offset.
    R,
    /// Argument is a constant or register/constant.
    K,
}

impl OpCode {
    /// Converts a raw integer to an opcode, if valid.
    #[must_use]
    pub fn from_u8(n: u8) -> Option<Self> {
        // Match instead of transmute to avoid unsafe.
        match n {
            0 => Some(Self::Move),
            1 => Some(Self::LoadK),
            2 => Some(Self::LoadBool),
            3 => Some(Self::LoadNil),
            4 => Some(Self::GetUpval),
            5 => Some(Self::GetGlobal),
            6 => Some(Self::GetTable),
            7 => Some(Self::SetGlobal),
            8 => Some(Self::SetUpval),
            9 => Some(Self::SetTable),
            10 => Some(Self::NewTable),
            11 => Some(Self::OpSelf),
            12 => Some(Self::Add),
            13 => Some(Self::Sub),
            14 => Some(Self::Mul),
            15 => Some(Self::Div),
            16 => Some(Self::Mod),
            17 => Some(Self::Pow),
            18 => Some(Self::Unm),
            19 => Some(Self::Not),
            20 => Some(Self::Len),
            21 => Some(Self::Concat),
            22 => Some(Self::Jmp),
            23 => Some(Self::Eq),
            24 => Some(Self::Lt),
            25 => Some(Self::Le),
            26 => Some(Self::Test),
            27 => Some(Self::TestSet),
            28 => Some(Self::Call),
            29 => Some(Self::TailCall),
            30 => Some(Self::Return),
            31 => Some(Self::ForLoop),
            32 => Some(Self::ForPrep),
            33 => Some(Self::TForLoop),
            34 => Some(Self::SetList),
            35 => Some(Self::Close),
            36 => Some(Self::Closure),
            37 => Some(Self::VarArg),
            _ => None,
        }
    }

    /// Returns `true` if this opcode is a "test mode" instruction.
    ///
    /// Test mode opcodes use the A field as a boolean condition flag
    /// and are always followed by a JMP instruction. Used by
    /// `getjumpcontrol` to find the control instruction before a JMP.
    ///
    /// Maps to PUC-Rio's `testTMode` / `luaP_opmodes` OpArgMask flag.
    #[must_use]
    pub fn is_test_mode(self) -> bool {
        matches!(
            self,
            Self::Eq | Self::Lt | Self::Le | Self::Test | Self::TestSet
        )
    }

    /// Returns the opcode name matching PUC-Rio's `luaP_opnames`.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Move => "MOVE",
            Self::LoadK => "LOADK",
            Self::LoadBool => "LOADBOOL",
            Self::LoadNil => "LOADNIL",
            Self::GetUpval => "GETUPVAL",
            Self::GetGlobal => "GETGLOBAL",
            Self::GetTable => "GETTABLE",
            Self::SetGlobal => "SETGLOBAL",
            Self::SetUpval => "SETUPVAL",
            Self::SetTable => "SETTABLE",
            Self::NewTable => "NEWTABLE",
            Self::OpSelf => "SELF",
            Self::Add => "ADD",
            Self::Sub => "SUB",
            Self::Mul => "MUL",
            Self::Div => "DIV",
            Self::Mod => "MOD",
            Self::Pow => "POW",
            Self::Unm => "UNM",
            Self::Not => "NOT",
            Self::Len => "LEN",
            Self::Concat => "CONCAT",
            Self::Jmp => "JMP",
            Self::Eq => "EQ",
            Self::Lt => "LT",
            Self::Le => "LE",
            Self::Test => "TEST",
            Self::TestSet => "TESTSET",
            Self::Call => "CALL",
            Self::TailCall => "TAILCALL",
            Self::Return => "RETURN",
            Self::ForLoop => "FORLOOP",
            Self::ForPrep => "FORPREP",
            Self::TForLoop => "TFORLOOP",
            Self::SetList => "SETLIST",
            Self::Close => "CLOSE",
            Self::Closure => "CLOSURE",
            Self::VarArg => "VARARG",
        }
    }

    /// Returns the instruction format for this opcode.
    ///
    /// Matches PUC-Rio's `getOpMode(m)` from `luaP_opmodes`.
    #[must_use]
    pub fn mode(self) -> OpMode {
        match self {
            Self::Jmp | Self::ForLoop | Self::ForPrep => OpMode::IAsBx,
            Self::LoadK | Self::GetGlobal | Self::SetGlobal | Self::Closure => OpMode::IABx,
            _ => OpMode::IABC,
        }
    }

    /// Returns the B-operand significance for this opcode.
    ///
    /// Matches PUC-Rio's `getBMode(m)` from `luaP_opmodes`.
    #[must_use]
    pub fn b_mode(self) -> OpArgMask {
        match self {
            // OpArgK: constant or register/constant
            Self::LoadK
            | Self::GetGlobal
            | Self::SetGlobal
            | Self::SetTable
            | Self::Add
            | Self::Sub
            | Self::Mul
            | Self::Div
            | Self::Mod
            | Self::Pow
            | Self::Eq
            | Self::Lt
            | Self::Le => OpArgMask::K,
            // OpArgR: register or jump offset
            Self::Move
            | Self::LoadNil
            | Self::Unm
            | Self::Not
            | Self::Len
            | Self::Concat
            | Self::Jmp
            | Self::Test
            | Self::TestSet
            | Self::ForLoop
            | Self::ForPrep
            | Self::GetTable
            | Self::OpSelf => OpArgMask::R,
            // OpArgU: used (arbitrary)
            Self::LoadBool
            | Self::GetUpval
            | Self::SetUpval
            | Self::NewTable
            | Self::Call
            | Self::TailCall
            | Self::Return
            | Self::SetList
            | Self::Closure
            | Self::VarArg => OpArgMask::U,
            // OpArgN: not used
            Self::TForLoop | Self::Close => OpArgMask::N,
        }
    }

    /// Returns the C-operand significance for this opcode.
    ///
    /// Matches PUC-Rio's `getCMode(m)` from `luaP_opmodes`.
    #[must_use]
    pub fn c_mode(self) -> OpArgMask {
        match self {
            // OpArgK: constant or register/constant
            Self::GetTable
            | Self::OpSelf
            | Self::SetTable
            | Self::Add
            | Self::Sub
            | Self::Mul
            | Self::Div
            | Self::Mod
            | Self::Pow
            | Self::Eq
            | Self::Lt
            | Self::Le => OpArgMask::K,
            // OpArgR: register or jump offset
            Self::Concat => OpArgMask::R,
            // OpArgU: used (arbitrary)
            Self::LoadBool
            | Self::NewTable
            | Self::Call
            | Self::TailCall
            | Self::SetList
            | Self::Test
            | Self::TestSet
            | Self::TForLoop
            | Self::VarArg => OpArgMask::U,
            // OpArgN: not used
            _ => OpArgMask::N,
        }
    }

    /// Returns whether the instruction sets register A (i.e. produces a value).
    ///
    /// Matches PUC-Rio's `testAMode(m)` from `luaP_opmodes`.
    #[must_use]
    pub fn sets_register_a(self) -> bool {
        !matches!(
            self,
            Self::SetGlobal
                | Self::SetUpval
                | Self::SetTable
                | Self::Jmp
                | Self::Eq
                | Self::Lt
                | Self::Le
                | Self::Return
                | Self::SetList
                | Self::Close
                | Self::TForLoop
        )
    }
}

impl fmt::Display for OpCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// A single Lua bytecode instruction packed in a `u32`.
///
/// Provides builder methods for the three encoding formats and accessor
/// methods for extracting fields.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Instruction(u32);

impl Instruction {
    /// Creates an iABC-format instruction.
    #[must_use]
    pub const fn abc(op: OpCode, a: u32, b: u32, c: u32) -> Self {
        Self(
            ((op as u32) << POS_OP)
                | ((a & MASK_A) << POS_A)
                | ((b & MASK_B) << POS_B)
                | ((c & MASK_C) << POS_C),
        )
    }

    /// Creates an iABx-format instruction.
    #[must_use]
    pub const fn a_bx(op: OpCode, a: u32, bx: u32) -> Self {
        Self(((op as u32) << POS_OP) | ((a & MASK_A) << POS_A) | ((bx & MASK_BX) << POS_BX))
    }

    /// Creates an iAsBx-format instruction (signed Bx via excess-K).
    #[must_use]
    pub const fn a_sbx(op: OpCode, a: u32, sbx: i32) -> Self {
        // excess-K encoding: stored = sbx + MAXARG_sBx
        let encoded = (sbx + MAXARG_SBX) as u32;
        Self::a_bx(op, a, encoded)
    }

    /// Returns the raw `u32` encoding.
    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }

    /// Creates an instruction from a raw `u32`.
    #[must_use]
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    /// Extracts the opcode field (bits 0-5).
    #[must_use]
    pub fn opcode(self) -> OpCode {
        let op = (self.0 >> POS_OP) & MASK_OP;
        #[allow(clippy::cast_possible_truncation)]
        OpCode::from_u8(op as u8).unwrap_or(OpCode::Move)
    }

    /// Extracts the A field (bits 6-13).
    #[must_use]
    pub const fn a(self) -> u32 {
        (self.0 >> POS_A) & MASK_A
    }

    /// Extracts the B field (bits 23-31).
    #[must_use]
    pub const fn b(self) -> u32 {
        (self.0 >> POS_B) & MASK_B
    }

    /// Extracts the C field (bits 14-22).
    #[must_use]
    pub const fn c(self) -> u32 {
        (self.0 >> POS_C) & MASK_C
    }

    /// Extracts the Bx field (bits 14-31, unsigned).
    #[must_use]
    pub const fn bx(self) -> u32 {
        (self.0 >> POS_BX) & MASK_BX
    }

    /// Extracts the sBx field (bits 14-31, signed via excess-K).
    #[must_use]
    pub const fn sbx(self) -> i32 {
        self.bx() as i32 - MAXARG_SBX
    }

    /// Sets the A field.
    pub fn set_a(&mut self, a: u32) {
        self.0 = (self.0 & !(MASK_A << POS_A)) | ((a & MASK_A) << POS_A);
    }

    /// Sets the B field.
    pub fn set_b(&mut self, b: u32) {
        self.0 = (self.0 & !(MASK_B << POS_B)) | ((b & MASK_B) << POS_B);
    }

    /// Sets the C field.
    pub fn set_c(&mut self, c: u32) {
        self.0 = (self.0 & !(MASK_C << POS_C)) | ((c & MASK_C) << POS_C);
    }

    /// Sets the sBx field (signed via excess-K).
    pub fn set_sbx(&mut self, sbx: i32) {
        let encoded = (sbx + MAXARG_SBX) as u32;
        self.0 = (self.0 & !(MASK_BX << POS_BX)) | ((encoded & MASK_BX) << POS_BX);
    }
}

impl fmt::Debug for Instruction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Instruction({} A={} B={} C={} Bx={} sBx={})",
            self.opcode().name(),
            self.a(),
            self.b(),
            self.c(),
            self.bx(),
            self.sbx()
        )
    }
}

impl fmt::Display for Instruction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.opcode().name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- OpCode tests --

    #[test]
    fn opcode_from_u8_valid() {
        assert_eq!(OpCode::from_u8(0), Some(OpCode::Move));
        assert_eq!(OpCode::from_u8(22), Some(OpCode::Jmp));
        assert_eq!(OpCode::from_u8(37), Some(OpCode::VarArg));
    }

    #[test]
    fn opcode_from_u8_invalid() {
        assert_eq!(OpCode::from_u8(38), None);
        assert_eq!(OpCode::from_u8(255), None);
    }

    #[test]
    fn opcode_names() {
        assert_eq!(OpCode::Move.name(), "MOVE");
        assert_eq!(OpCode::LoadK.name(), "LOADK");
        assert_eq!(OpCode::Jmp.name(), "JMP");
        assert_eq!(OpCode::Return.name(), "RETURN");
        assert_eq!(OpCode::Closure.name(), "CLOSURE");
        assert_eq!(OpCode::VarArg.name(), "VARARG");
    }

    #[test]
    fn opcode_display() {
        assert_eq!(format!("{}", OpCode::Add), "ADD");
        assert_eq!(format!("{}", OpCode::SetList), "SETLIST");
    }

    #[test]
    fn opcode_values_match_puc_rio() {
        assert_eq!(OpCode::Move as u8, 0);
        assert_eq!(OpCode::LoadK as u8, 1);
        assert_eq!(OpCode::LoadBool as u8, 2);
        assert_eq!(OpCode::LoadNil as u8, 3);
        assert_eq!(OpCode::GetUpval as u8, 4);
        assert_eq!(OpCode::GetGlobal as u8, 5);
        assert_eq!(OpCode::GetTable as u8, 6);
        assert_eq!(OpCode::SetGlobal as u8, 7);
        assert_eq!(OpCode::SetUpval as u8, 8);
        assert_eq!(OpCode::SetTable as u8, 9);
        assert_eq!(OpCode::NewTable as u8, 10);
        assert_eq!(OpCode::OpSelf as u8, 11);
        assert_eq!(OpCode::Add as u8, 12);
        assert_eq!(OpCode::Sub as u8, 13);
        assert_eq!(OpCode::Mul as u8, 14);
        assert_eq!(OpCode::Div as u8, 15);
        assert_eq!(OpCode::Mod as u8, 16);
        assert_eq!(OpCode::Pow as u8, 17);
        assert_eq!(OpCode::Unm as u8, 18);
        assert_eq!(OpCode::Not as u8, 19);
        assert_eq!(OpCode::Len as u8, 20);
        assert_eq!(OpCode::Concat as u8, 21);
        assert_eq!(OpCode::Jmp as u8, 22);
        assert_eq!(OpCode::Eq as u8, 23);
        assert_eq!(OpCode::Lt as u8, 24);
        assert_eq!(OpCode::Le as u8, 25);
        assert_eq!(OpCode::Test as u8, 26);
        assert_eq!(OpCode::TestSet as u8, 27);
        assert_eq!(OpCode::Call as u8, 28);
        assert_eq!(OpCode::TailCall as u8, 29);
        assert_eq!(OpCode::Return as u8, 30);
        assert_eq!(OpCode::ForLoop as u8, 31);
        assert_eq!(OpCode::ForPrep as u8, 32);
        assert_eq!(OpCode::TForLoop as u8, 33);
        assert_eq!(OpCode::SetList as u8, 34);
        assert_eq!(OpCode::Close as u8, 35);
        assert_eq!(OpCode::Closure as u8, 36);
        assert_eq!(OpCode::VarArg as u8, 37);
    }

    // -- Instruction encoding tests --

    #[test]
    fn abc_round_trip() {
        let instr = Instruction::abc(OpCode::Add, 1, 2, 3);
        assert_eq!(instr.opcode(), OpCode::Add);
        assert_eq!(instr.a(), 1);
        assert_eq!(instr.b(), 2);
        assert_eq!(instr.c(), 3);
    }

    #[test]
    fn abc_max_values() {
        let instr = Instruction::abc(OpCode::Move, MAXARG_A, MAXARG_B, MAXARG_C);
        assert_eq!(instr.a(), MAXARG_A);
        assert_eq!(instr.b(), MAXARG_B);
        assert_eq!(instr.c(), MAXARG_C);
    }

    #[test]
    fn a_bx_round_trip() {
        let instr = Instruction::a_bx(OpCode::LoadK, 5, 1000);
        assert_eq!(instr.opcode(), OpCode::LoadK);
        assert_eq!(instr.a(), 5);
        assert_eq!(instr.bx(), 1000);
    }

    #[test]
    fn a_bx_max_value() {
        let instr = Instruction::a_bx(OpCode::Closure, 0, MAXARG_BX);
        assert_eq!(instr.bx(), MAXARG_BX);
    }

    #[test]
    fn a_sbx_positive() {
        let instr = Instruction::a_sbx(OpCode::Jmp, 0, 100);
        assert_eq!(instr.opcode(), OpCode::Jmp);
        assert_eq!(instr.a(), 0);
        assert_eq!(instr.sbx(), 100);
    }

    #[test]
    fn a_sbx_negative() {
        let instr = Instruction::a_sbx(OpCode::ForLoop, 0, -50);
        assert_eq!(instr.opcode(), OpCode::ForLoop);
        assert_eq!(instr.sbx(), -50);
    }

    #[test]
    fn a_sbx_zero() {
        let instr = Instruction::a_sbx(OpCode::Jmp, 0, 0);
        assert_eq!(instr.sbx(), 0);
    }

    #[test]
    fn a_sbx_max_positive() {
        let instr = Instruction::a_sbx(OpCode::Jmp, 0, MAXARG_SBX);
        assert_eq!(instr.sbx(), MAXARG_SBX);
    }

    #[test]
    fn a_sbx_max_negative() {
        let instr = Instruction::a_sbx(OpCode::Jmp, 0, -MAXARG_SBX);
        assert_eq!(instr.sbx(), -MAXARG_SBX);
    }

    // -- Mutator tests --

    #[test]
    fn set_a() {
        let mut instr = Instruction::abc(OpCode::Move, 0, 1, 2);
        instr.set_a(42);
        assert_eq!(instr.a(), 42);
        assert_eq!(instr.b(), 1);
        assert_eq!(instr.c(), 2);
        assert_eq!(instr.opcode(), OpCode::Move);
    }

    #[test]
    fn set_b() {
        let mut instr = Instruction::abc(OpCode::Add, 1, 0, 3);
        instr.set_b(100);
        assert_eq!(instr.b(), 100);
        assert_eq!(instr.a(), 1);
        assert_eq!(instr.c(), 3);
    }

    #[test]
    fn set_c() {
        let mut instr = Instruction::abc(OpCode::Add, 1, 2, 0);
        instr.set_c(200);
        assert_eq!(instr.c(), 200);
        assert_eq!(instr.a(), 1);
        assert_eq!(instr.b(), 2);
    }

    #[test]
    fn set_sbx() {
        let mut instr = Instruction::a_sbx(OpCode::Jmp, 0, 0);
        instr.set_sbx(42);
        assert_eq!(instr.sbx(), 42);
        instr.set_sbx(-42);
        assert_eq!(instr.sbx(), -42);
    }

    // -- RK helper tests --

    #[test]
    fn rk_encoding() {
        assert!(!is_k(0));
        assert!(!is_k(255));
        assert!(is_k(256));
        assert!(is_k(257));
    }

    #[test]
    fn rk_round_trip() {
        let idx = 42;
        let rk = rk_as_k(idx);
        assert!(is_k(rk));
        assert_eq!(index_k(rk), idx);
    }

    #[test]
    fn rk_max_index() {
        let rk = rk_as_k(MAXINDEXRK);
        assert!(is_k(rk));
        assert_eq!(index_k(rk), MAXINDEXRK);
    }

    // -- Instruction Debug/Display --

    #[test]
    fn instruction_debug() {
        let instr = Instruction::abc(OpCode::Move, 1, 2, 0);
        let debug = format!("{instr:?}");
        assert!(debug.contains("MOVE"));
        assert!(debug.contains("A=1"));
        assert!(debug.contains("B=2"));
    }

    #[test]
    fn instruction_display() {
        let instr = Instruction::abc(OpCode::Add, 0, 0, 0);
        assert_eq!(format!("{instr}"), "ADD");
    }

    // -- Constants --

    #[test]
    fn constants_match_puc_rio() {
        assert_eq!(MAXARG_BX, 262_143);
        assert_eq!(MAXARG_SBX, 131_071);
        assert_eq!(MAXARG_A, 255);
        assert_eq!(MAXARG_B, 511);
        assert_eq!(MAXARG_C, 511);
        assert_eq!(BITRK, 256);
        assert_eq!(MAXINDEXRK, 255);
        assert_eq!(NO_REG, 255);
        assert_eq!(NO_JUMP, -1);
        assert_eq!(LFIELDS_PER_FLUSH, 50);
        assert_eq!(LUAI_MAXVARS, 200);
        assert_eq!(LUAI_MAXUPVALUES, 60);
        assert_eq!(MAXSTACK, 250);
        assert_eq!(NUM_OPCODES, 38);
    }

    // -- Raw encoding --

    #[test]
    fn from_raw_round_trip() {
        let original = Instruction::abc(OpCode::GetTable, 10, 20, 30);
        let raw = original.raw();
        let decoded = Instruction::from_raw(raw);
        assert_eq!(decoded.opcode(), OpCode::GetTable);
        assert_eq!(decoded.a(), 10);
        assert_eq!(decoded.b(), 20);
        assert_eq!(decoded.c(), 30);
    }

    // -- All opcodes encode/decode --

    #[test]
    fn all_opcodes_round_trip() {
        let opcodes = [
            OpCode::Move,
            OpCode::LoadK,
            OpCode::LoadBool,
            OpCode::LoadNil,
            OpCode::GetUpval,
            OpCode::GetGlobal,
            OpCode::GetTable,
            OpCode::SetGlobal,
            OpCode::SetUpval,
            OpCode::SetTable,
            OpCode::NewTable,
            OpCode::OpSelf,
            OpCode::Add,
            OpCode::Sub,
            OpCode::Mul,
            OpCode::Div,
            OpCode::Mod,
            OpCode::Pow,
            OpCode::Unm,
            OpCode::Not,
            OpCode::Len,
            OpCode::Concat,
            OpCode::Jmp,
            OpCode::Eq,
            OpCode::Lt,
            OpCode::Le,
            OpCode::Test,
            OpCode::TestSet,
            OpCode::Call,
            OpCode::TailCall,
            OpCode::Return,
            OpCode::ForLoop,
            OpCode::ForPrep,
            OpCode::TForLoop,
            OpCode::SetList,
            OpCode::Close,
            OpCode::Closure,
            OpCode::VarArg,
        ];
        for (i, &op) in opcodes.iter().enumerate() {
            let instr = Instruction::abc(op, 1, 2, 3);
            assert_eq!(instr.opcode(), op, "opcode mismatch at index {i}");
            assert_eq!(op as u8, i as u8, "enum value mismatch for {}", op.name());
        }
    }

    // -- Opcode mode metadata --

    #[test]
    fn opcode_modes_match_puc_rio() {
        // iABx opcodes
        assert_eq!(OpCode::LoadK.mode(), OpMode::IABx);
        assert_eq!(OpCode::GetGlobal.mode(), OpMode::IABx);
        assert_eq!(OpCode::SetGlobal.mode(), OpMode::IABx);
        assert_eq!(OpCode::Closure.mode(), OpMode::IABx);

        // iAsBx opcodes
        assert_eq!(OpCode::Jmp.mode(), OpMode::IAsBx);
        assert_eq!(OpCode::ForLoop.mode(), OpMode::IAsBx);
        assert_eq!(OpCode::ForPrep.mode(), OpMode::IAsBx);

        // iABC opcodes (spot check)
        assert_eq!(OpCode::Move.mode(), OpMode::IABC);
        assert_eq!(OpCode::Add.mode(), OpMode::IABC);
        assert_eq!(OpCode::Call.mode(), OpMode::IABC);
        assert_eq!(OpCode::Return.mode(), OpMode::IABC);
        assert_eq!(OpCode::SetTable.mode(), OpMode::IABC);
    }

    #[test]
    fn opcode_b_mode_match_puc_rio() {
        assert_eq!(OpCode::Move.b_mode(), OpArgMask::R);
        assert_eq!(OpCode::LoadK.b_mode(), OpArgMask::K);
        assert_eq!(OpCode::LoadBool.b_mode(), OpArgMask::U);
        assert_eq!(OpCode::GetUpval.b_mode(), OpArgMask::U);
        assert_eq!(OpCode::Add.b_mode(), OpArgMask::K);
        assert_eq!(OpCode::Jmp.b_mode(), OpArgMask::R);
        assert_eq!(OpCode::TForLoop.b_mode(), OpArgMask::N);
        assert_eq!(OpCode::Close.b_mode(), OpArgMask::N);
        assert_eq!(OpCode::VarArg.b_mode(), OpArgMask::U);
    }

    #[test]
    fn opcode_c_mode_match_puc_rio() {
        assert_eq!(OpCode::Move.c_mode(), OpArgMask::N);
        assert_eq!(OpCode::GetTable.c_mode(), OpArgMask::K);
        assert_eq!(OpCode::SetTable.c_mode(), OpArgMask::K);
        assert_eq!(OpCode::Add.c_mode(), OpArgMask::K);
        assert_eq!(OpCode::Concat.c_mode(), OpArgMask::R);
        assert_eq!(OpCode::Call.c_mode(), OpArgMask::U);
        assert_eq!(OpCode::LoadBool.c_mode(), OpArgMask::U);
        assert_eq!(OpCode::Return.c_mode(), OpArgMask::N);
        assert_eq!(OpCode::TForLoop.c_mode(), OpArgMask::U);
    }

    #[test]
    fn opcode_sets_register_a() {
        // Instructions that set register A
        assert!(OpCode::Move.sets_register_a());
        assert!(OpCode::LoadK.sets_register_a());
        assert!(OpCode::Add.sets_register_a());
        assert!(OpCode::Call.sets_register_a());
        assert!(OpCode::Closure.sets_register_a());

        // Instructions that do NOT set register A
        assert!(!OpCode::SetGlobal.sets_register_a());
        assert!(!OpCode::SetUpval.sets_register_a());
        assert!(!OpCode::SetTable.sets_register_a());
        assert!(!OpCode::Jmp.sets_register_a());
        assert!(!OpCode::Eq.sets_register_a());
        assert!(!OpCode::Return.sets_register_a());
        assert!(!OpCode::Close.sets_register_a());
    }
}
