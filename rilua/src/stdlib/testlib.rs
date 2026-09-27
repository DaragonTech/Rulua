//! Test library: internal VM introspection for the PUC-Rio test suite.
//!
//! Registers a global `T` table with functions that expose internal
//! data structures. This is the Rust equivalent of PUC-Rio's `ltests.c`.
//!
//! Functions provided:
//! - `T.querytab(t [,i])` - inspect table array/hash internals
//! - `T.hash(key [,table])` - string hash or main position
//! - `T.int2fb(n)` - float-byte encoding (luaO_int2fb / luaO_fb2int)
//! - `T.log2(n)` - integer log2 (luaO_log2)
//! - `T.listcode(f)` - disassemble function bytecode
//! - `T.setyhook(thread, mask, count)` - set yield-on-hook
//! - `T.resume(thread)` - resume coroutine (no args)
//! - `T.d2s(number)` - f64 to 8-byte native-endian string
//! - `T.s2d(string)` - 8-byte native-endian string to f64
//! - `T.testC(prog, ...)` - C API mini-interpreter
//! - `T.newuserdata(size)` - create userdata with given byte size
//! - `T.udataval(ud)` - return unique integer ID for userdata
//! - `T.pushuserdata(id)` - find/create userdata by its ID
//! - `T.ref(obj)` - store obj in registry, return integer key
//! - `T.unref(key)` - remove registry entry
//! - `T.getref(key)` - get value from registry by key
//! - `T.upvalue(f, n [, val])` - get/set upvalue n of closure f
//! - `T.checkmemory()` - no-op (GC consistency check stub)
//! - `T.gsub(s, p, r)` - simple string substitution
//! - `T.doonnewstack(code)` - run code in a new coroutine
//! - `T.newstate()` - create independent Lua state
//! - `T.closestate(L)` - close a state created by newstate
//! - `T.doremote(L, code)` - execute code string in remote state
//! - `T.loadlib(L)` - load standard libraries into remote state
//! - `T.totalmem([limit])` - get/set memory limit

use crate::compiler::codegen::int2fb;
use crate::error::{LuaError, LuaResult, RuntimeError};
use crate::vm::closure::{Closure, RustClosure};
use crate::vm::execute::fb2int;
use crate::vm::gc::arena::GcRef;
use crate::vm::instructions::{Instruction, OpMode};
use crate::vm::state::{LuaState, MASK_COUNT, MASK_LINE};
use crate::vm::table::Table;
use crate::vm::value::{Userdata, Val};

use super::coroutine;
use super::{arg_error, type_error};

// =========================================================================
// Helper: create a runtime error without proto/pc context
// =========================================================================

fn rt_error(msg: &str) -> LuaError {
    LuaError::Runtime(RuntimeError {
        message: msg.into(),
        level: 0,
        traceback: vec![],
    })
}

// =========================================================================
// T.testC mini-interpreter
// =========================================================================

/// Implements `T.testC(prog, ...)` — a string-based mini-interpreter that
/// simulates C API operations on the Lua stack.
///
/// Parses semicolon- and comma-delimited commands from the first argument,
/// executing them sequentially. Remaining arguments are accessible via
/// testC stack indices starting at 1.
///
/// Special first-argument handling: if the first argument is a userdata
/// (a remote state from `T.newstate`), operates on that state instead.
///
/// Matches PUC-Rio's `runC` from `ltests.c`.
pub fn t_testc(state: &mut LuaState) -> LuaResult<u32> {
    if state.base >= state.top {
        return Err(rt_error("testC: missing program string"));
    }

    // Check if first arg is a remote state (userdata).
    let first = state.stack_get(state.base);
    if let Val::Userdata(ud_ref) = first {
        // Remote state mode: T.testC(L, prog)
        return testc_remote(state, ud_ref);
    }

    // Normal mode: first arg is the program string.
    let prog = get_string(state, state.base)?;

    // Get the calling closure's environment for E pseudo-index.
    let caller_func_idx = state.call_stack[state.ci].func;
    let caller_closure_ref = match state.stack_get(caller_func_idx) {
        Val::Function(r) => Some(r),
        _ => None,
    };

    // The program string is at base+0. Extra args start at base+1.
    // In PUC-Rio, luaL_checkstring pops/reads the string, and testC
    // indices start at 1 for the first extra arg. We keep base at
    // base+0 (the program string), so testC index 1 = base+1.
    // But for stack operations, we need to track an "effective base"
    // that skips the program string.
    let tc_base = state.base; // testC index 0 = program string
    // testC index 1 = tc_base + 1 = first extra arg

    run_testc_program(state, &prog, tc_base, caller_closure_ref)
}

/// Check if a SpecialIndex points to a valid (in-range) stack position.
/// G, E, R, U pseudo-indices are always valid.
fn is_valid_index(state: &LuaState, tc_base: usize, si: &SpecialIndex) -> bool {
    match si {
        SpecialIndex::Stack(i) => {
            if let Some(pos) = resolve_index(state, tc_base, *i) {
                pos < state.top
            } else {
                false
            }
        }
        _ => true, // G/E/R/U are always valid
    }
}

/// Resolves a testC stack index to an absolute stack position.
///
/// Matches PUC-Rio's C API indexing: positive indices are 1-based from
/// `L->base` (index 1 = the program string, index 2 = first extra arg).
/// Negative indices count back from `state.top`.
fn resolve_index(state: &LuaState, tc_base: usize, idx: i32) -> Option<usize> {
    match idx.cmp(&0) {
        std::cmp::Ordering::Greater => {
            // PUC-Rio: positive index i maps to L->base + (i - 1).
            // tc_base == state.base, so pos = tc_base + idx - 1.
            let pos = tc_base + (idx as usize - 1);
            Some(pos)
        }
        std::cmp::Ordering::Less => {
            let pos = state.top as i64 + i64::from(idx);
            if pos >= 0 { Some(pos as usize) } else { None }
        }
        std::cmp::Ordering::Equal => None,
    }
}

enum SpecialIndex {
    Stack(i32),
    Global,
    Env,
    Registry,
    Upvalue(usize),
}

/// Get the value at a special index.
fn get_special(
    state: &LuaState,
    tc_base: usize,
    idx: &SpecialIndex,
    caller_closure_ref: Option<GcRef<Closure>>,
) -> Val {
    match idx {
        SpecialIndex::Stack(i) => {
            if let Some(pos) = resolve_index(state, tc_base, *i) {
                if pos < state.top {
                    state.stack_get(pos)
                } else {
                    Val::Nil
                }
            } else {
                Val::Nil
            }
        }
        SpecialIndex::Global => Val::Table(state.global),
        SpecialIndex::Env => {
            if let Some(cr) = caller_closure_ref {
                if let Some(cl) = state.gc.closures.get(cr) {
                    match cl {
                        Closure::Lua(lc) => Val::Table(lc.env),
                        Closure::Rust(rc) => Val::Table(rc.env.unwrap_or(state.global)),
                    }
                } else {
                    Val::Table(state.global)
                }
            } else {
                Val::Table(state.global)
            }
        }
        SpecialIndex::Registry => Val::Table(state.registry),
        SpecialIndex::Upvalue(n) => {
            // PUC-Rio: U0 = globals, U1 = first upvalue (index 0), etc.
            if *n == 0 {
                return Val::Table(state.global);
            }
            let idx = *n - 1;
            if let Some(cr) = caller_closure_ref
                && let Some(Closure::Rust(rc)) = state.gc.closures.get(cr)
                && idx < rc.upvalues.len()
            {
                return rc.upvalues[idx];
            }
            Val::Nil
        }
    }
}

/// Set the value at a special index.
fn set_special(
    state: &mut LuaState,
    tc_base: usize,
    idx: &SpecialIndex,
    val: Val,
    caller_closure_ref: Option<GcRef<Closure>>,
) {
    match idx {
        SpecialIndex::Stack(i) => {
            if let Some(pos) = resolve_index(state, tc_base, *i) {
                state.ensure_stack(pos + 1);
                state.stack_set(pos, val);
            }
        }
        SpecialIndex::Env => {
            if let Val::Table(t) = val
                && let Some(cr) = caller_closure_ref
                && let Some(cl) = state.gc.closures.get_mut(cr)
            {
                match cl {
                    Closure::Lua(lc) => lc.env = t,
                    Closure::Rust(rc) => rc.env = Some(t),
                }
            }
        }
        SpecialIndex::Global => {
            if let Val::Table(t) = val {
                state.global = t;
            }
        }
        SpecialIndex::Registry => {
            // Can't replace registry
        }
        SpecialIndex::Upvalue(n) => {
            // PUC-Rio: U0 = globals (set global table), U1 = first upvalue
            if *n == 0 {
                if let Val::Table(t) = val {
                    state.global = t;
                }
                return;
            }
            let idx = *n - 1;
            if let Some(cr) = caller_closure_ref
                && let Some(Closure::Rust(rc)) = state.gc.closures.get_mut(cr)
                && idx < rc.upvalues.len()
            {
                rc.upvalues[idx] = val;
            }
        }
    }
}

/// Get a Lua string value from a stack position as a Rust String.
fn get_string(state: &LuaState, pos: usize) -> LuaResult<String> {
    match state.stack_get(pos) {
        Val::Str(r) => state
            .gc
            .string_arena
            .get(r)
            .map(|s| String::from_utf8_lossy(s.data()).to_string())
            .ok_or_else(|| rt_error("invalid string")),
        _ => Err(rt_error("string expected")),
    }
}

/// Get bytes from a Lua string value.
fn get_string_bytes(state: &LuaState, pos: usize) -> LuaResult<Vec<u8>> {
    match state.stack_get(pos) {
        Val::Str(r) => state
            .gc
            .string_arena
            .get(r)
            .map(|s| s.data().to_vec())
            .ok_or_else(|| rt_error("invalid string")),
        _ => Err(rt_error("string expected")),
    }
}

/// Load a file for testC's `loadfile` command.
/// Separates open vs read errors like PUC-Rio's `luaL_loadfile`.
fn load_file_for_testc(filename: &str) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let mut file =
        std::fs::File::open(filename).map_err(|e| format!("cannot open {filename}: {e}"))?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf)
        .map_err(|e| format!("cannot read {filename}: {e}"))?;
    Ok(buf)
}

/// PUC-Rio delimiter set: spaces, tabs, newlines, commas, semicolons.
fn is_delimit(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | ',' | ';')
}

/// Split program into tokens on PUC-Rio delimiters. Returns a `Vec<&str>`
/// of non-empty tokens.
fn tokenize_testc(prog: &str) -> Vec<&str> {
    prog.split(is_delimit).filter(|s| !s.is_empty()).collect()
}

/// Consume the next token from the iterator, matching PUC-Rio's `getnum_aux`.
/// If the token is `.`, pops the top stack value and converts to i32.
/// Otherwise parses as an integer with support for leading `-`.
fn next_num(tokens: &mut &[&str], state: &mut LuaState) -> i32 {
    let tok = if tokens.is_empty() {
        "0"
    } else {
        let t = tokens[0];
        *tokens = &tokens[1..];
        t
    };
    if tok == "." {
        let val = state.pop();
        match val {
            Val::Num(n) => n as i32,
            _ => 0,
        }
    } else {
        tok.parse().unwrap_or(0)
    }
}

/// Consume the next token as a name (string).
fn next_name<'a>(tokens: &mut &[&'a str]) -> &'a str {
    if tokens.is_empty() {
        ""
    } else {
        let t = tokens[0];
        *tokens = &tokens[1..];
        t
    }
}

/// Consume the next token as an index (PUC-Rio's `getindex_aux`).
/// Handles G, E, R, U<n>, `.` (pop from stack), and plain integers.
fn next_index(tokens: &mut &[&str], state: &mut LuaState) -> SpecialIndex {
    let tok = if tokens.is_empty() {
        "0"
    } else {
        let t = tokens[0];
        *tokens = &tokens[1..];
        t
    };
    match tok {
        "G" => SpecialIndex::Global,
        "E" => SpecialIndex::Env,
        "R" => SpecialIndex::Registry,
        _ if tok.starts_with('U') => {
            if let Ok(n) = tok[1..].parse::<usize>() {
                SpecialIndex::Upvalue(n)
            } else {
                SpecialIndex::Stack(0)
            }
        }
        "." => {
            let val = state.pop();
            let n = match val {
                Val::Num(n) => n as i32,
                _ => 0,
            };
            SpecialIndex::Stack(n)
        }
        _ => SpecialIndex::Stack(tok.parse().unwrap_or(0)),
    }
}

/// Execute the testC program commands.
///
/// Uses PUC-Rio's delimiter-based token parsing: the program string is split
/// into tokens on spaces, tabs, newlines, commas, and semicolons. Each
/// command consumes tokens as needed (matching PUC-Rio's getnum/getname/
/// getindex character-level parsing).
fn run_testc_program(
    state: &mut LuaState,
    prog: &str,
    tc_base: usize,
    caller_closure_ref: Option<GcRef<Closure>>,
) -> LuaResult<u32> {
    let all_tokens = tokenize_testc(prog);
    let mut tokens: &[&str] = &all_tokens;

    loop {
        let cmd = next_name(&mut tokens);
        if cmd.is_empty() {
            break;
        }

        match cmd {
            // --- Stack push operations ---
            "pushnum" => {
                let n = next_num(&mut tokens, state);
                state.ensure_stack(state.top + 1);
                state.push(Val::Num(f64::from(n)));
            }
            "pushbool" => {
                let b = next_num(&mut tokens, state);
                state.ensure_stack(state.top + 1);
                state.push(Val::Bool(b != 0));
            }
            "pushnil" => {
                state.ensure_stack(state.top + 1);
                state.push(Val::Nil);
            }
            "pushstring" => {
                let s = next_name(&mut tokens);
                let sr = state.gc.intern_string(s.as_bytes());
                state.ensure_stack(state.top + 1);
                state.push(Val::Str(sr));
            }
            "pushvalue" => {
                let si = next_index(&mut tokens, state);
                let val = get_special(state, tc_base, &si, caller_closure_ref);
                state.ensure_stack(state.top + 1);
                state.push(val);
            }
            "pushcclosure" => {
                let n = next_num(&mut tokens, state) as usize;
                let mut upvalues = Vec::with_capacity(n);
                for _ in 0..n {
                    upvalues.push(state.pop());
                }
                upvalues.reverse();

                let env = if let Some(cr) = caller_closure_ref {
                    if let Some(cl) = state.gc.closures.get(cr) {
                        match cl {
                            Closure::Lua(lc) => Some(lc.env),
                            Closure::Rust(rc) => rc.env.or(Some(state.global)),
                        }
                    } else {
                        Some(state.global)
                    }
                } else {
                    Some(state.global)
                };

                let closure = Closure::Rust(RustClosure {
                    func: t_testc,
                    upvalues,
                    name: "testC_closure".to_string(),
                    env,
                    native: 0,
                });
                let cr = state.gc.alloc_closure(closure);
                state.push(Val::Function(cr));
            }
            "pushupvalueindex" => {
                let n = next_num(&mut tokens, state) as usize;
                let val = get_special(
                    state,
                    tc_base,
                    &SpecialIndex::Upvalue(n),
                    caller_closure_ref,
                );
                state.push(val);
            }

            // --- Stack manipulation ---
            "gettop" => {
                let n = state.top.saturating_sub(tc_base);
                state.ensure_stack(state.top + 1);
                state.push(Val::Num(n as f64));
            }
            "settop" => {
                let n = next_num(&mut tokens, state);
                let new_top = if n >= 0 {
                    tc_base + n as usize
                } else {
                    let target = state.top as i64 + i64::from(n);
                    if target < tc_base as i64 {
                        tc_base
                    } else {
                        target as usize
                    }
                };
                state.ensure_stack(new_top);
                for i in state.top..new_top {
                    state.stack_set(i, Val::Nil);
                }
                state.top = new_top;
            }
            "pop" => {
                let n = next_num(&mut tokens, state) as usize;
                let new_top = if state.top >= n {
                    state.top - n
                } else {
                    tc_base
                };
                state.top = new_top;
            }
            "remove" => {
                let idx = next_num(&mut tokens, state);
                if let Some(pos) = resolve_index(state, tc_base, idx)
                    && pos < state.top
                {
                    for i in pos..state.top - 1 {
                        let v = state.stack_get(i + 1);
                        state.stack_set(i, v);
                    }
                    state.top -= 1;
                }
            }
            "insert" => {
                let idx = next_num(&mut tokens, state);
                if let Some(pos) = resolve_index(state, tc_base, idx)
                    && pos < state.top
                {
                    let top_val = state.stack_get(state.top - 1);
                    for i in (pos..state.top - 1).rev() {
                        let v = state.stack_get(i);
                        state.stack_set(i + 1, v);
                    }
                    state.stack_set(pos, top_val);
                }
            }
            "replace" => {
                // PUC-Rio resolves index BEFORE popping (lapi.c:177).
                let si = next_index(&mut tokens, state);
                match si {
                    SpecialIndex::Stack(i) => {
                        if let Some(abs_pos) = resolve_index(state, tc_base, i) {
                            let top_val = state.pop();
                            state.ensure_stack(abs_pos + 1);
                            state.stack_set(abs_pos, top_val);
                        } else {
                            state.pop();
                        }
                    }
                    other => {
                        let top_val = state.pop();
                        set_special(state, tc_base, &other, top_val, caller_closure_ref);
                    }
                }
            }

            // --- Type checks (use getindex like PUC-Rio) ---
            "isnumber" => {
                let si = next_index(&mut tokens, state);
                let val = get_special(state, tc_base, &si, caller_closure_ref);
                let is = match val {
                    Val::Num(_) => true,
                    Val::Str(r) => {
                        if let Some(s) = state.gc.string_arena.get(r) {
                            let text = String::from_utf8_lossy(s.data());
                            text.trim().parse::<f64>().is_ok()
                        } else {
                            false
                        }
                    }
                    _ => false,
                };
                state.push(Val::Num(if is { 1.0 } else { 0.0 }));
            }
            "isstring" => {
                let si = next_index(&mut tokens, state);
                let val = get_special(state, tc_base, &si, caller_closure_ref);
                let is = matches!(val, Val::Str(_) | Val::Num(_));
                state.push(Val::Num(if is { 1.0 } else { 0.0 }));
            }
            "isfunction" => {
                let si = next_index(&mut tokens, state);
                let val = get_special(state, tc_base, &si, caller_closure_ref);
                let is = matches!(val, Val::Function(_));
                state.push(Val::Num(if is { 1.0 } else { 0.0 }));
            }
            "iscfunction" => {
                let si = next_index(&mut tokens, state);
                let val = get_special(state, tc_base, &si, caller_closure_ref);
                let is = if let Val::Function(r) = val {
                    matches!(state.gc.closures.get(r), Some(Closure::Rust(_)))
                } else {
                    false
                };
                state.push(Val::Num(if is { 1.0 } else { 0.0 }));
            }
            "istable" => {
                let si = next_index(&mut tokens, state);
                let val = get_special(state, tc_base, &si, caller_closure_ref);
                let is = matches!(val, Val::Table(_));
                state.push(Val::Num(if is { 1.0 } else { 0.0 }));
            }
            "isuserdata" => {
                let si = next_index(&mut tokens, state);
                let val = get_special(state, tc_base, &si, caller_closure_ref);
                let is = matches!(val, Val::Userdata(_) | Val::LightUserdata(_));
                state.push(Val::Num(if is { 1.0 } else { 0.0 }));
            }
            "isudataval" => {
                let si = next_index(&mut tokens, state);
                let val = get_special(state, tc_base, &si, caller_closure_ref);
                let is = matches!(val, Val::LightUserdata(_));
                state.push(Val::Num(if is { 1.0 } else { 0.0 }));
            }
            "isnil" => {
                let si = next_index(&mut tokens, state);
                // PUC-Rio: lua_isnil returns false for out-of-range indices
                // (those are LUA_TNONE, not LUA_TNIL).
                let is = if is_valid_index(state, tc_base, &si) {
                    let val = get_special(state, tc_base, &si, caller_closure_ref);
                    val.is_nil()
                } else {
                    false
                };
                state.push(Val::Num(if is { 1.0 } else { 0.0 }));
            }
            "isnull" => {
                let si = next_index(&mut tokens, state);
                let is_null = match si {
                    SpecialIndex::Stack(i) => {
                        if let Some(pos) = resolve_index(state, tc_base, i) {
                            pos >= state.top
                        } else {
                            true
                        }
                    }
                    _ => false,
                };
                state.push(Val::Num(if is_null { 1.0 } else { 0.0 }));
            }

            // --- Conversions ---
            "tostring" => {
                let si = next_index(&mut tokens, state);
                let val = get_special(state, tc_base, &si, caller_closure_ref);
                let result = match val {
                    Val::Str(_) => val,
                    Val::Num(_) => {
                        let s = format!("{val}");
                        let sr = state.gc.intern_string(s.as_bytes());
                        Val::Str(sr)
                    }
                    Val::Bool(b) => {
                        let sr = state.gc.intern_string(if b { b"true" } else { b"false" });
                        Val::Str(sr)
                    }
                    _ => Val::Nil,
                };
                state.push(result);
            }
            "tonumber" => {
                let si = next_index(&mut tokens, state);
                let val = get_special(state, tc_base, &si, caller_closure_ref);
                let n = match val {
                    Val::Num(n) => n,
                    Val::Str(r) => {
                        if let Some(s) = state.gc.string_arena.get(r) {
                            let text = String::from_utf8_lossy(s.data());
                            text.trim().parse::<f64>().unwrap_or(0.0)
                        } else {
                            0.0
                        }
                    }
                    _ => 0.0,
                };
                state.push(Val::Num(n));
            }
            "tobool" => {
                let si = next_index(&mut tokens, state);
                let val = get_special(state, tc_base, &si, caller_closure_ref);
                let b = val.is_truthy();
                state.push(Val::Num(if b { 1.0 } else { 0.0 }));
            }
            "tocfunction" => {
                let si = next_index(&mut tokens, state);
                let val = get_special(state, tc_base, &si, caller_closure_ref);
                if let Val::Function(r) = val {
                    if let Some(Closure::Rust(rc)) = state.gc.closures.get(r) {
                        let func = rc.func;
                        let name = rc.name.clone();
                        let new_cl = Closure::Rust(RustClosure {
                            func,
                            upvalues: Vec::new(),
                            name,
                            env: None,
                            native: 0,
                        });
                        let cr = state.gc.alloc_closure(new_cl);
                        state.push(Val::Function(cr));
                    } else {
                        state.push(Val::Nil);
                    }
                } else {
                    state.push(Val::Nil);
                }
            }
            "objsize" => {
                let si = next_index(&mut tokens, state);
                let val = get_special(state, tc_base, &si, caller_closure_ref);
                let size = match val {
                    Val::Str(r) => state.gc.string_arena.get(r).map_or(0, |s| s.data().len()),
                    Val::Table(r) => {
                        let strings = &state.gc.string_arena;
                        state.gc.tables.get(r).map_or(0, |t| t.len(strings))
                    }
                    Val::Userdata(r) => state.gc.userdata.get(r).map_or(0, |ud| {
                        ud.downcast_ref::<TestUserdata>().map_or(0, |tu| tu.size)
                    }),
                    Val::Num(_) => {
                        // PUC-Rio's lua_objlen coerces numbers to strings.
                        let s = format!("{val}");
                        s.len()
                    }
                    _ => 0,
                };
                state.push(Val::Num(size as f64));
            }
            "type" => {
                let idx = next_num(&mut tokens, state);
                if let Some(pos) = resolve_index(state, tc_base, idx) {
                    if pos < state.top {
                        let val = state.stack_get(pos);
                        let name = val.type_name();
                        let s = state.gc.intern_string(name.as_bytes());
                        state.push(Val::Str(s));
                    } else {
                        let s = state.gc.intern_string(b"no value");
                        state.push(Val::Str(s));
                    }
                } else {
                    let s = state.gc.intern_string(b"nil");
                    state.push(Val::Str(s));
                }
            }
            "newuserdata" => {
                let size = next_num(&mut tokens, state) as usize;
                let ud = create_test_userdata(state, size);
                if let Some(cr) = caller_closure_ref
                    && let Some(cl) = state.gc.closures.get(cr)
                {
                    let env = match cl {
                        Closure::Lua(lc) => lc.env,
                        Closure::Rust(rc) => rc.env.unwrap_or(state.global),
                    };
                    if let Some(ud_obj) = state.gc.userdata.get_mut(ud) {
                        ud_obj.set_env(Some(env));
                    }
                }
                state.push(Val::Userdata(ud));
            }

            // --- Table operations ---
            "gettable" => {
                let si = next_index(&mut tokens, state);
                let table_val = get_special(state, tc_base, &si, caller_closure_ref);
                let key = state.pop();
                let result = state.gettable(table_val, key)?;
                state.push(result);
            }
            "settable" => {
                let si = next_index(&mut tokens, state);
                let table_val = get_special(state, tc_base, &si, caller_closure_ref);
                let value = state.pop();
                let key = state.pop();
                state.settable(table_val, key, value)?;
            }
            "next" => {
                let key = state.pop();
                let table_val = state.stack_get(state.top - 1);
                let Val::Table(table_ref) = table_val else {
                    return Err(rt_error("table expected for next"));
                };
                let result = state
                    .gc
                    .tables
                    .get(table_ref)
                    .ok_or_else(|| rt_error("invalid table"))?
                    .next(key, &state.gc.string_arena)?;
                if let Some((k, v)) = result {
                    state.push(k);
                    state.push(v);
                } else {
                    state.push(Val::Nil);
                }
            }
            "concat" => {
                let n = next_num(&mut tokens, state) as usize;
                state.api_concat(n)?;
            }

            // --- Comparison operations (use getindex for both args) ---
            "lessthan" => {
                let a_si = next_index(&mut tokens, state);
                let b_si = next_index(&mut tokens, state);
                let a_val = get_special(state, tc_base, &a_si, caller_closure_ref);
                let b_val = get_special(state, tc_base, &b_si, caller_closure_ref);
                let result = state.api_lessthan(a_val, b_val)?;
                state.push(Val::Bool(result));
            }
            "equal" => {
                let a_si = next_index(&mut tokens, state);
                let b_si = next_index(&mut tokens, state);
                // PUC-Rio's lua_equal returns 0 if either index is invalid
                // (luaO_nilobject), even though nil == nil would be true.
                let a_valid = is_valid_index(state, tc_base, &a_si);
                let b_valid = is_valid_index(state, tc_base, &b_si);
                if !a_valid || !b_valid {
                    state.push(Val::Bool(false));
                } else {
                    let a_val = get_special(state, tc_base, &a_si, caller_closure_ref);
                    let b_val = get_special(state, tc_base, &b_si, caller_closure_ref);
                    let result = state.api_equal(a_val, b_val)?;
                    state.push(Val::Bool(result));
                }
            }

            // --- Metatable operations ---
            "setmetatable" => {
                let si = next_index(&mut tokens, state);
                let target = get_special(state, tc_base, &si, caller_closure_ref);
                let mt = state.pop();
                let mt_ref = if let Val::Table(r) = mt {
                    Some(r)
                } else {
                    None
                };
                match target {
                    Val::Table(r) => {
                        if let Some(t) = state.gc.tables.get_mut(r) {
                            t.set_metatable(mt_ref);
                        }
                    }
                    Val::Userdata(r) => {
                        if let Some(ud) = state.gc.userdata.get_mut(r) {
                            ud.set_metatable(mt_ref);
                        }
                    }
                    _ => {}
                }
            }
            "getmetatable" => {
                let si = next_index(&mut tokens, state);
                let target = get_special(state, tc_base, &si, caller_closure_ref);
                let mt = match target {
                    Val::Table(r) => state.gc.tables.get(r).and_then(Table::metatable),
                    Val::Userdata(r) => state.gc.userdata.get(r).and_then(Userdata::metatable),
                    _ => None,
                };
                if let Some(mt_ref) = mt {
                    state.push(Val::Table(mt_ref));
                } else {
                    state.push(Val::Nil);
                }
            }

            // --- Call operations ---
            // PUC-Rio: rawcall = lua_call (unprotected), call = lua_pcall (protected)
            "rawcall" => {
                let nargs = next_num(&mut tokens, state);
                let nresults = next_num(&mut tokens, state);
                let func_pos = state.top - nargs as usize - 1;
                state.call_function(func_pos, nresults)?;
            }
            "call" => {
                let nargs = next_num(&mut tokens, state);
                let nresults = next_num(&mut tokens, state);
                let func_pos = state.top - nargs as usize - 1;

                let saved_ci = state.ci;
                let saved_n_ccalls = state.n_ccalls;
                let saved_call_depth = state.call_depth;
                state.error_object = None;

                match state.call_function(func_pos, nresults) {
                    Ok(()) => {}
                    Err(err) => {
                        state.ci = saved_ci;
                        state.base = state.call_stack[state.ci].base;
                        state.n_ccalls = saved_n_ccalls;
                        state.call_depth = saved_call_depth;
                        if state.ci < crate::vm::state::MAXCALLS {
                            state.ci_overflow = false;
                        }
                        state.close_upvalues(func_pos);
                        let error_val = state.error_object.take().unwrap_or_else(|| {
                            let r = state.gc.intern_string(err.to_string().as_bytes());
                            Val::Str(r)
                        });
                        state.top = func_pos;
                        state.push(error_val);
                    }
                }
            }
            "pcall" => {
                let nargs = next_num(&mut tokens, state);
                let nresults = next_num(&mut tokens, state);
                let func_pos = state.top - nargs as usize - 1;

                let saved_ci = state.ci;
                let saved_n_ccalls = state.n_ccalls;
                let saved_call_depth = state.call_depth;
                state.error_object = None;

                match state.call_function(func_pos, nresults) {
                    Ok(()) => {
                        state.push(Val::Num(0.0));
                    }
                    Err(err) => {
                        state.ci = saved_ci;
                        state.base = state.call_stack[state.ci].base;
                        state.n_ccalls = saved_n_ccalls;
                        state.call_depth = saved_call_depth;
                        if state.ci < crate::vm::state::MAXCALLS {
                            state.ci_overflow = false;
                        }
                        state.close_upvalues(func_pos);
                        let error_val = state.error_object.take().unwrap_or_else(|| {
                            let r = state.gc.intern_string(err.to_string().as_bytes());
                            Val::Str(r)
                        });
                        state.top = func_pos;
                        state.push(error_val);
                    }
                }
            }
            "loadstring" => {
                let idx = next_num(&mut tokens, state);
                let pos = resolve_index(state, tc_base, idx).unwrap_or(tc_base);
                if pos < state.top {
                    let source = get_string_bytes(state, pos)?;
                    let name = String::from_utf8_lossy(&source).to_string();
                    let chunk_name = format!("={name}");
                    match crate::compile_or_undump(&source, &chunk_name) {
                        Ok(proto) => {
                            let mut proto = crate::vm::proto::ProtoRef::try_unwrap(proto)
                                .unwrap_or_else(|rc| (*rc).clone());
                            crate::patch_string_constants(&mut proto, &mut state.gc);
                            let proto = crate::vm::proto::ProtoRef::new(proto);
                            let num_upvalues = proto.num_upvalues as usize;
                            let mut lua_cl =
                                crate::vm::closure::LuaClosure::new(proto, state.global);
                            for _ in 0..num_upvalues {
                                let uv = crate::vm::closure::Upvalue::new_closed(Val::Nil);
                                let uv_ref = state.gc.alloc_upvalue(uv);
                                lua_cl.upvalues.push(uv_ref);
                            }
                            let cr = state.gc.alloc_closure(Closure::Lua(lua_cl));
                            state.push(Val::Function(cr));
                        }
                        Err(e) => {
                            let msg_bytes = match &e {
                                LuaError::Syntax(syn) => syn.to_lua_bytes(),
                                _ => e.to_string().into_bytes(),
                            };
                            let msg = state.gc.intern_string(&msg_bytes);
                            state.push(Val::Str(msg));
                        }
                    }
                } else {
                    let s = state.gc.intern_string(b"no string to load");
                    state.push(Val::Str(s));
                }
            }
            "loadfile" => {
                let idx = next_num(&mut tokens, state);
                let pos = resolve_index(state, tc_base, idx).unwrap_or(tc_base);
                if pos < state.top {
                    let filename = get_string(state, pos)?;
                    match load_file_for_testc(&filename) {
                        Ok(source) => {
                            let name = format!("@{filename}");
                            match crate::compile_or_undump(&source, &name) {
                                Ok(proto) => {
                                    let mut proto = crate::vm::proto::ProtoRef::try_unwrap(proto)
                                        .unwrap_or_else(|rc| (*rc).clone());
                                    crate::patch_string_constants(&mut proto, &mut state.gc);
                                    let proto = crate::vm::proto::ProtoRef::new(proto);
                                    let num_upvalues = proto.num_upvalues as usize;
                                    let mut lua_cl =
                                        crate::vm::closure::LuaClosure::new(proto, state.global);
                                    for _ in 0..num_upvalues {
                                        let uv = crate::vm::closure::Upvalue::new_closed(Val::Nil);
                                        let uv_ref = state.gc.alloc_upvalue(uv);
                                        lua_cl.upvalues.push(uv_ref);
                                    }
                                    let cr = state.gc.alloc_closure(Closure::Lua(lua_cl));
                                    state.push(Val::Function(cr));
                                }
                                Err(e) => {
                                    let msg_bytes = match &e {
                                        LuaError::Syntax(syn) => syn.to_lua_bytes(),
                                        _ => e.to_string().into_bytes(),
                                    };
                                    let msg = state.gc.intern_string(&msg_bytes);
                                    state.push(Val::Str(msg));
                                }
                            }
                        }
                        Err(msg) => {
                            let s = state.gc.intern_string(msg.as_bytes());
                            state.push(Val::Str(s));
                        }
                    }
                } else {
                    let s = state.gc.intern_string(b"cannot read file");
                    state.push(Val::Str(s));
                }
            }

            // --- Getn ---
            "getn" => {
                let si = next_index(&mut tokens, state);
                let val = get_special(state, tc_base, &si, caller_closure_ref);
                let len = if let Val::Table(r) = val {
                    let strings = &state.gc.string_arena;
                    state.gc.tables.get(r).map_or(0, |t| t.len(strings))
                } else {
                    0
                };
                state.push(Val::Num(len as f64));
            }

            // --- Return ---
            "return" => {
                let n = next_num(&mut tokens, state) as usize;
                let start = if state.top >= n {
                    state.top - n
                } else {
                    tc_base
                };
                let actual = state.top - start;
                for i in 0..actual {
                    let v = state.stack_get(start + i);
                    state.stack_set(state.base + i, v);
                }
                state.top = state.base + actual;
                return Ok(actual as u32);
            }

            _ => {
                return Err(rt_error(&format!("unknown testC instruction: {cmd}")));
            }
        }
    }

    state.top = state.base;
    Ok(0)
}

/// Handle `T.testC(L, prog)` — run testC on a remote state.
fn testc_remote(state: &mut LuaState, ud_ref: GcRef<Userdata>) -> LuaResult<u32> {
    // Get the program string (second argument).
    if state.base + 1 >= state.top {
        return Err(rt_error("testC: missing program string for remote state"));
    }
    let prog = get_string(state, state.base + 1)?;

    // Get the remote state from the userdata.
    let remote_ptr = state
        .gc
        .userdata
        .get(ud_ref)
        .and_then(|ud| ud.downcast_ref::<RemoteState>())
        .map(|rs| rs.ptr)
        .ok_or_else(|| rt_error("testC: invalid remote state"))?;

    // Safety: we trust that the pointer is valid (created by T.newstate).
    #[allow(unsafe_code)]
    let remote = unsafe { &mut *remote_ptr };

    // Run the program on the remote state.
    run_testc_program(remote, &prog, remote.base, None)?;

    // No results returned to the calling state.
    state.top = state.base;
    Ok(0)
}

// =========================================================================
// Test userdata type (for T.newuserdata / T.objsize)
// =========================================================================

/// Data stored inside test userdata created by `T.newuserdata`.
/// Stores the requested size and allocates that many bytes so the GC
/// tracks the memory correctly.
struct TestUserdata {
    size: usize,
    id: usize,
    _data: Vec<u8>,
}

/// Global counter for userdata IDs.
static USERDATA_NEXT_ID: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Create a test userdata with the given size and a unique ID.
/// Adds the data size to GC's total_bytes so `collectgarbage("count")`
/// reflects the allocation.
fn create_test_userdata(state: &mut LuaState, size: usize) -> GcRef<Userdata> {
    let id = USERDATA_NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let ud = Userdata::new(Box::new(TestUserdata {
        size,
        id,
        _data: vec![0u8; size],
    }));
    // Account for the actual data allocation in GC memory tracking.
    state.gc.gc_state.total_bytes += size;
    state.gc.alloc_userdata(ud)
}

// =========================================================================
// T.newuserdata(size)
// =========================================================================

/// Creates a new userdata with the given byte size.
pub fn t_newuserdata(state: &mut LuaState) -> LuaResult<u32> {
    let size: usize = if state.base < state.top {
        match state.stack_get(state.base) {
            Val::Num(n) => n as usize,
            _ => 0,
        }
    } else {
        0
    };

    let ud_ref = create_test_userdata(state, size);
    // Userdata inherits environment from global (default behavior).
    state.push(Val::Userdata(ud_ref));
    Ok(1)
}

// =========================================================================
// T.udataval(ud)
// =========================================================================

/// Returns the unique integer ID associated with a userdata.
pub fn t_udataval(state: &mut LuaState) -> LuaResult<u32> {
    if state.base >= state.top {
        return Err(type_error(state, 0, "userdata"));
    }
    let val = state.stack_get(state.base);
    let Val::Userdata(r) = val else {
        return Err(type_error(state, 0, "userdata"));
    };
    let id = state
        .gc
        .userdata
        .get(r)
        .and_then(|ud| ud.downcast_ref::<TestUserdata>())
        .map_or(0, |tu| tu.id);
    state.push(Val::Num(id as f64));
    Ok(1)
}

// =========================================================================
// T.pushuserdata(id)
// =========================================================================

/// Finds or creates userdata by its integer ID.
///
/// Scans the userdata arena for a `TestUserdata` with matching ID.
/// If not found, creates a new one with that ID.
pub fn t_pushuserdata(state: &mut LuaState) -> LuaResult<u32> {
    let id: usize = if state.base < state.top {
        match state.stack_get(state.base) {
            Val::Num(n) => n as usize,
            _ => 0,
        }
    } else {
        0
    };

    // Scan the arena for existing userdata with this ID.
    let existing: Option<GcRef<Userdata>> = state
        .gc
        .userdata
        .iter()
        .find(|(_, ud, _)| {
            ud.downcast_ref::<TestUserdata>()
                .is_some_and(|tu| tu.id == id)
        })
        .map(|(r, _, _)| r);

    if let Some(ud_ref) = existing {
        state.push(Val::Userdata(ud_ref));
        return Ok(1);
    }

    // Create a new one with this ID.
    let ud = Userdata::new(Box::new(TestUserdata {
        size: 0,
        id,
        _data: Vec::new(),
    }));
    let ud_ref = state.gc.alloc_userdata(ud);
    state.push(Val::Userdata(ud_ref));
    Ok(1)
}

// =========================================================================
// T.ref(obj), T.unref(key), T.getref(key)
// =========================================================================

/// Constant for nil reference (PUC-Rio's LUA_REFNIL = -1).
const LUA_REFNIL: i32 = -1;

/// Stores a value in the registry and returns an integer key.
/// Matches PUC-Rio's `luaL_ref`.
pub fn t_ref(state: &mut LuaState) -> LuaResult<u32> {
    let val = if state.base < state.top {
        state.stack_get(state.base)
    } else {
        Val::Nil
    };

    if val.is_nil() {
        state.push(Val::Num(f64::from(LUA_REFNIL)));
        return Ok(1);
    }

    // Implementation of luaL_ref: use registry["n"] as the next free slot.
    // If registry[free] exists, use it as the next free pointer.
    let registry = state.registry;
    let n_key = state.gc.intern_string(b"n");

    // Get current "n" from registry.
    let current_n = state
        .gc
        .tables
        .get(registry)
        .map_or(Val::Nil, |t| t.get(Val::Str(n_key), &state.gc.string_arena));

    // Check free list: registry[0] points to first free slot.
    let zero_key = Val::Num(0.0);
    let free_ref = state
        .gc
        .tables
        .get(registry)
        .map_or(Val::Nil, |t| t.get(zero_key, &state.gc.string_arena));

    let ref_key: i32;
    if let Val::Num(free_n) = free_ref {
        let free_idx = free_n as i32;
        if free_idx > 0 {
            // Reuse free slot. Get the next free from registry[free_idx].
            let next_free = state.gc.tables.get(registry).map_or(Val::Nil, |t| {
                t.get(Val::Num(f64::from(free_idx)), &state.gc.string_arena)
            });
            // Update free list head.
            if let Some(t) = state.gc.tables.get_mut(registry) {
                t.raw_set(zero_key, next_free, &state.gc.string_arena)?;
            }
            ref_key = free_idx;
        } else {
            // No free slots, extend.
            let n = match current_n {
                Val::Num(x) => x as i32,
                _ => 0,
            };
            ref_key = n + 1;
            if let Some(t) = state.gc.tables.get_mut(registry) {
                t.raw_set(
                    Val::Str(n_key),
                    Val::Num(f64::from(ref_key)),
                    &state.gc.string_arena,
                )?;
            }
        }
    } else {
        // First ref ever.
        let n = match current_n {
            Val::Num(x) => x as i32,
            _ => 0,
        };
        ref_key = n + 1;
        if let Some(t) = state.gc.tables.get_mut(registry) {
            t.raw_set(
                Val::Str(n_key),
                Val::Num(f64::from(ref_key)),
                &state.gc.string_arena,
            )?;
        }
    }

    // Store the value.
    if let Some(t) = state.gc.tables.get_mut(registry) {
        t.raw_set(Val::Num(f64::from(ref_key)), val, &state.gc.string_arena)?;
    }

    state.push(Val::Num(f64::from(ref_key)));
    Ok(1)
}

/// Removes a registry entry. Matches PUC-Rio's `luaL_unref`.
pub fn t_unref(state: &mut LuaState) -> LuaResult<u32> {
    let key: i32 = if state.base < state.top {
        match state.stack_get(state.base) {
            Val::Num(n) => n as i32,
            _ => return Ok(0),
        }
    } else {
        return Ok(0);
    };

    if key <= 0 {
        return Ok(0); // LUA_REFNIL and LUA_NOREF are no-ops.
    }

    let registry = state.registry;
    let zero_key = Val::Num(0.0);

    // Get current free list head.
    let current_free = state
        .gc
        .tables
        .get(registry)
        .map_or(Val::Nil, |t| t.get(zero_key, &state.gc.string_arena));

    // Point this slot to the current free list head.
    if let Some(t) = state.gc.tables.get_mut(registry) {
        t.raw_set(
            Val::Num(f64::from(key)),
            current_free,
            &state.gc.string_arena,
        )?;
    }

    // Update free list head to point to this slot.
    if let Some(t) = state.gc.tables.get_mut(registry) {
        t.raw_set(zero_key, Val::Num(f64::from(key)), &state.gc.string_arena)?;
    }

    Ok(0)
}

/// Gets a value from the registry by key.
pub fn t_getref(state: &mut LuaState) -> LuaResult<u32> {
    let key: i32 = if state.base < state.top {
        if let Val::Num(n) = state.stack_get(state.base) {
            n as i32
        } else {
            state.push(Val::Nil);
            return Ok(1);
        }
    } else {
        state.push(Val::Nil);
        return Ok(1);
    };

    if key == LUA_REFNIL {
        state.push(Val::Nil);
        return Ok(1);
    }

    let registry = state.registry;
    let val = state.gc.tables.get(registry).map_or(Val::Nil, |t| {
        t.get(Val::Num(f64::from(key)), &state.gc.string_arena)
    });

    state.push(val);
    Ok(1)
}

// =========================================================================
// T.upvalue(f, n [, val])
// =========================================================================

/// Gets or sets an upvalue of a closure.
///
/// With 2 args: returns the current value of upvalue n (1-based).
/// With 3 args: sets upvalue n to val and returns the old value.
pub fn t_upvalue(state: &mut LuaState) -> LuaResult<u32> {
    if state.base >= state.top {
        return Err(arg_error(state, 1, "function expected"));
    }
    let func_val = state.stack_get(state.base);
    let Val::Function(func_ref) = func_val else {
        return Err(arg_error(state, 1, "function expected"));
    };

    let n: usize = if state.base + 1 < state.top {
        match state.stack_get(state.base + 1) {
            Val::Num(x) => x as usize,
            _ => 1,
        }
    } else {
        1
    };

    let has_new_val = state.base + 2 < state.top;
    let new_val = if has_new_val {
        state.stack_get(state.base + 2)
    } else {
        Val::Nil
    };

    // Get the upvalue. n is 1-based.
    let cl = state
        .gc
        .closures
        .get(func_ref)
        .ok_or_else(|| arg_error(state, 1, "invalid function"))?;

    match cl {
        Closure::Lua(lc) => {
            let idx = n - 1;
            if idx >= lc.upvalues.len() {
                state.push(Val::Nil);
                return Ok(1);
            }
            let uv_ref = lc.upvalues[idx];
            let old_val = state
                .gc
                .upvalues
                .get(uv_ref)
                .map_or(Val::Nil, |uv| uv.get(&state.stack));

            if has_new_val && let Some(uv) = state.gc.upvalues.get_mut(uv_ref) {
                uv.set(&mut state.stack, new_val);
            }
            state.push(old_val);
        }
        Closure::Rust(rc) => {
            let idx = n - 1;
            if idx >= rc.upvalues.len() {
                state.push(Val::Nil);
                return Ok(1);
            }
            let old_val = rc.upvalues[idx];

            if has_new_val {
                // Need mutable access.
                let _ = cl;
                if let Some(Closure::Rust(rc)) = state.gc.closures.get_mut(func_ref) {
                    rc.upvalues[idx] = new_val;
                }
            }
            state.push(old_val);
        }
    }

    Ok(1)
}

// =========================================================================
// T.checkmemory()
// =========================================================================

/// No-op GC consistency check stub.
pub fn t_checkmemory(state: &mut LuaState) -> LuaResult<u32> {
    let _ = state;
    Ok(0)
}

// =========================================================================
// T.gsub(s, p, r)
// =========================================================================

/// Simple string substitution (equivalent to PUC-Rio's `luaL_gsub`).
///
/// Replaces all occurrences of pattern `p` in string `s` with `r`.
/// This is a literal substring replacement (not a Lua pattern).
pub fn t_gsub(state: &mut LuaState) -> LuaResult<u32> {
    let s = get_string(state, state.base)?;
    let p = get_string(state, state.base + 1)?;
    let r = get_string(state, state.base + 2)?;

    let result = if p.is_empty() {
        s
    } else {
        let mut out = String::new();
        let mut rest = s.as_str();
        while let Some(pos) = rest.find(&p) {
            out.push_str(&rest[..pos]);
            out.push_str(&r);
            rest = &rest[pos + p.len()..];
        }
        out.push_str(rest);
        out
    };

    let sr = state.gc.intern_string(result.as_bytes());
    state.push(Val::Str(sr));
    Ok(1)
}

// =========================================================================
// T.doonnewstack(code)
// =========================================================================

/// Runs Lua code in a new coroutine thread.
///
/// Creates a new coroutine, loads the code string into it, and resumes it.
/// Returns the status (0 = success).
pub fn t_doonnewstack(state: &mut LuaState) -> LuaResult<u32> {
    let code = get_string_bytes(state, state.base)?;

    // Compile the code.
    match crate::compile_or_undump(&code, "=doonnewstack") {
        Ok(proto) => {
            let mut proto =
                crate::vm::proto::ProtoRef::try_unwrap(proto).unwrap_or_else(|rc| (*rc).clone());
            crate::patch_string_constants(&mut proto, &mut state.gc);
            let proto = crate::vm::proto::ProtoRef::new(proto);
            let num_upvalues = proto.num_upvalues as usize;
            let mut lua_cl = crate::vm::closure::LuaClosure::new(proto, state.global);
            for _ in 0..num_upvalues {
                let uv = crate::vm::closure::Upvalue::new_closed(Val::Nil);
                let uv_ref = state.gc.alloc_upvalue(uv);
                lua_cl.upvalues.push(uv_ref);
            }
            let cr = state.gc.alloc_closure(Closure::Lua(lua_cl));
            let func_val = Val::Function(cr);

            // Create coroutine thread with the function.
            let thread = crate::vm::state::LuaThread::new(func_val, state.global);
            let co_ref = state.gc.alloc_thread(thread);

            // Resume the coroutine.
            match coroutine::auxresume(state, co_ref, &[]) {
                Ok(_) => {
                    state.push(Val::Num(0.0));
                }
                Err(err) => {
                    state.push(Val::Num(1.0));
                    state.push(err);
                    return Ok(2);
                }
            }
        }
        Err(e) => {
            state.push(Val::Num(1.0));
            let msg = state.gc.intern_string(e.to_string().as_bytes());
            state.push(Val::Str(msg));
            return Ok(2);
        }
    }

    Ok(1)
}

// =========================================================================
// Remote state support: T.newstate, T.closestate, T.doremote, T.loadlib
// =========================================================================

/// Data stored in the userdata wrapping a remote LuaState.
struct RemoteState {
    ptr: *mut LuaState,
}

// RemoteState must be Send for the Mutex-wrapped global state.
// The pointer is only accessed from a single thread at a time.
#[allow(unsafe_code)]
unsafe impl Send for RemoteState {}

/// Creates a new independent Lua state, returned as a userdata.
pub fn t_newstate(state: &mut LuaState) -> LuaResult<u32> {
    let new_state = Box::new(LuaState::new());
    let ptr = Box::into_raw(new_state);
    let ud = Userdata::new(Box::new(RemoteState { ptr }));
    let ud_ref = state.gc.alloc_userdata(ud);
    state.push(Val::Userdata(ud_ref));
    Ok(1)
}

/// Closes a state created by `T.newstate`.
pub fn t_closestate(state: &mut LuaState) -> LuaResult<u32> {
    if state.base >= state.top {
        return Err(rt_error("closestate: missing state argument"));
    }
    let val = state.stack_get(state.base);
    let Val::Userdata(ud_ref) = val else {
        return Err(rt_error("closestate: userdata expected"));
    };
    let ptr = state
        .gc
        .userdata
        .get(ud_ref)
        .and_then(|ud| ud.downcast_ref::<RemoteState>())
        .map(|rs| rs.ptr)
        .ok_or_else(|| rt_error("closestate: invalid remote state"))?;

    // Safety: we created this pointer in t_newstate.
    #[allow(unsafe_code)]
    unsafe {
        drop(Box::from_raw(ptr));
    }
    Ok(0)
}

/// Executes code in a remote state and returns results as strings.
pub fn t_doremote(state: &mut LuaState) -> LuaResult<u32> {
    if state.base + 1 >= state.top {
        return Err(rt_error("doremote: missing arguments"));
    }

    let ud_val = state.stack_get(state.base);
    let Val::Userdata(ud_ref) = ud_val else {
        return Err(rt_error("doremote: userdata expected"));
    };

    let code = get_string_bytes(state, state.base + 1)?;

    let remote_ptr = state
        .gc
        .userdata
        .get(ud_ref)
        .and_then(|ud| ud.downcast_ref::<RemoteState>())
        .map(|rs| rs.ptr)
        .ok_or_else(|| rt_error("doremote: invalid remote state"))?;

    // Safety: pointer created by t_newstate.
    #[allow(unsafe_code)]
    let remote = unsafe { &mut *remote_ptr };

    // Compile and run in the remote state.
    match crate::compile_or_undump(&code, "=doremote") {
        Ok(proto) => {
            let mut proto =
                crate::vm::proto::ProtoRef::try_unwrap(proto).unwrap_or_else(|rc| (*rc).clone());
            crate::patch_string_constants(&mut proto, &mut remote.gc);
            let proto = crate::vm::proto::ProtoRef::new(proto);
            let num_upvalues = proto.num_upvalues as usize;
            let mut lua_cl = crate::vm::closure::LuaClosure::new(proto, remote.global);
            for _ in 0..num_upvalues {
                let uv = crate::vm::closure::Upvalue::new_closed(Val::Nil);
                let uv_ref = remote.gc.alloc_upvalue(uv);
                lua_cl.upvalues.push(uv_ref);
            }
            let cr = remote.gc.alloc_closure(Closure::Lua(lua_cl));

            let func_pos = remote.top;
            remote.ensure_stack(func_pos + 1);
            remote.stack_set(func_pos, Val::Function(cr));
            remote.top = func_pos + 1;

            match remote.call_function(func_pos, -1) {
                Ok(()) => {
                    // Collect results from remote state and return as strings.
                    let n_results = remote.top - func_pos;
                    for i in 0..n_results {
                        let v = remote.stack_get(func_pos + i);
                        let s = match v {
                            Val::Str(r) => remote
                                .gc
                                .string_arena
                                .get(r)
                                .map(|s| s.data().to_vec())
                                .unwrap_or_default(),
                            Val::Bool(true) => b"true".to_vec(),
                            Val::Bool(false) => b"false".to_vec(),
                            Val::Nil => b"nil".to_vec(),
                            _ => format!("{v}").into_bytes(),
                        };
                        let sr = state.gc.intern_string(&s);
                        state.push(Val::Str(sr));
                    }
                    remote.top = func_pos;
                    Ok(n_results as u32)
                }
                Err(err) => {
                    // Error: return nil, error_code, error_message.
                    remote.top = func_pos;
                    state.push(Val::Nil);
                    let code = match &err {
                        LuaError::Syntax(_) => 3,
                        _ => 2,
                    };
                    state.push(Val::Num(f64::from(code)));
                    let msg = state.gc.intern_string(err.to_string().as_bytes());
                    state.push(Val::Str(msg));
                    Ok(3)
                }
            }
        }
        Err(e) => {
            state.push(Val::Nil);
            state.push(Val::Num(3.0)); // syntax error
            let msg = state.gc.intern_string(e.to_string().as_bytes());
            state.push(Val::Str(msg));
            Ok(3)
        }
    }
}

/// Loads standard libraries into a remote state.
///
/// Registers global functions that open each standard library:
/// `baselibopen`, `strlibopen`, `tablibopen`, `iolibopen`, `mathlibopen`,
/// `dblibopen`, `packageopen`.
pub fn t_loadlib(state: &mut LuaState) -> LuaResult<u32> {
    if state.base >= state.top {
        return Err(rt_error("loadlib: missing state argument"));
    }

    let ud_val = state.stack_get(state.base);
    let Val::Userdata(ud_ref) = ud_val else {
        return Err(rt_error("loadlib: userdata expected"));
    };

    let remote_ptr = state
        .gc
        .userdata
        .get(ud_ref)
        .and_then(|ud| ud.downcast_ref::<RemoteState>())
        .map(|rs| rs.ptr)
        .ok_or_else(|| rt_error("loadlib: invalid remote state"))?;

    #[allow(unsafe_code)]
    let remote = unsafe { &mut *remote_ptr };

    // Register library opener functions as globals.
    #[allow(clippy::type_complexity)]
    let openers: &[(&str, fn(&mut LuaState) -> LuaResult<u32>)] = &[
        ("baselibopen", lib_open_base),
        ("strlibopen", lib_open_string),
        ("tablibopen", lib_open_table),
        ("iolibopen", lib_open_io),
        ("mathlibopen", lib_open_math),
        ("dblibopen", lib_open_debug),
        ("packageopen", lib_open_package),
    ];

    for &(name, func) in openers {
        let closure = Closure::Rust(RustClosure::new(func, name));
        let cr = remote.gc.alloc_closure(closure);
        let key = remote.gc.intern_string(name.as_bytes());
        if let Some(t) = remote.gc.tables.get_mut(remote.global) {
            t.raw_set(Val::Str(key), Val::Function(cr), &remote.gc.string_arena)?;
        }
    }

    Ok(0)
}

// Library opener functions for remote states.

/// Sets `package.loaded[name] = val` in the given state. PUC-Rio's
/// `luaL_register` does this automatically via `luaI_openlib`. We need
/// to do it manually since our `open_libs_selective` doesn't.
fn set_package_loaded(state: &mut LuaState, name: &[u8], val: Val) {
    let loaded_key = state.gc.intern_string(b"_LOADED");
    if let Some(reg) = state.gc.tables.get(state.registry) {
        let loaded_val = reg.get(Val::Str(loaded_key), &state.gc.string_arena);
        if let Val::Table(loaded_ref) = loaded_val {
            let name_key = state.gc.intern_string(name);
            if let Some(loaded_t) = state.gc.tables.get_mut(loaded_ref) {
                let _ = loaded_t.raw_set(Val::Str(name_key), val, &state.gc.string_arena);
            }
        }
    }
}

fn lib_open_base(state: &mut LuaState) -> LuaResult<u32> {
    use crate::stdlib::StdLib;
    crate::stdlib::open_libs_selective(state, StdLib::BASE)?;
    let g = Val::Table(state.global);
    set_package_loaded(state, b"_G", g);
    state.push(g);
    Ok(1)
}

fn lib_open_named(state: &mut LuaState, lib: crate::stdlib::StdLib, name: &[u8]) -> LuaResult<u32> {
    crate::stdlib::open_libs_selective(state, lib)?;
    let key = state.gc.intern_string(name);
    let val = state
        .gc
        .tables
        .get(state.global)
        .map_or(Val::Nil, |t| t.get(Val::Str(key), &state.gc.string_arena));
    set_package_loaded(state, name, val);
    state.push(val);
    Ok(1)
}

fn lib_open_string(state: &mut LuaState) -> LuaResult<u32> {
    lib_open_named(state, crate::stdlib::StdLib::STRING, b"string")
}

fn lib_open_table(state: &mut LuaState) -> LuaResult<u32> {
    lib_open_named(state, crate::stdlib::StdLib::TABLE, b"table")
}

fn lib_open_io(state: &mut LuaState) -> LuaResult<u32> {
    lib_open_named(state, crate::stdlib::StdLib::IO, b"io")
}

fn lib_open_math(state: &mut LuaState) -> LuaResult<u32> {
    lib_open_named(state, crate::stdlib::StdLib::MATH, b"math")
}

fn lib_open_debug(state: &mut LuaState) -> LuaResult<u32> {
    lib_open_named(state, crate::stdlib::StdLib::DEBUG, b"debug")
}

fn lib_open_package(state: &mut LuaState) -> LuaResult<u32> {
    use crate::stdlib::StdLib;
    crate::stdlib::open_libs_selective(state, StdLib::PACKAGE)?;
    let key = state.gc.intern_string(b"package");
    let val = state
        .gc
        .tables
        .get(state.global)
        .map_or(Val::Nil, |t| t.get(Val::Str(key), &state.gc.string_arena));
    state.push(val);
    Ok(1)
}

// =========================================================================
// T.totalmem([limit])
// =========================================================================

/// Gets or sets the memory allocation limit.
///
/// With no args: returns (total, numblocks, maxmem).
/// With one arg: sets the limit and returns 0 values.
///
/// Reference: `mem_query` in `ltests.c`.
pub fn t_totalmem(state: &mut LuaState) -> LuaResult<u32> {
    if state.base < state.top {
        // Set mode: set alloc limit, return nothing.
        let limit = match state.stack_get(state.base) {
            Val::Num(n) => n as usize,
            _ => usize::MAX,
        };
        state.gc.set_alloc_limit(limit);
        Ok(0)
    } else {
        // Query mode: return (total, numblocks, maxmem).
        let total = state.gc.total_alloc();
        let numblocks = state.gc.count_blocks();
        let maxmem = state.gc.gc_state.max_bytes;
        state.push(Val::Num(total as f64));
        state.push(Val::Num(numblocks as f64));
        state.push(Val::Num(maxmem as f64));
        Ok(3)
    }
}

// =========================================================================
// Existing functions (unchanged)
// =========================================================================

// -------------------------------------------------------------------------
// T.querytab(t [, i])
// -------------------------------------------------------------------------

/// Inspects a table's internal structure.
///
/// With one argument: returns (array_size, hash_size, last_free_index).
/// With two arguments where i < array_size: returns (i, array\[i\], nil).
/// With two arguments where i >= array_size: returns (key, value, next)
/// for hash node at index (i - array_size).
///
/// Matches PUC-Rio's `table_query` from `ltests.c`.
pub fn t_querytab(state: &mut LuaState) -> LuaResult<u32> {
    let arg0 = if state.base < state.top {
        state.stack_get(state.base)
    } else {
        return Err(type_error(state, 0, "table"));
    };

    let Val::Table(table_ref) = arg0 else {
        return Err(type_error(state, 0, "table"));
    };

    // Optional second argument: index, default -1.
    let i: i32 = if state.base + 1 < state.top {
        match state.stack_get(state.base + 1) {
            Val::Num(n) => n as i32,
            _ => -1,
        }
    } else {
        -1
    };

    // Extract all table data before any mutable borrows.
    let (asize, hsize, last_free, array_val, node_info) = {
        let table = state
            .gc
            .tables
            .get(table_ref)
            .ok_or_else(|| type_error(state, 0, "table"))?;
        let asize = table.array_len();
        let hsize = table.hash_size();
        let last_free = table.last_free_index();

        let array_val = if i >= 0 && (i as usize) < asize {
            Some(table.array_get(i as usize).unwrap_or(Val::Nil))
        } else {
            None
        };

        let node_info = if i >= 0 && (i as usize) >= asize {
            let hash_idx = (i as usize) - asize;
            if (hash_idx as u32) < hsize {
                table.query_node(hash_idx as u32)
            } else {
                None
            }
        } else {
            None
        };

        (asize, hsize, last_free, array_val, node_info)
    };

    state.ensure_stack(3);

    if i == -1 {
        // Return (array_size, hash_size, last_free).
        state.stack_set(state.base, Val::Num(asize as f64));
        state.stack_set(state.base + 1, Val::Num(f64::from(hsize)));
        state.stack_set(state.base + 2, Val::Num(f64::from(last_free)));
    } else if let Some(val) = array_val {
        // Array part query: return (i, array[i], nil).
        state.stack_set(state.base, Val::Num(f64::from(i)));
        state.stack_set(state.base + 1, val);
        state.stack_set(state.base + 2, Val::Nil);
    } else if let Some((key, value, next)) = node_info {
        // Hash part query: return (key, value, next).
        let display_key = if !value.is_nil() || key.is_nil() || matches!(key, Val::Num(_)) {
            key
        } else {
            let s = state.gc.intern_string(b"<undef>");
            Val::Str(s)
        };

        state.stack_set(state.base, display_key);
        state.stack_set(state.base + 1, value);
        match next {
            Some(idx) => state.stack_set(state.base + 2, Val::Num(f64::from(idx))),
            None => state.stack_set(state.base + 2, Val::Nil),
        }
    } else {
        state.stack_set(state.base, Val::Nil);
        state.stack_set(state.base + 1, Val::Nil);
        state.stack_set(state.base + 2, Val::Nil);
    }
    state.top = state.base + 3;
    Ok(3)
}

// -------------------------------------------------------------------------
// T.hash(key [, table])
// -------------------------------------------------------------------------

/// Returns a string's hash value (1 arg) or the main position index of
/// a key in a table's hash part (2 args).
pub fn t_hash(state: &mut LuaState) -> LuaResult<u32> {
    let arg0 = if state.base < state.top {
        state.stack_get(state.base)
    } else {
        return Err(type_error(state, 0, "value"));
    };

    let has_arg2 = state.base + 1 < state.top;

    let result = if has_arg2 {
        let arg1 = state.stack_get(state.base + 1);
        let Val::Table(table_ref) = arg1 else {
            return Err(type_error(state, 1, "table"));
        };
        let table = state
            .gc
            .tables
            .get(table_ref)
            .ok_or_else(|| type_error(state, 1, "table"))?;
        let mp = if table.hash_size() == 0 {
            0
        } else {
            table.main_position(&arg0, &state.gc.string_arena)
        };
        f64::from(mp)
    } else {
        let Val::Str(str_ref) = arg0 else {
            return Err(arg_error(state, 1, "string expected"));
        };
        let hash = state
            .gc
            .string_arena
            .get(str_ref)
            .map_or(0, super::super::vm::string::LuaString::hash);
        f64::from(hash)
    };

    state.stack_set(state.base, Val::Num(result));
    state.top = state.base + 1;
    Ok(1)
}

// -------------------------------------------------------------------------
// T.int2fb(n)
// -------------------------------------------------------------------------

/// Converts an integer to float-byte encoding and back.
pub fn t_int2fb(state: &mut LuaState) -> LuaResult<u32> {
    let arg0 = if state.base < state.top {
        state.stack_get(state.base)
    } else {
        return Err(type_error(state, 0, "number"));
    };

    let Val::Num(n) = arg0 else {
        return Err(type_error(state, 0, "number"));
    };

    let x = n as u32;
    let encoded = int2fb(x);
    let decoded = fb2int(encoded);

    state.ensure_stack(2);
    state.stack_set(state.base, Val::Num(f64::from(encoded)));
    state.stack_set(state.base + 1, Val::Num(f64::from(decoded)));
    state.top = state.base + 2;
    Ok(2)
}

// -------------------------------------------------------------------------
// T.log2(n)
// -------------------------------------------------------------------------

/// PUC-Rio's luaO_log2: integer log base 2 using a lookup table.
pub fn t_log2(state: &mut LuaState) -> LuaResult<u32> {
    let arg0 = if state.base < state.top {
        state.stack_get(state.base)
    } else {
        return Err(type_error(state, 0, "number"));
    };

    let Val::Num(n) = arg0 else {
        return Err(type_error(state, 0, "number"));
    };

    let result = lua_o_log2(n as u32);
    state.stack_set(state.base, Val::Num(f64::from(result)));
    state.top = state.base + 1;
    Ok(1)
}

fn lua_o_log2(mut x: u32) -> i32 {
    #[rustfmt::skip]
    const LOG_2: [u8; 256] = [
        0,1,2,2,3,3,3,3,4,4,4,4,4,4,4,4,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,
        6,6,6,6,6,6,6,6,6,6,6,6,6,6,6,6,6,6,6,6,6,6,6,6,6,6,6,6,6,6,6,6,
        7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,
        7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,
        8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,
        8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,
        8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,
        8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,8,
    ];
    let mut l: i32 = -1;
    while x >= 256 {
        l += 8;
        x >>= 8;
    }
    l + i32::from(LOG_2[x as usize])
}

// -------------------------------------------------------------------------
// T.listcode(f)
// -------------------------------------------------------------------------

/// Disassembles a Lua function's bytecode into a table.
pub fn t_listcode(state: &mut LuaState) -> LuaResult<u32> {
    let arg0 = if state.base < state.top {
        state.stack_get(state.base)
    } else {
        return Err(arg_error(state, 1, "Lua function expected"));
    };

    let Val::Function(func_ref) = arg0 else {
        return Err(arg_error(state, 1, "Lua function expected"));
    };

    let proto = {
        let closure = state
            .gc
            .closures
            .get(func_ref)
            .ok_or_else(|| arg_error(state, 1, "Lua function expected"))?;
        match closure {
            Closure::Lua(lc) => lc.proto.clone(),
            Closure::Rust(_) => {
                return Err(arg_error(state, 1, "Lua function expected"));
            }
        }
    };

    let result_table = state.gc.alloc_table(Table::new());

    let maxstack_key = state.gc.intern_string(b"maxstack");
    if let Some(t) = state.gc.tables.get_mut(result_table) {
        t.raw_set(
            Val::Str(maxstack_key),
            Val::Num(f64::from(proto.max_stack_size)),
            &state.gc.string_arena,
        )?;
    }

    let numparams_key = state.gc.intern_string(b"numparams");
    if let Some(t) = state.gc.tables.get_mut(result_table) {
        t.raw_set(
            Val::Str(numparams_key),
            Val::Num(f64::from(proto.num_params)),
            &state.gc.string_arena,
        )?;
    }

    for pc in 0..proto.code.len() {
        let instr = Instruction::from_raw(proto.code[pc]);
        let line = proto.line_info.get(pc).copied().unwrap_or(0);
        let op_str = buildop(instr, line, pc);

        let key = Val::Num((pc + 1) as f64);
        let val_str = state.gc.intern_string(op_str.as_bytes());
        let val = Val::Str(val_str);

        if let Some(t) = state.gc.tables.get_mut(result_table) {
            t.raw_set(key, val, &state.gc.string_arena)?;
        }
    }

    state.stack_set(state.base, Val::Table(result_table));
    state.top = state.base + 1;
    Ok(1)
}

fn buildop(instr: Instruction, line: u32, pc: usize) -> String {
    let op = instr.opcode();
    let name = op.name();
    let a = instr.a();

    match op.mode() {
        OpMode::IABC => {
            let b = instr.b();
            let c = instr.c();
            format!("({line:4}) {pc:4} - {name:<12}{a:4} {b:4} {c:4}")
        }
        OpMode::IABx => {
            let bx = instr.bx();
            format!("({line:4}) {pc:4} - {name:<12}{a:4} {bx:4}")
        }
        OpMode::IAsBx => {
            let sbx = instr.sbx();
            format!("({line:4}) {pc:4} - {name:<12}{a:4} {sbx:4}")
        }
    }
}

// -------------------------------------------------------------------------
// T.setyhook([mask [, count]])
// -------------------------------------------------------------------------

/// Sets a yield-on-hook on the current thread.
pub fn t_setyhook(state: &mut LuaState) -> LuaResult<u32> {
    let arg0 = if state.base < state.top {
        state.stack_get(state.base)
    } else {
        state.hook.hook_mask = 0;
        state.hook.yield_on_hook = false;
        return Ok(0);
    };

    if arg0.is_nil() {
        state.hook.hook_mask = 0;
        state.hook.yield_on_hook = false;
        return Ok(0);
    }

    let mask_str = match arg0 {
        Val::Str(r) => state
            .gc
            .string_arena
            .get(r)
            .map(|s| String::from_utf8_lossy(s.data()).to_string())
            .unwrap_or_default(),
        _ => String::new(),
    };

    let count: i32 = if state.base + 1 < state.top {
        match state.stack_get(state.base + 1) {
            Val::Num(n) => n as i32,
            _ => 0,
        }
    } else {
        0
    };

    let mut mask: u8 = 0;
    for ch in mask_str.chars() {
        if ch == 'l' {
            mask |= MASK_LINE;
        }
    }

    if count > 0 {
        mask |= MASK_COUNT;
    }

    state.hook.hook_mask = mask;
    state.hook.base_hook_count = count;
    state.hook.hook_count = count;
    state.hook.yield_on_hook = mask != 0;

    Ok(0)
}

// -------------------------------------------------------------------------
// T.resume(thread)
// -------------------------------------------------------------------------

/// Resumes a coroutine thread with no arguments.
pub fn t_resume(state: &mut LuaState) -> LuaResult<u32> {
    let arg0 = if state.base < state.top {
        state.stack_get(state.base)
    } else {
        return Err(type_error(state, 0, "thread"));
    };

    let Val::Thread(co_ref) = arg0 else {
        return Err(type_error(state, 0, "thread"));
    };

    match coroutine::auxresume(state, co_ref, &[]) {
        Ok(_results) => {
            let base = state.base;
            state.stack_set(base, Val::Bool(true));
            state.top = base + 1;
            Ok(1)
        }
        Err(error_val) => {
            let base = state.base;
            state.stack_set(base, Val::Bool(false));
            state.stack_set(base + 1, error_val);
            state.top = base + 2;
            Ok(2)
        }
    }
}

// -------------------------------------------------------------------------
// T.d2s(number) -> string (8 bytes, native endian)
// -------------------------------------------------------------------------

/// Converts an f64 to its 8-byte native-endian binary representation.
pub fn t_d2s(state: &mut LuaState) -> LuaResult<u32> {
    let arg0 = if state.base < state.top {
        state.stack_get(state.base)
    } else {
        return Err(type_error(state, 0, "number"));
    };

    let Val::Num(n) = arg0 else {
        return Err(type_error(state, 0, "number"));
    };

    let bytes = n.to_ne_bytes();
    let s = state.gc.intern_string(&bytes);
    state.stack_set(state.base, Val::Str(s));
    state.top = state.base + 1;
    Ok(1)
}

// -------------------------------------------------------------------------
// T.s2d(string) -> number
// -------------------------------------------------------------------------

/// Converts an 8-byte native-endian binary string back to an f64.
pub fn t_s2d(state: &mut LuaState) -> LuaResult<u32> {
    let arg0 = if state.base < state.top {
        state.stack_get(state.base)
    } else {
        return Err(type_error(state, 0, "string"));
    };

    let Val::Str(str_ref) = arg0 else {
        return Err(type_error(state, 0, "string"));
    };

    let empty: &[u8] = &[];
    let data = state
        .gc
        .string_arena
        .get(str_ref)
        .map_or(empty, super::super::vm::string::LuaString::data);

    if data.len() < 8 {
        return Err(arg_error(state, 1, "string must be at least 8 bytes"));
    }

    let mut buf = [0u8; 8];
    buf.copy_from_slice(&data[..8]);
    let n = f64::from_ne_bytes(buf);

    state.stack_set(state.base, Val::Num(n));
    state.top = state.base + 1;
    Ok(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vm::instructions::OpCode;

    #[test]
    fn log2_values() {
        assert_eq!(lua_o_log2(0), -1);
        assert_eq!(lua_o_log2(1), 0);
        assert_eq!(lua_o_log2(2), 1);
        assert_eq!(lua_o_log2(3), 1);
        assert_eq!(lua_o_log2(4), 2);
        assert_eq!(lua_o_log2(7), 2);
        assert_eq!(lua_o_log2(8), 3);
        assert_eq!(lua_o_log2(255), 7);
        assert_eq!(lua_o_log2(256), 8);
        assert_eq!(lua_o_log2(1024), 10);
    }

    #[test]
    fn buildop_iabc() {
        let instr = Instruction::abc(OpCode::Move, 1, 2, 0);
        let s = buildop(instr, 5, 0);
        assert!(s.contains("MOVE"));
        assert!(s.starts_with("(   5)    0 - "));
    }

    #[test]
    fn buildop_iabx() {
        let instr = Instruction::a_bx(OpCode::LoadK, 0, 1);
        let s = buildop(instr, 1, 0);
        assert!(s.contains("LOADK"));
    }

    #[test]
    fn buildop_iasbx() {
        let instr = Instruction::a_sbx(OpCode::Jmp, 0, 10);
        let s = buildop(instr, 1, 0);
        assert!(s.contains("JMP"));
        assert!(s.contains("  10"));
    }
}
