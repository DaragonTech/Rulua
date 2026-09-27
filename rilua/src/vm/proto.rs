//! Function prototype: bytecode container with constants and metadata.
//!
//! A `Proto` is the compiled representation of a Lua function. It contains
//! the bytecode instructions, constant pool, nested function prototypes,
//! and debug information (line numbers, local variable names).
//!
//! Protos are reference-counted rather than GC-managed because they are
//! immutable after compilation and cannot participate in cycles.
//!
//! The reference type is `Rc<Proto>` by default, or `Arc<Proto>` when
//! the `send` feature is enabled (for thread-safe embedding).

use super::value::Val;

/// Reference-counted wrapper for `Proto`.
///
/// Uses `Rc` by default (cheaper, single-threaded). With the `send`
/// feature, switches to `Arc` so that `Proto` (and thus `Lua`) can
/// be sent between threads.
#[cfg(not(feature = "send"))]
pub type ProtoRef = std::rc::Rc<Proto>;

/// Reference-counted wrapper for `Proto` (thread-safe variant).
///
/// Uses `Arc` when the `send` feature is enabled, allowing `Lua` to
/// implement `Send`.
#[cfg(feature = "send")]
pub type ProtoRef = std::sync::Arc<Proto>;

/// Vararg flag: function uses the implicit `arg` table (5.0 compat).
pub const VARARG_HASARG: u8 = 1;

/// Vararg flag: function is declared with `...`.
pub const VARARG_ISVARARG: u8 = 2;

/// Vararg flag: function needs the `arg` table.
pub const VARARG_NEEDSARG: u8 = 4;

/// A local variable's debug information.
#[derive(Debug, Clone)]
pub struct LocalVar {
    /// Variable name.
    pub name: String,
    /// First PC where the variable is active (inclusive).
    pub start_pc: u32,
    /// Last PC where the variable is active (inclusive).
    pub end_pc: u32,
}

/// Compiled function prototype.
///
/// Contains everything needed to instantiate a closure at runtime:
/// bytecode, constants, nested protos, and debug info.
#[derive(Debug, Clone)]
pub struct Proto {
    /// Bytecode instructions (each packed as `u32`).
    pub code: Vec<u32>,
    /// Constant pool (numbers, strings, nil, booleans).
    pub constants: Vec<Val>,
    /// Nested function prototypes.
    pub protos: Vec<ProtoRef>,
    /// Line number for each instruction (parallel to `code`).
    pub line_info: Vec<u32>,
    /// Local variable debug information.
    pub local_vars: Vec<LocalVar>,
    /// Upvalue names for debug output.
    pub upvalue_names: Vec<String>,
    /// Source file name.
    pub source: String,
    /// Line where the function definition starts.
    pub line_defined: u32,
    /// Line where the function definition ends.
    pub last_line_defined: u32,
    /// Number of upvalues used by this function.
    pub num_upvalues: u8,
    /// Number of fixed parameters (not including varargs).
    pub num_params: u8,
    /// Vararg flags (combination of `VARARG_*` constants).
    pub is_vararg: u8,
    /// Maximum stack size needed by this function.
    pub max_stack_size: u8,
    /// String constants awaiting GC interning. Each entry maps a constant
    /// pool index to the raw string bytes. Populated by the compiler,
    /// consumed by `patch_string_constants` before execution.
    pub string_pool: Vec<(u32, Vec<u8>)>,
}

impl Proto {
    /// Creates a new prototype for the given source.
    #[must_use]
    pub fn new(source: &str) -> Self {
        Self {
            code: Vec::new(),
            constants: Vec::new(),
            protos: Vec::new(),
            line_info: Vec::new(),
            local_vars: Vec::new(),
            upvalue_names: Vec::new(),
            source: source.to_string(),
            line_defined: 0,
            last_line_defined: 0,
            num_upvalues: 0,
            num_params: 0,
            is_vararg: 0,
            max_stack_size: 2, // minimum per PUC-Rio (register 0 + temps)
            string_pool: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proto_new_defaults() {
        let p = Proto::new("test");
        assert_eq!(p.source, "test");
        assert!(p.code.is_empty());
        assert!(p.constants.is_empty());
        assert!(p.protos.is_empty());
        assert!(p.line_info.is_empty());
        assert!(p.local_vars.is_empty());
        assert!(p.upvalue_names.is_empty());
        assert_eq!(p.line_defined, 0);
        assert_eq!(p.last_line_defined, 0);
        assert_eq!(p.num_upvalues, 0);
        assert_eq!(p.num_params, 0);
        assert_eq!(p.is_vararg, 0);
        assert_eq!(p.max_stack_size, 2);
        assert!(p.string_pool.is_empty());
    }

    #[test]
    fn vararg_flags() {
        assert_eq!(VARARG_HASARG, 1);
        assert_eq!(VARARG_ISVARARG, 2);
        assert_eq!(VARARG_NEEDSARG, 4);
        // Combined flags
        assert_eq!(VARARG_HASARG | VARARG_ISVARARG, 3);
        assert_eq!(VARARG_ISVARARG | VARARG_NEEDSARG, 6);
    }

    #[test]
    fn local_var_construction() {
        let var = LocalVar {
            name: "x".into(),
            start_pc: 0,
            end_pc: 10,
        };
        assert_eq!(var.name, "x");
        assert_eq!(var.start_pc, 0);
        assert_eq!(var.end_pc, 10);
    }

    #[test]
    fn proto_with_code() {
        use crate::vm::instructions::{Instruction, OpCode};
        let mut p = Proto::new("test");
        let instr = Instruction::abc(OpCode::Return, 0, 1, 0);
        p.code.push(instr.raw());
        p.line_info.push(1);
        assert_eq!(p.code.len(), 1);
        assert_eq!(p.line_info.len(), 1);
        let decoded = Instruction::from_raw(p.code[0]);
        assert_eq!(decoded.opcode(), OpCode::Return);
    }

    #[test]
    fn proto_with_constants() {
        let mut p = Proto::new("test");
        p.constants.push(Val::Nil);
        p.constants.push(Val::Bool(true));
        p.constants.push(Val::Num(3.0));
        assert_eq!(p.constants.len(), 3);
    }

    #[test]
    fn proto_nested() {
        let inner = ProtoRef::new(Proto::new("inner"));
        let mut outer = Proto::new("outer");
        outer.protos.push(inner);
        assert_eq!(outer.protos.len(), 1);
        assert_eq!(outer.protos[0].source, "inner");
    }

    #[test]
    fn proto_ref_sharing() {
        let p = ProtoRef::new(Proto::new("shared"));
        let p2 = ProtoRef::clone(&p);
        #[cfg(not(feature = "send"))]
        assert_eq!(std::rc::Rc::strong_count(&p), 2);
        #[cfg(feature = "send")]
        assert_eq!(std::sync::Arc::strong_count(&p), 2);
        assert_eq!(p2.source, "shared");
    }
}
