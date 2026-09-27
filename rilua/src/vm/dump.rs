//! Binary chunk serializer (dump Proto to PUC-Rio Lua 5.1.1 format).
//!
//! Implements the binary chunk format described in `lundump.h`. Produces
//! byte-identical output to PUC-Rio's `luaU_dump()` in `ldump.c` for the
//! same input Proto on a 64-bit little-endian Linux platform.
//!
//! The dump function accepts an optional string arena reference:
//! - `Some(arena)`: "patched" Proto from a live closure (strings are `Val::Str`)
//! - `None`: "unpatched" Proto from the compiler (strings in `string_pool`)

use super::gc::arena::Arena;
use super::proto::Proto;
use super::string::LuaString;
use super::value::Val;

// ---------------------------------------------------------------------------
// Header constants (matching lundump.h)
// ---------------------------------------------------------------------------

/// Binary chunk signature: `\x1bLua` (ESC + "Lua").
pub const LUA_SIGNATURE: &[u8] = b"\x1bLua";

/// Lua version byte: 5.1 = 0x51.
const LUAC_VERSION: u8 = 0x51;

/// Format version: 0 = official PUC-Rio format.
const LUAC_FORMAT: u8 = 0;

/// Header size in bytes.
pub const LUAC_HEADERSIZE: usize = 12;

/// Endianness flag: 1 = little-endian (our fixed target).
const ENDIANNESS: u8 = 1;

/// sizeof(int) on target platform.
const SIZEOF_INT: u8 = 4;

/// sizeof(size_t) on target platform (64-bit).
const SIZEOF_SIZE_T: u8 = 8;

/// sizeof(Instruction) (u32).
const SIZEOF_INSTRUCTION: u8 = 4;

/// sizeof(lua_Number) (f64).
const SIZEOF_LUA_NUMBER: u8 = 8;

/// Integral flag: 0 = floating point.
const INTEGRAL_FLAG: u8 = 0;

// ---------------------------------------------------------------------------
// Constant type tags (matching lundump.c)
// ---------------------------------------------------------------------------

/// Constant type: nil.
const LUA_TNIL: u8 = 0;

/// Constant type: boolean.
const LUA_TBOOLEAN: u8 = 1;

/// Constant type: number (f64).
const LUA_TNUMBER: u8 = 3;

/// Constant type: string.
const LUA_TSTRING: u8 = 4;

// ---------------------------------------------------------------------------
// DumpState
// ---------------------------------------------------------------------------

/// Internal state for the dump operation.
struct DumpState<'a> {
    /// Output buffer.
    buf: Vec<u8>,
    /// Whether to strip debug information.
    strip: bool,
    /// Optional string arena for resolving patched string constants.
    /// `Some` for live closures, `None` for freshly compiled Protos.
    string_arena: Option<&'a Arena<LuaString>>,
}

impl<'a> DumpState<'a> {
    fn new(strip: bool, string_arena: Option<&'a Arena<LuaString>>) -> Self {
        Self {
            buf: Vec::with_capacity(256),
            strip,
            string_arena,
        }
    }

    /// Writes a single byte.
    fn dump_byte(&mut self, b: u8) {
        self.buf.push(b);
    }

    /// Writes a 4-byte little-endian int.
    fn dump_int(&mut self, n: i32) {
        self.buf.extend_from_slice(&n.to_le_bytes());
    }

    /// Writes an 8-byte little-endian size_t (u64).
    fn dump_size(&mut self, n: u64) {
        self.buf.extend_from_slice(&n.to_le_bytes());
    }

    /// Writes an 8-byte little-endian f64.
    fn dump_number(&mut self, n: f64) {
        self.buf.extend_from_slice(&n.to_le_bytes());
    }

    /// Writes raw bytes.
    fn dump_block(&mut self, data: &[u8]) {
        self.buf.extend_from_slice(data);
    }

    /// Writes a string in PUC-Rio format.
    ///
    /// - Null string: size_t(0), no data.
    /// - Non-null: size_t(len+1), bytes, \0 terminator.
    fn dump_string(&mut self, s: Option<&[u8]>) {
        match s {
            None => self.dump_size(0),
            Some(data) => {
                let len = data.len() as u64;
                self.dump_size(len + 1);
                self.dump_block(data);
                self.dump_byte(0); // null terminator
            }
        }
    }

    /// Writes the 12-byte header.
    fn dump_header(&mut self) {
        self.dump_block(LUA_SIGNATURE);
        self.dump_byte(LUAC_VERSION);
        self.dump_byte(LUAC_FORMAT);
        self.dump_byte(ENDIANNESS);
        self.dump_byte(SIZEOF_INT);
        self.dump_byte(SIZEOF_SIZE_T);
        self.dump_byte(SIZEOF_INSTRUCTION);
        self.dump_byte(SIZEOF_LUA_NUMBER);
        self.dump_byte(INTEGRAL_FLAG);
    }

    /// Writes the code section (instruction array).
    fn dump_code(&mut self, proto: &Proto) {
        self.dump_int(proto.code.len() as i32);
        for &instr in &proto.code {
            self.buf.extend_from_slice(&instr.to_le_bytes());
        }
    }

    /// Writes the constant pool and nested protos.
    fn dump_constants(&mut self, proto: &Proto, parent_source: &str) {
        self.dump_int(proto.constants.len() as i32);
        for (i, val) in proto.constants.iter().enumerate() {
            match val {
                Val::Nil => {
                    // Check if this is a placeholder for a string_pool entry.
                    if let Some(bytes) = proto
                        .string_pool
                        .iter()
                        .find(|(idx, _)| *idx == i as u32)
                        .map(|(_, b)| b.as_slice())
                    {
                        self.dump_byte(LUA_TSTRING);
                        self.dump_string(Some(bytes));
                    } else {
                        self.dump_byte(LUA_TNIL);
                    }
                }
                Val::Bool(b) => {
                    self.dump_byte(LUA_TBOOLEAN);
                    self.dump_byte(u8::from(*b));
                }
                Val::Num(n) => {
                    self.dump_byte(LUA_TNUMBER);
                    self.dump_number(*n);
                }
                Val::Str(r) => {
                    self.dump_byte(LUA_TSTRING);
                    if let Some(arena) = self.string_arena {
                        if let Some(s) = arena.get(*r) {
                            self.dump_string(Some(s.data()));
                        } else {
                            // Stale ref — treat as empty string.
                            self.dump_string(Some(b""));
                        }
                    } else {
                        // No arena — look up in string_pool.
                        if let Some(bytes) = proto
                            .string_pool
                            .iter()
                            .find(|(idx, _)| *idx == i as u32)
                            .map(|(_, b)| b.as_slice())
                        {
                            self.dump_string(Some(bytes));
                        } else {
                            self.dump_string(Some(b""));
                        }
                    }
                }
                // Other Val variants cannot appear in constant pools.
                _ => self.dump_byte(LUA_TNIL),
            }
        }

        // Nested protos.
        self.dump_int(proto.protos.len() as i32);
        for child in &proto.protos {
            self.dump_function(child, parent_source);
        }
    }

    /// Writes the debug info section.
    fn dump_debug(&mut self, proto: &Proto) {
        if self.strip {
            // Stripped: all counts are 0.
            self.dump_int(0); // lineinfo
            self.dump_int(0); // localvars
            self.dump_int(0); // upvalue names
        } else {
            // Line info (parallel to code).
            self.dump_int(proto.line_info.len() as i32);
            for &line in &proto.line_info {
                self.dump_int(line as i32);
            }

            // Local variables.
            self.dump_int(proto.local_vars.len() as i32);
            for var in &proto.local_vars {
                self.dump_string(Some(var.name.as_bytes()));
                self.dump_int(var.start_pc as i32);
                self.dump_int(var.end_pc as i32);
            }

            // Upvalue names.
            self.dump_int(proto.upvalue_names.len() as i32);
            for name in &proto.upvalue_names {
                self.dump_string(Some(name.as_bytes()));
            }
        }
    }

    /// Writes a function block (recursive).
    ///
    /// `parent_source` is used for source name elision: if the child's
    /// source matches the parent, NULL is written instead (matching
    /// PUC-Rio's `DumpFunction` in `ldump.c`).
    fn dump_function(&mut self, proto: &Proto, parent_source: &str) {
        // Source name: elide if same as parent.
        if proto.source == parent_source {
            self.dump_string(None);
        } else {
            self.dump_string(Some(proto.source.as_bytes()));
        }

        self.dump_int(proto.line_defined as i32);
        self.dump_int(proto.last_line_defined as i32);
        self.dump_byte(proto.num_upvalues);
        self.dump_byte(proto.num_params);
        self.dump_byte(proto.is_vararg);
        self.dump_byte(proto.max_stack_size);

        self.dump_code(proto);
        self.dump_constants(proto, &proto.source);
        self.dump_debug(proto);
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Serializes a Proto to PUC-Rio Lua 5.1.1 binary chunk format.
///
/// # Arguments
///
/// - `proto`: The function prototype to serialize.
/// - `string_arena`: `Some(arena)` for patched Protos (live closures),
///   `None` for unpatched Protos (fresh compiler output with `string_pool`).
/// - `strip`: If `true`, omit debug information (line numbers, local
///   variable names, upvalue names).
///
/// # Returns
///
/// The complete binary chunk as a byte vector.
pub fn dump(proto: &Proto, string_arena: Option<&Arena<LuaString>>, strip: bool) -> Vec<u8> {
    let mut d = DumpState::new(strip, string_arena);
    d.dump_header();
    d.dump_function(proto, "");
    d.buf
}

/// Generates the 12-byte header for validation purposes.
#[must_use]
pub fn make_header() -> [u8; LUAC_HEADERSIZE] {
    let mut h = [0u8; LUAC_HEADERSIZE];
    h[0..4].copy_from_slice(LUA_SIGNATURE);
    h[4] = LUAC_VERSION;
    h[5] = LUAC_FORMAT;
    h[6] = ENDIANNESS;
    h[7] = SIZEOF_INT;
    h[8] = SIZEOF_SIZE_T;
    h[9] = SIZEOF_INSTRUCTION;
    h[10] = SIZEOF_LUA_NUMBER;
    h[11] = INTEGRAL_FLAG;
    h
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::approx_constant
)]
mod tests {
    use super::*;
    use crate::vm::instructions::{Instruction, OpCode};

    #[test]
    fn dump_header_format() {
        let proto = Proto::new("test");
        let bytes = dump(&proto, None, false);
        assert!(bytes.len() >= LUAC_HEADERSIZE);
        assert_eq!(&bytes[0..4], LUA_SIGNATURE);
        assert_eq!(bytes[4], LUAC_VERSION);
        assert_eq!(bytes[5], LUAC_FORMAT);
        assert_eq!(bytes[6], ENDIANNESS);
        assert_eq!(bytes[7], SIZEOF_INT);
        assert_eq!(bytes[8], SIZEOF_SIZE_T);
        assert_eq!(bytes[9], SIZEOF_INSTRUCTION);
        assert_eq!(bytes[10], SIZEOF_LUA_NUMBER);
        assert_eq!(bytes[11], INTEGRAL_FLAG);
    }

    #[test]
    fn dump_empty_function() {
        let mut proto = Proto::new("=test");
        // Add a RETURN instruction (minimal valid function).
        proto
            .code
            .push(Instruction::abc(OpCode::Return, 0, 1, 0).raw());
        proto.line_info.push(0);

        let bytes = dump(&proto, None, false);
        assert!(bytes.len() > LUAC_HEADERSIZE);
    }

    #[test]
    fn dump_with_constants() {
        let mut proto = Proto::new("=test");
        proto.constants.push(Val::Nil);
        proto.constants.push(Val::Bool(true));
        proto.constants.push(Val::Bool(false));
        proto.constants.push(Val::Num(3.14));
        // Add string via string_pool (unpatched).
        proto.constants.push(Val::Nil); // placeholder
        proto.string_pool.push((4, b"hello".to_vec()));
        proto
            .code
            .push(Instruction::abc(OpCode::Return, 0, 1, 0).raw());
        proto.line_info.push(0);

        let bytes = dump(&proto, None, false);
        // Verify it contains the string "hello" somewhere.
        assert!(bytes.windows(5).any(|w| w == b"hello"));
    }

    #[test]
    fn dump_with_nested_protos() {
        let mut inner = Proto::new("=test");
        inner
            .code
            .push(Instruction::abc(OpCode::Return, 0, 1, 0).raw());
        inner.line_info.push(0);

        let mut outer = Proto::new("=test");
        outer.protos.push(crate::vm::proto::ProtoRef::new(inner));
        outer
            .code
            .push(Instruction::abc(OpCode::Return, 0, 1, 0).raw());
        outer.line_info.push(0);

        let bytes = dump(&outer, None, false);
        // Should have 2 function blocks (outer + inner).
        assert!(bytes.len() > LUAC_HEADERSIZE + 20);
    }

    #[test]
    fn dump_stripped_no_debug() {
        let mut proto = Proto::new("=test");
        proto
            .code
            .push(Instruction::abc(OpCode::Return, 0, 1, 0).raw());
        proto.line_info.push(1);
        proto.local_vars.push(crate::vm::proto::LocalVar {
            name: "x".into(),
            start_pc: 0,
            end_pc: 1,
        });
        proto.upvalue_names.push("_ENV".into());

        let stripped = dump(&proto, None, true);
        let full = dump(&proto, None, false);
        // Stripped should be smaller (no debug info).
        assert!(stripped.len() < full.len());
    }

    #[test]
    fn make_header_matches_dump() {
        let h = make_header();
        let proto = Proto::new("test");
        let bytes = dump(&proto, None, false);
        assert_eq!(&bytes[..LUAC_HEADERSIZE], &h[..]);
    }

    #[test]
    fn dump_source_elision() {
        // Child with same source as parent should have NULL source.
        let mut inner = Proto::new("=test");
        inner
            .code
            .push(Instruction::abc(OpCode::Return, 0, 1, 0).raw());
        inner.line_info.push(0);

        let mut outer = Proto::new("=test");
        outer.protos.push(crate::vm::proto::ProtoRef::new(inner));
        outer
            .code
            .push(Instruction::abc(OpCode::Return, 0, 1, 0).raw());
        outer.line_info.push(0);

        let bytes = dump(&outer, None, false);

        // The outer source "=test" should appear once in the dump.
        // The inner source should be elided (written as size 0).
        let test_bytes = b"=test";
        let count = bytes
            .windows(test_bytes.len())
            .filter(|w| *w == test_bytes)
            .count();
        assert_eq!(count, 1, "inner source should be elided");
    }
}
