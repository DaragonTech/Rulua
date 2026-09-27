//! Binary chunk deserializer (load PUC-Rio Lua 5.1.1 binary format).
//!
//! Implements the loader side of the binary chunk format described in
//! `lundump.h`. Produces an unpatched Proto (strings in `string_pool`,
//! `Val::Nil` placeholders in `constants`) identical to compiler output.
//! The caller uses `patch_string_constants()` to intern strings before
//! execution.
//!
//! Error messages match PUC-Rio format:
//! `"{name}: {reason} in precompiled chunk"`.

use crate::error::{LuaError, LuaResult, SyntaxError};

use super::dump::{LUA_SIGNATURE, LUAC_HEADERSIZE, make_header};
use super::proto::{LocalVar, Proto, ProtoRef};
use super::value::Val;

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
// LoadState
// ---------------------------------------------------------------------------

/// Internal state for the undump operation.
struct LoadState<'a> {
    /// Input data.
    data: &'a [u8],
    /// Current read position.
    pos: usize,
    /// Chunk name for error messages.
    name: String,
}

impl<'a> LoadState<'a> {
    fn new(data: &'a [u8], name: &str) -> Self {
        Self {
            data,
            pos: 0,
            name: name.to_string(),
        }
    }

    /// Returns a syntax error with the standard PUC-Rio format.
    fn error(&self, reason: &str) -> LuaError {
        LuaError::Syntax(SyntaxError {
            message: format!("{}: {} in precompiled chunk", self.name, reason),
            source: self.name.clone(),
            line: 0,
            raw_message: None,
        })
    }

    /// Reads a single byte.
    fn load_byte(&mut self) -> LuaResult<u8> {
        if self.pos >= self.data.len() {
            return Err(self.error("truncated"));
        }
        let b = self.data[self.pos];
        self.pos += 1;
        Ok(b)
    }

    /// Reads `n` bytes as a slice.
    fn load_block(&mut self, n: usize) -> LuaResult<&'a [u8]> {
        if self.pos + n > self.data.len() {
            return Err(self.error("truncated"));
        }
        let slice = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(slice)
    }

    /// Reads a 4-byte little-endian int.
    fn load_int(&mut self) -> LuaResult<i32> {
        let bytes = self.load_block(4)?;
        Ok(i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    /// Reads an 8-byte little-endian size_t (u64).
    fn load_size(&mut self) -> LuaResult<u64> {
        let bytes = self.load_block(8)?;
        Ok(u64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }

    /// Reads an 8-byte little-endian f64.
    fn load_number(&mut self) -> LuaResult<f64> {
        let bytes = self.load_block(8)?;
        Ok(f64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }

    /// Reads a string in PUC-Rio format.
    ///
    /// Returns `None` for null strings (size == 0), `Some(bytes)` otherwise.
    /// The null terminator is consumed but not included in the returned data.
    fn load_string(&mut self) -> LuaResult<Option<Vec<u8>>> {
        let size = self.load_size()?;
        if size == 0 {
            return Ok(None);
        }
        // size includes the null terminator.
        let data_len = (size - 1) as usize;
        let data = self.load_block(data_len)?.to_vec();
        // Consume the null terminator.
        let _null = self.load_byte()?;
        Ok(Some(data))
    }

    /// Validates the 12-byte header.
    fn load_header(&mut self) -> LuaResult<()> {
        // Check signature.
        let sig = self.load_block(LUA_SIGNATURE.len())?;
        if sig != LUA_SIGNATURE {
            return Err(self.error("not a precompiled chunk"));
        }

        // Read remaining header bytes and compare against expected.
        let expected = make_header();
        let rest = self.load_block(LUAC_HEADERSIZE - LUA_SIGNATURE.len())?;
        if rest != &expected[LUA_SIGNATURE.len()..] {
            return Err(self.error("incompatible precompiled chunk"));
        }

        Ok(())
    }

    /// Loads the code section (instruction array).
    fn load_code(&mut self, proto: &mut Proto) -> LuaResult<()> {
        let n = self.load_int()? as usize;
        proto.code.reserve(n);
        for _ in 0..n {
            let bytes = self.load_block(4)?;
            proto
                .code
                .push(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]));
        }
        Ok(())
    }

    /// Loads the constant pool and nested protos.
    fn load_constants(&mut self, proto: &mut Proto, parent_source: &str) -> LuaResult<()> {
        let n = self.load_int()? as usize;
        proto.constants.reserve(n);
        for i in 0..n {
            let tag = self.load_byte()?;
            match tag {
                LUA_TNIL => {
                    proto.constants.push(Val::Nil);
                }
                LUA_TBOOLEAN => {
                    let b = self.load_byte()?;
                    proto.constants.push(Val::Bool(b != 0));
                }
                LUA_TNUMBER => {
                    let num = self.load_number()?;
                    proto.constants.push(Val::Num(num));
                }
                LUA_TSTRING => {
                    let bytes = self.load_string()?.unwrap_or_default();
                    // Store as unpatched: placeholder Nil + string_pool entry.
                    proto.constants.push(Val::Nil);
                    proto.string_pool.push((i as u32, bytes));
                }
                _ => {
                    return Err(self.error("bad constant type"));
                }
            }
        }

        // Nested protos.
        let np = self.load_int()? as usize;
        proto.protos.reserve(np);
        for _ in 0..np {
            let child = self.load_function(parent_source)?;
            proto.protos.push(child);
        }

        Ok(())
    }

    /// Loads the debug info section.
    fn load_debug(&mut self, proto: &mut Proto) -> LuaResult<()> {
        // Line info.
        let n = self.load_int()? as usize;
        proto.line_info.reserve(n);
        for _ in 0..n {
            proto.line_info.push(self.load_int()? as u32);
        }

        // Local variables.
        let n = self.load_int()? as usize;
        proto.local_vars.reserve(n);
        for _ in 0..n {
            let name = self.load_string()?.unwrap_or_default();
            let name = String::from_utf8_lossy(&name).into_owned();
            let start_pc = self.load_int()? as u32;
            let end_pc = self.load_int()? as u32;
            proto.local_vars.push(LocalVar {
                name,
                start_pc,
                end_pc,
            });
        }

        // Upvalue names.
        let n = self.load_int()? as usize;
        proto.upvalue_names.reserve(n);
        for _ in 0..n {
            let name = self.load_string()?.unwrap_or_default();
            proto
                .upvalue_names
                .push(String::from_utf8_lossy(&name).into_owned());
        }

        Ok(())
    }

    /// Loads a function block (recursive).
    ///
    /// `parent_source` is used for source name reconstruction: if the
    /// loaded source is NULL, the parent's source is used instead
    /// (matching PUC-Rio's `LoadFunction` in `lundump.c`).
    fn load_function(&mut self, parent_source: &str) -> LuaResult<ProtoRef> {
        // Source name.
        let source = match self.load_string()? {
            Some(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
            None => parent_source.to_string(),
        };

        let mut proto = Proto::new(&source);

        proto.line_defined = self.load_int()? as u32;
        proto.last_line_defined = self.load_int()? as u32;
        proto.num_upvalues = self.load_byte()?;
        proto.num_params = self.load_byte()?;
        proto.is_vararg = self.load_byte()?;
        proto.max_stack_size = self.load_byte()?;

        self.load_code(&mut proto)?;
        self.load_constants(&mut proto, &source)?;
        self.load_debug(&mut proto)?;

        Ok(ProtoRef::new(proto))
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Deserializes a PUC-Rio Lua 5.1.1 binary chunk into a Proto.
///
/// The returned Proto is "unpatched": string constants are stored in
/// `string_pool` with `Val::Nil` placeholders in `constants`, identical
/// to compiler output. Call `patch_string_constants()` before execution.
///
/// # Arguments
///
/// - `data`: The complete binary chunk bytes.
/// - `name`: Chunk name for error messages (e.g., `"=stdin"`, `"@file.lua"`).
///
/// # Errors
///
/// Returns `LuaError::Syntax` for invalid binary chunks, with messages
/// matching PUC-Rio format: `"{name}: {reason} in precompiled chunk"`.
pub fn undump(data: &[u8], name: &str) -> LuaResult<ProtoRef> {
    let mut s = LoadState::new(data, name);
    s.load_header()?;
    s.load_function(name)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::vm::dump::dump;
    use crate::vm::instructions::{Instruction, OpCode};

    #[test]
    fn undump_valid_header() {
        let proto = Proto::new("=test");
        let bytes = dump(&proto, None, false);
        let result = undump(&bytes, "=test");
        assert!(result.is_ok());
    }

    #[test]
    fn undump_bad_signature() {
        let data = b"not a lua chunk at all";
        let result = undump(data, "=test");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("not a precompiled chunk"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn undump_bad_version() {
        // Valid signature but wrong version byte.
        let mut data = vec![0x1b, b'L', b'u', b'a'];
        data.push(0x52); // Lua 5.2 instead of 5.1
        data.extend_from_slice(&[0; 20]); // padding
        let result = undump(&data, "=test");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("incompatible"), "unexpected error: {err}");
    }

    #[test]
    fn undump_truncated() {
        let data = b"\x1bLua"; // only signature, no version
        let result = undump(data, "=test");
        assert!(result.is_err());
    }

    #[test]
    fn round_trip_simple() {
        let mut proto = Proto::new("=test");
        proto.line_defined = 0;
        proto.last_line_defined = 0;
        proto.num_upvalues = 0;
        proto.num_params = 0;
        proto.is_vararg = 2;
        proto.max_stack_size = 2;
        proto
            .code
            .push(Instruction::abc(OpCode::Return, 0, 1, 0).raw());
        proto.line_info.push(1);

        let bytes = dump(&proto, None, false);
        let loaded = undump(&bytes, "=test").expect("undump should succeed");

        assert_eq!(loaded.source, proto.source);
        assert_eq!(loaded.line_defined, proto.line_defined);
        assert_eq!(loaded.last_line_defined, proto.last_line_defined);
        assert_eq!(loaded.num_upvalues, proto.num_upvalues);
        assert_eq!(loaded.num_params, proto.num_params);
        assert_eq!(loaded.is_vararg, proto.is_vararg);
        assert_eq!(loaded.max_stack_size, proto.max_stack_size);
        assert_eq!(loaded.code, proto.code);
        assert_eq!(loaded.line_info, proto.line_info);
    }

    #[test]
    fn round_trip_with_strings() {
        let mut proto = Proto::new("=test");
        proto.constants.push(Val::Num(42.0));
        proto.constants.push(Val::Nil); // placeholder for string
        proto.string_pool.push((1, b"hello".to_vec()));
        proto.constants.push(Val::Bool(true));
        proto
            .code
            .push(Instruction::abc(OpCode::Return, 0, 1, 0).raw());
        proto.line_info.push(0);

        let bytes = dump(&proto, None, false);
        let loaded = undump(&bytes, "=test").expect("undump should succeed");

        // Number constant should be preserved.
        assert_eq!(loaded.constants[0], Val::Num(42.0));
        // String should be in string_pool (index 1).
        assert_eq!(loaded.string_pool.len(), 1);
        assert_eq!(loaded.string_pool[0].0, 1);
        assert_eq!(loaded.string_pool[0].1, b"hello");
        // Bool should be preserved.
        assert_eq!(loaded.constants[2], Val::Bool(true));
    }

    #[test]
    fn round_trip_with_nested() {
        let mut inner = Proto::new("=test");
        inner
            .code
            .push(Instruction::abc(OpCode::Return, 0, 1, 0).raw());
        inner.line_info.push(0);
        inner.line_defined = 5;
        inner.last_line_defined = 10;

        let mut outer = Proto::new("=test");
        outer.protos.push(ProtoRef::new(inner));
        outer
            .code
            .push(Instruction::abc(OpCode::Return, 0, 1, 0).raw());
        outer.line_info.push(0);

        let bytes = dump(&outer, None, false);
        let loaded = undump(&bytes, "=test").expect("undump should succeed");

        assert_eq!(loaded.protos.len(), 1);
        assert_eq!(loaded.protos[0].line_defined, 5);
        assert_eq!(loaded.protos[0].last_line_defined, 10);
        assert_eq!(loaded.protos[0].source, "=test");
    }

    #[test]
    fn round_trip_stripped() {
        let mut proto = Proto::new("=test");
        proto
            .code
            .push(Instruction::abc(OpCode::Return, 0, 1, 0).raw());
        proto.line_info.push(1);
        proto.local_vars.push(LocalVar {
            name: "x".into(),
            start_pc: 0,
            end_pc: 1,
        });
        proto.upvalue_names.push("_ENV".into());

        let bytes = dump(&proto, None, true);
        let loaded = undump(&bytes, "=test").expect("undump should succeed");

        // Code should be preserved.
        assert_eq!(loaded.code, proto.code);
        // Debug info should be stripped.
        assert!(loaded.line_info.is_empty());
        assert!(loaded.local_vars.is_empty());
        assert!(loaded.upvalue_names.is_empty());
    }

    #[test]
    fn round_trip_debug_info() {
        let mut proto = Proto::new("=test");
        proto
            .code
            .push(Instruction::abc(OpCode::Return, 0, 1, 0).raw());
        proto.line_info.push(42);
        proto.local_vars.push(LocalVar {
            name: "myvar".into(),
            start_pc: 0,
            end_pc: 5,
        });
        proto.upvalue_names.push("upval1".into());

        let bytes = dump(&proto, None, false);
        let loaded = undump(&bytes, "=test").expect("undump should succeed");

        assert_eq!(loaded.line_info, vec![42]);
        assert_eq!(loaded.local_vars.len(), 1);
        assert_eq!(loaded.local_vars[0].name, "myvar");
        assert_eq!(loaded.local_vars[0].start_pc, 0);
        assert_eq!(loaded.local_vars[0].end_pc, 5);
        assert_eq!(loaded.upvalue_names, vec!["upval1"]);
    }
}
