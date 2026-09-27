//! Base library: print, assert, type, tostring, tonumber, etc.
//!
//! Reference: `lbaselib.c` in PUC-Rio Lua 5.1.1.



use crate::error::{LuaError, LuaResult, RuntimeError};
use crate::vm::callinfo::LUA_MULTRET;
use crate::vm::execute;
use crate::vm::metatable::{self, val_raw_equal};
use crate::vm::state::LuaState;
use crate::vm::table::Table;
use crate::vm::value::{Userdata, Val};

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

/// Number of arguments passed to the current function.
#[inline]
fn nargs(state: &LuaState) -> usize {
    state.top.saturating_sub(state.base)
}

/// Gets argument at position `n` (0-indexed from first arg).
///
/// Returns `Val::Nil` if `n` is past the actual argument count.
#[inline]
fn arg(state: &LuaState, n: usize) -> Val {
    let idx = state.base + n;
    if idx < state.top {
        state.stack_get(idx)
    } else {
        Val::Nil
    }
}

/// Returns "bad argument" error.
fn bad_argument(name: &str, n: usize, msg: &str) -> LuaError {
    LuaError::Runtime(RuntimeError {
        message: format!("bad argument #{n} to '{name}' ({msg})"),
        level: 0,
        traceback: vec![],
    })
}

/// Returns a simple runtime error (no source location).
fn simple_error(msg: String) -> LuaError {
    LuaError::Runtime(RuntimeError {
        message: msg,
        level: 0,
        traceback: vec![],
    })
}

/// Check minimum argument count.
fn check_args(name: &str, state: &LuaState, min: usize) -> LuaResult<()> {
    if nargs(state) < min {
        Err(bad_argument(name, min, "value expected"))
    } else {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Metafield helpers (for __tostring, __metatable -- not in TMS enum)
// ---------------------------------------------------------------------------

/// Looks up a named metafield in a value's metatable.
///
/// Unlike the TMS-based lookup, this works for any string key
/// (e.g., `__tostring`, `__metatable`).
fn get_metafield(gc: &mut crate::vm::state::Gc, val: Val, name: &[u8]) -> Option<Val> {
    let mt = match val {
        Val::Table(r) => gc.tables.get(r).and_then(Table::metatable)?,
        Val::Userdata(r) => gc.userdata.get(r).and_then(Userdata::metatable)?,
        _ => gc.type_metatables[metatable::type_tag(val)]?,
    };

    let key_ref = gc.intern_string(name);
    let table = gc.tables.get(mt)?;
    let result = table.get_str(key_ref, &gc.string_arena);
    if result.is_nil() { None } else { Some(result) }
}

// ---------------------------------------------------------------------------
// print
// ---------------------------------------------------------------------------

/// Implements Lua's `print(...)`.
///
/// Tab-separated values, newline-terminated. Uses `tostring()` conversion
/// for each argument.
///
/// Reference: `luaB_print` in `lbaselib.c`.
pub fn lua_print(state: &mut LuaState) -> LuaResult<u32> {
    let base = state.base;
    let top = state.top;
    let n = top.saturating_sub(base);

    // Written through C `stdout` (like PUC-Rio's `fputs`) so output stays
    // ordered with `io.write` and with a C host's own stdio output.
    let mut out: Vec<u8> = Vec::new();

    for i in 0..n {
        if i > 0 {
            out.extend_from_slice(b"\t");
        }
        let val = state.stack_get(base + i);
        match val {
            Val::Nil => out.extend_from_slice(b"nil"),
            Val::Bool(b) => {
                if b {
                    out.extend_from_slice(b"true");
                } else {
                    out.extend_from_slice(b"false");
                }
            }
            Val::Str(r) => {
                if let Some(s) = state.gc.string_arena.get(r) {
                    out.extend_from_slice(s.data());
                } else {
                    out.extend_from_slice(b"string: ???");
                }
            }
            _ => {
                let s = format!("{val}");
                out.extend_from_slice(s.as_bytes());
            }
        }
    }
    out.push(b'\n');
    crate::platform::write_c_stdout(&out);

    Ok(0)
}

// ---------------------------------------------------------------------------
// type
// ---------------------------------------------------------------------------

/// Implements Lua's `type(v)`.
///
/// Returns the type name as a string.
/// Reference: `luaB_type` in `lbaselib.c`.
pub fn lua_type(state: &mut LuaState) -> LuaResult<u32> {
    check_args("type", state, 1)?;
    let val = arg(state, 0);
    let name = val.type_name();
    let r = state.gc.intern_string(name.as_bytes());
    state.push(Val::Str(r));
    Ok(1)
}

// ---------------------------------------------------------------------------
// tostring
// ---------------------------------------------------------------------------

/// Implements Lua's `tostring(v)`.
///
/// Respects `__tostring` metamethod. Otherwise converts to string:
/// - nil -> "nil", booleans -> "true"/"false"
/// - numbers -> %.14g formatted
/// - strings -> identity
/// - other -> "type: 0xADDR"
///
/// Reference: `luaB_tostring` in `lbaselib.c`.
pub fn lua_tostring(state: &mut LuaState) -> LuaResult<u32> {
    check_args("tostring", state, 1)?;
    let val = arg(state, 0);

    // Check __tostring metamethod.
    if let Some(tm_val) = get_metafield(&mut state.gc, val, b"__tostring") {
        // Call __tostring(val).
        let call_base = state.top;
        state.ensure_stack(call_base + 3);
        state.stack_set(call_base, tm_val);
        state.stack_set(call_base + 1, val);
        state.top = call_base + 2;

        state.call_function(call_base, 1)?;

        // Result is at call_base (poscall put it there).
        let result = state.stack_get(call_base);
        state.push(result);
        return Ok(1);
    }

    let result = val_to_str(state, val);
    state.push(result);
    Ok(1)
}

/// Converts a value to a Lua string without metamethods.
fn val_to_str(state: &mut LuaState, val: Val) -> Val {
    match val {
        Val::Str(_) => val,
        Val::Nil => {
            let r = state.gc.intern_string(b"nil");
            Val::Str(r)
        }
        Val::Bool(b) => {
            let s = if b {
                b"true".as_ref()
            } else {
                b"false".as_ref()
            };
            let r = state.gc.intern_string(s);
            Val::Str(r)
        }
        Val::Num(_) => {
            let s = format!("{val}");
            let r = state.gc.intern_string(s.as_bytes());
            Val::Str(r)
        }
        _ => {
            // "type: 0xADDR" format
            let s = format!("{val}");
            let r = state.gc.intern_string(s.as_bytes());
            Val::Str(r)
        }
    }
}

// ---------------------------------------------------------------------------
// tonumber
// ---------------------------------------------------------------------------

/// Implements Lua's `tonumber(v, base?)`.
///
/// Base 10 (default): converts numbers and numeric strings.
/// Other bases (2-36): converts string to integer in that base.
/// Returns nil if conversion fails.
///
/// Reference: `luaB_tonumber` in `lbaselib.c`.
pub fn lua_tonumber(state: &mut LuaState) -> LuaResult<u32> {
    check_args("tonumber", state, 1)?;
    let val = arg(state, 0);
    let base_arg = arg(state, 1);

    let base = match base_arg {
        Val::Nil => 10,
        Val::Num(n) => n as i64,
        _ => {
            return Err(bad_argument(
                "tonumber",
                2,
                "number expected, got non-number",
            ));
        }
    };

    if base == 10 {
        // Standard conversion.
        if let Some(n) = execute::coerce_to_number(val, &state.gc) {
            state.push(Val::Num(n));
        } else {
            state.push(Val::Nil);
        }
    } else {
        // Non-decimal base.
        if !(2..=36).contains(&base) {
            return Err(bad_argument("tonumber", 2, "base out of range"));
        }
        let Val::Str(r) = val else {
            return Err(bad_argument("tonumber", 1, "string expected"));
        };
        let s = state
            .gc
            .string_arena
            .get(r)
            .map(|ls| String::from_utf8_lossy(ls.data()).to_string())
            .unwrap_or_default();
        let trimmed = s.trim();
        #[allow(clippy::cast_precision_loss)]
        if let Ok(n) = u64::from_str_radix(trimmed, base as u32) {
            state.push(Val::Num(n as f64));
        } else {
            state.push(Val::Nil);
        }
    }
    Ok(1)
}

// ---------------------------------------------------------------------------
// assert
// ---------------------------------------------------------------------------

/// Implements Lua's `assert(v, msg?)`.
///
/// If the first argument is falsy, raises an error with the optional
/// message (default: "assertion failed!"). If truthy, returns ALL arguments.
///
/// Reference: `luaB_assert` in `lbaselib.c`.
pub fn lua_assert(state: &mut LuaState) -> LuaResult<u32> {
    check_args("assert", state, 1)?;
    let val = arg(state, 0);

    if !val.is_truthy() {
        let msg = arg(state, 1);
        let error_msg = if let Val::Str(r) = msg {
            state.gc.string_arena.get(r).map_or_else(
                || "assertion failed!".to_string(),
                |s| String::from_utf8_lossy(s.data()).to_string(),
            )
        } else if msg.is_nil() {
            "assertion failed!".to_string()
        } else {
            format!("{msg}")
        };

        return Err(simple_error(error_msg));
    }

    // Return all arguments.
    let n = nargs(state);
    Ok(n as u32)
}

// ---------------------------------------------------------------------------
// error
// ---------------------------------------------------------------------------

/// Implements Lua's `error(msg, level?)`.
///
/// Raises a runtime error. If `msg` is a string and `level > 0`,
/// prepends source location. The error object (which can be any Lua value)
/// is stored in `state.error_object` for `pcall`/`xpcall` to retrieve.
///
/// Reference: `luaB_error` in `lbaselib.c`.
pub fn lua_error(state: &mut LuaState) -> LuaResult<u32> {
    let msg = arg(state, 0);
    let level_arg = arg(state, 1);

    let level: u32 = match level_arg {
        Val::Num(n) => {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let l = n as u32;
            l
        }
        _ => 1, // default level
    };

    // If msg is a string or number (lua_isstring) and level > 0, prepend source location.
    // PUC-Rio's lua_isstring returns true for both strings and numbers.
    let is_stringable = matches!(msg, Val::Str(_) | Val::Num(_));
    let error_val = if is_stringable && level > 0 {
        let where_prefix = execute::get_where(state, level);
        let original = match msg {
            Val::Str(r) => state
                .gc
                .string_arena
                .get(r)
                .map(|s| String::from_utf8_lossy(s.data()).to_string())
                .unwrap_or_default(),
            _ => format!("{msg}"),
        };
        let full_msg = format!("{where_prefix}{original}");
        let new_r = state.gc.intern_string(full_msg.as_bytes());
        Val::Str(new_r)
    } else {
        msg // Non-string/number: throw as-is.
    };

    // Store the error object for pcall to retrieve.
    state.error_object = Some(error_val);

    // Create the error message string for the RuntimeError.
    let display_msg = match error_val {
        Val::Str(r) => state
            .gc
            .string_arena
            .get(r)
            .map(|s| String::from_utf8_lossy(s.data()).to_string())
            .unwrap_or_default(),
        Val::Nil => "nil".to_string(),
        _ => format!("{error_val}"),
    };

    Err(LuaError::Runtime(RuntimeError {
        message: display_msg,
        level,
        traceback: vec![],
    }))
}

// ---------------------------------------------------------------------------
// pcall
// ---------------------------------------------------------------------------

/// Implements Lua's `pcall(f, ...)`.
///
/// Calls `f` with the given arguments in protected mode. On success,
/// returns `true` followed by all return values. On error, returns
/// `false` followed by the error message.
///
/// Reference: `luaB_pcall` in `lbaselib.c`.
pub fn lua_pcall(state: &mut LuaState) -> LuaResult<u32> {
    check_args("pcall", state, 1)?;

    let func_pos = state.base; // The function to call is pcall's first argument.
    let n_call_args = nargs(state) - 1; // Arguments to pass to the function.

    // Save state for error recovery.
    let saved_ci = state.ci;
    let saved_n_ccalls = state.n_ccalls;
    let saved_call_depth = state.call_depth;

    // Clear any stale error object.
    state.error_object = None;

    // Set top to include function and its arguments.
    state.top = func_pos + 1 + n_call_args;

    // Attempt the call (through call_function for C-call boundary tracking).
    let call_result = state.call_function(func_pos, LUA_MULTRET);

    match call_result {
        Ok(()) => {
            // Success: results are at func_pos..state.top.
            let n_inner = state.top - func_pos;

            // Shift results right by 1 to insert "true" prefix.
            state.ensure_stack(state.top + 1);
            for i in (func_pos..state.top).rev() {
                let v = state.stack_get(i);
                state.stack_set(i + 1, v);
            }
            state.stack_set(func_pos, Val::Bool(true));
            state.top = func_pos + 1 + n_inner;

            Ok((1 + n_inner) as u32)
        }
        Err(err) => {
            // Error: restore state and push false + error value.
            state.ci = saved_ci;
            state.base = state.call_stack[state.ci].base;
            state.n_ccalls = saved_n_ccalls;
            state.call_depth = saved_call_depth;
            // Clear overflow flag since ci is back below MAXCALLS.
            if state.ci < crate::vm::state::MAXCALLS {
                state.ci_overflow = false;
            }

            // Close upvalues opened during the failed call.
            state.close_upvalues(func_pos);

            // Get the error value. Prefer the stored error object (from error()),
            // falling back to the error message string.
            let error_val = state.error_object.take().unwrap_or_else(|| {
                let r = state.gc.intern_string(err.to_string().as_bytes());
                Val::Str(r)
            });

            state.stack_set(func_pos, Val::Bool(false));
            state.stack_set(func_pos + 1, error_val);
            state.top = func_pos + 2;

            Ok(2)
        }
    }
}

// ---------------------------------------------------------------------------
// xpcall
// ---------------------------------------------------------------------------

/// Implements Lua's `xpcall(f, err)`.
///
/// Calls `f` with no arguments in protected mode. On error, calls the
/// error handler `err` with the error value. Returns `true` + results
/// on success, or `false` + handler return value on error.
///
/// Reference: `luaB_xpcall` in `lbaselib.c`.
pub fn lua_xpcall(state: &mut LuaState) -> LuaResult<u32> {
    check_args("xpcall", state, 2)?;

    let func_val = arg(state, 0);
    let handler_val = arg(state, 1);

    // Set up: keep handler on stack at func_pos (GC-visible), function at
    // func_pos+1. This prevents the handler closure from being collected
    // during a GC cycle triggered by the protected call.
    let func_pos = state.base;
    let handler_slot = func_pos;
    let call_pos = func_pos + 1;

    // Save state for error recovery.
    let saved_ci = state.ci;
    let saved_n_ccalls = state.n_ccalls;
    let saved_call_depth = state.call_depth;
    state.error_object = None;

    // Place handler at func_pos (GC anchor), function at func_pos+1.
    state.ensure_stack(call_pos + 1);
    state.stack_set(handler_slot, handler_val);
    state.stack_set(call_pos, func_val);
    state.top = call_pos + 1;

    // Attempt the call (through call_function for C-call boundary tracking).
    let call_result = state.call_function(call_pos, LUA_MULTRET);

    match call_result {
        Ok(()) => {
            // Success: results at call_pos..state.top (call_pos = func_pos + 1).
            // The handler at func_pos is no longer needed; overwrite with true.
            let n_results = state.top - call_pos;
            state.stack_set(func_pos, Val::Bool(true));
            // Results are already at func_pos+1..state.top, so just adjust count.
            state.top = func_pos + 1 + n_results;

            Ok((1 + n_results) as u32)
        }
        Err(err) => {
            // Get the error value before modifying state.
            let error_val = state.error_object.take().unwrap_or_else(|| {
                let r = state.gc.intern_string(err.to_string().as_bytes());
                Val::Str(r)
            });

            state.close_upvalues(func_pos);

            // Retrieve the handler from its GC-visible stack slot.
            let handler_from_stack = state.stack_get(handler_slot);

            // Call the error handler BEFORE restoring ci, so it can see the
            // full call stack (e.g. debug.traceback). Place the handler call
            // above the current stack top to avoid clobbering error frames.
            // PUC-Rio calls the handler via luaG_errormsg before longjmp.
            let handler_pos = state.top;
            state.ensure_stack(handler_pos + 2);
            state.stack_set(handler_pos, handler_from_stack);
            state.stack_set(handler_pos + 1, error_val);
            state.top = handler_pos + 2;

            let handler_result = state.call_function(handler_pos, 1);

            let handler_ret = if handler_result.is_ok() {
                state.stack_get(handler_pos)
            } else {
                let msg = state.gc.intern_string(b"error in error handling");
                Val::Str(msg)
            };

            // NOW restore state (after handler has seen the full stack).
            state.ci = saved_ci;
            state.base = state.call_stack[state.ci].base;
            state.n_ccalls = saved_n_ccalls;
            state.call_depth = saved_call_depth;
            if state.ci < crate::vm::state::MAXCALLS {
                state.ci_overflow = false;
            }

            state.stack_set(func_pos, Val::Bool(false));
            state.stack_set(func_pos + 1, handler_ret);
            state.top = func_pos + 2;
            Ok(2)
        }
    }
}

// ---------------------------------------------------------------------------
// setmetatable / getmetatable
// ---------------------------------------------------------------------------

/// Implements Lua's `setmetatable(table, metatable)`.
///
/// Sets the metatable for a table. The second argument must be nil or a table.
/// If the current metatable has a `__metatable` field, raises an error.
///
/// Reference: `luaB_setmetatable` in `lbaselib.c`.
pub fn lua_setmetatable(state: &mut LuaState) -> LuaResult<u32> {
    check_args("setmetatable", state, 2)?;
    let table_val = arg(state, 0);
    let mt_val = arg(state, 1);

    let Val::Table(table_ref) = table_val else {
        return Err(bad_argument("setmetatable", 1, "table expected"));
    };

    // Validate second argument is nil or table.
    let new_mt = match mt_val {
        Val::Nil => None,
        Val::Table(r) => Some(r),
        _ => {
            return Err(bad_argument("setmetatable", 2, "nil or table expected"));
        }
    };

    // Check for __metatable protection.
    if let Some(existing_mt) = state.gc.tables.get(table_ref).and_then(Table::metatable) {
        let key_ref = state.gc.intern_string(b"__metatable");
        let has_protection = state
            .gc
            .tables
            .get(existing_mt)
            .is_some_and(|t| !t.get_str(key_ref, &state.gc.string_arena).is_nil());
        if has_protection {
            return Err(simple_error(
                "cannot change a protected metatable".to_string(),
            ));
        }
    }

    // Set the metatable.
    if let Some(t) = state.gc.tables.get_mut(table_ref) {
        t.set_metatable(new_mt);
    }
    // Write barrier: table was mutated (metatable field changed).
    state.gc.barrier_back(table_ref);

    // Return the table.
    state.push(table_val);
    Ok(1)
}

/// Implements Lua's `getmetatable(obj)`.
///
/// Returns the metatable of the object. If the metatable has a `__metatable`
/// field, returns that field instead of the actual metatable.
///
/// Reference: `luaB_getmetatable` in `lbaselib.c`.
pub fn lua_getmetatable(state: &mut LuaState) -> LuaResult<u32> {
    check_args("getmetatable", state, 1)?;
    let val = arg(state, 0);

    // Get the actual metatable.
    let mt = match val {
        Val::Table(r) => state.gc.tables.get(r).and_then(Table::metatable),
        Val::Userdata(r) => state.gc.userdata.get(r).and_then(Userdata::metatable),
        _ => state.gc.type_metatables[metatable::type_tag(val)],
    };

    let Some(mt_ref) = mt else {
        state.push(Val::Nil);
        return Ok(1);
    };

    // Check for __metatable field.
    if let Some(protection) = get_metafield(&mut state.gc, val, b"__metatable") {
        state.push(protection);
    } else {
        state.push(Val::Table(mt_ref));
    }

    Ok(1)
}

// ---------------------------------------------------------------------------
// rawget / rawset / rawequal
// ---------------------------------------------------------------------------

/// Implements Lua's `rawget(table, index)`.
///
/// Gets a value from a table without invoking metamethods.
/// Reference: `luaB_rawget` in `lbaselib.c`.
pub fn lua_rawget(state: &mut LuaState) -> LuaResult<u32> {
    check_args("rawget", state, 2)?;
    let table_val = arg(state, 0);
    let key = arg(state, 1);

    let Val::Table(table_ref) = table_val else {
        return Err(bad_argument("rawget", 1, "table expected"));
    };

    let result = state
        .gc
        .tables
        .get(table_ref)
        .map_or(Val::Nil, |t| t.get(key, &state.gc.string_arena));
    state.push(result);
    Ok(1)
}

/// Implements Lua's `rawset(table, index, value)`.
///
/// Sets a value in a table without invoking metamethods.
/// Reference: `luaB_rawset` in `lbaselib.c`.
pub fn lua_rawset(state: &mut LuaState) -> LuaResult<u32> {
    check_args("rawset", state, 3)?;
    let table_val = arg(state, 0);
    let key = arg(state, 1);
    let value = arg(state, 2);

    let Val::Table(table_ref) = table_val else {
        return Err(bad_argument("rawset", 1, "table expected"));
    };

    // Need to split the borrow: get table mutably, string_arena immutably.
    // Since these are different arenas in gc, we access them via the public fields.
    let table = state
        .gc
        .tables
        .get_mut(table_ref)
        .ok_or_else(|| bad_argument("rawset", 1, "invalid table"))?;
    table.raw_set(key, value, &state.gc.string_arena)?;
    // Write barrier: table was mutated.
    state.gc.barrier_back(table_ref);

    // Return the table.
    state.push(table_val);
    Ok(1)
}

/// Implements Lua's `rawequal(v1, v2)`.
///
/// Compares two values without invoking metamethods.
/// Reference: `luaB_rawequal` in `lbaselib.c`.
pub fn lua_rawequal(state: &mut LuaState) -> LuaResult<u32> {
    check_args("rawequal", state, 2)?;
    let v1 = arg(state, 0);
    let v2 = arg(state, 1);

    let result = val_raw_equal(v1, v2, &state.gc.tables, &state.gc.string_arena);
    state.push(Val::Bool(result));
    Ok(1)
}

// ---------------------------------------------------------------------------
// select
// ---------------------------------------------------------------------------

/// Implements Lua's `select(index, ...)`.
///
/// If `index` is the string "#", returns the number of extra arguments.
/// Otherwise, returns all arguments from `index` onward.
///
/// Reference: `luaB_select` in `lbaselib.c`.
pub fn lua_select(state: &mut LuaState) -> LuaResult<u32> {
    check_args("select", state, 1)?;
    let n = nargs(state);
    let idx_val = arg(state, 0);

    // Check for "#" selector.
    if let Val::Str(r) = idx_val
        && let Some(s) = state.gc.string_arena.get(r)
        && s.data() == b"#"
    {
        #[allow(clippy::cast_precision_loss)]
        let count = (n - 1) as f64;
        state.push(Val::Num(count));
        return Ok(1);
    }

    // Numeric index.
    // Matches PUC-Rio's luaB_select: uses gettop (n) for clamping,
    // not n-1, so select(1) with 0 extra args returns 0 results.
    let Val::Num(idx_f) = idx_val else {
        return Err(bad_argument("select", 1, "number or string expected"));
    };
    #[allow(clippy::cast_possible_truncation)]
    let mut idx = idx_f as i64;
    let n_i64 = n as i64;

    if idx < 0 {
        idx += n_i64; // PUC-Rio: i = n + i (n includes the index arg)
    } else if idx > n_i64 {
        idx = n_i64; // PUC-Rio: clamp to n
    }
    if idx < 1 {
        return Err(bad_argument("select", 1, "index out of range"));
    }

    // Return all arguments from idx onward.
    // PUC-Rio: return n - i.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let result_count = (n_i64 - idx) as u32;
    Ok(result_count)
}

// ---------------------------------------------------------------------------
// unpack
// ---------------------------------------------------------------------------

/// Implements Lua's `unpack(list, i, j)`.
///
/// Returns `list[i], list[i+1], ..., list[j]`.
/// Default: `i=1`, `j=#list`.
///
/// Reference: `luaB_unpack` in `lbaselib.c`.
pub fn lua_unpack(state: &mut LuaState) -> LuaResult<u32> {
    check_args("unpack", state, 1)?;
    let list_val = arg(state, 0);
    let i_val = arg(state, 1);
    let j_val = arg(state, 2);

    let Val::Table(table_ref) = list_val else {
        return Err(bad_argument("unpack", 1, "table expected"));
    };

    #[allow(clippy::cast_possible_truncation)]
    let i = match i_val {
        Val::Num(n) => n as i64,
        Val::Nil => 1,
        _ => {
            return Err(bad_argument("unpack", 2, "number expected"));
        }
    };

    #[allow(clippy::cast_possible_truncation)]
    let j = match j_val {
        Val::Nil => {
            // Default: #list (table length).
            let table = state
                .gc
                .tables
                .get(table_ref)
                .ok_or_else(|| bad_argument("unpack", 1, "invalid table"))?;
            table.len(&state.gc.string_arena) as i64
        }
        Val::Num(n) => n as i64,
        _ => {
            return Err(bad_argument("unpack", 3, "number expected"));
        }
    };

    let n = j - i + 1;
    if n <= 0 {
        return Ok(0);
    }

    #[allow(clippy::cast_sign_loss)]
    let n_usize = n as usize;
    state.ensure_stack(state.top + n_usize);

    for k in i..=j {
        #[allow(clippy::cast_precision_loss)]
        let key = Val::Num(k as f64);
        let val = state
            .gc
            .tables
            .get(table_ref)
            .map_or(Val::Nil, |t| t.get(key, &state.gc.string_arena));
        state.push(val);
    }

    Ok(n_usize as u32)
}

// ---------------------------------------------------------------------------
// next / pairs / ipairs
// ---------------------------------------------------------------------------

/// Implements Lua's `next(table, key)`.
///
/// Returns the next key-value pair after `key` in the table.
/// If `key` is nil, returns the first pair. Returns nil at end.
///
/// Reference: `luaB_next` in `lbaselib.c`.
pub fn lua_next(state: &mut LuaState) -> LuaResult<u32> {
    check_args("next", state, 1)?;
    let table_val = arg(state, 0);
    let key = arg(state, 1); // defaults to nil if omitted

    let Val::Table(table_ref) = table_val else {
        return Err(bad_argument("next", 1, "table expected"));
    };

    let result = state
        .gc
        .tables
        .get(table_ref)
        .ok_or_else(|| bad_argument("next", 1, "invalid table"))?
        .next(key, &state.gc.string_arena)?;

    if let Some((k, v)) = result {
        state.push(k);
        state.push(v);
        Ok(2)
    } else {
        state.push(Val::Nil);
        Ok(1)
    }
}

/// Implements Lua's `pairs(t)`.
///
/// Returns three values: the `next` function, the table, and nil.
/// The generic `for` loop uses these to iterate over all key-value pairs.
///
/// Reference: `luaB_pairs` in `lbaselib.c`.
pub fn lua_pairs(state: &mut LuaState) -> LuaResult<u32> {
    check_args("pairs", state, 1)?;
    let table_val = arg(state, 0);

    let Val::Table(_) = table_val else {
        return Err(bad_argument("pairs", 1, "table expected"));
    };

    // Push the `next` function. We look it up from globals.
    let next_key = state.gc.intern_string(b"next");
    let next_fn = state
        .gc
        .tables
        .get(state.global)
        .map_or(Val::Nil, |t| t.get_str(next_key, &state.gc.string_arena));
    state.push(next_fn);
    state.push(table_val);
    state.push(Val::Nil);
    Ok(3)
}

/// Internal iterator for `ipairs`. Called as `ipairsaux(t, i)`.
///
/// Increments `i`, does `rawget(t, i+1)`, returns `(i+1, value)` or
/// nothing if value is nil.
fn ipairs_aux(state: &mut LuaState) -> LuaResult<u32> {
    let table_val = arg(state, 0);
    let idx_val = arg(state, 1);

    let Val::Table(table_ref) = table_val else {
        return Err(bad_argument("ipairs", 1, "table expected"));
    };

    let Val::Num(idx_f) = idx_val else {
        return Err(bad_argument("ipairs", 2, "number expected"));
    };

    let next_idx = idx_f + 1.0;
    #[allow(clippy::cast_precision_loss)]
    let key = Val::Num(next_idx);
    let val = state
        .gc
        .tables
        .get(table_ref)
        .map_or(Val::Nil, |t| t.get(key, &state.gc.string_arena));

    if val.is_nil() {
        Ok(0)
    } else {
        state.push(Val::Num(next_idx));
        state.push(val);
        Ok(2)
    }
}

/// Implements Lua's `ipairs(t)`.
///
/// Returns three values: an iterator function, the table, and 0.
/// The iterator returns sequential integer keys 1, 2, 3, ... with
/// their values, stopping at the first nil.
///
/// Reference: `luaB_ipairs` in `lbaselib.c`.
pub fn lua_ipairs(state: &mut LuaState) -> LuaResult<u32> {
    check_args("ipairs", state, 1)?;
    let table_val = arg(state, 0);

    let Val::Table(_) = table_val else {
        return Err(bad_argument("ipairs", 1, "table expected"));
    };

    // Create the ipairs_aux closure and push it.
    let closure = crate::vm::closure::Closure::Rust(crate::vm::closure::RustClosure::new(
        ipairs_aux,
        "ipairs_aux",
    ));
    let closure_ref = state.gc.alloc_closure(closure);
    state.push(Val::Function(closure_ref));
    state.push(table_val);
    state.push(Val::Num(0.0));
    Ok(3)
}

// ---------------------------------------------------------------------------
// loadstring / loadfile / dofile / load
// ---------------------------------------------------------------------------

/// Implements Lua's `loadstring(string [, chunkname])`.
///
/// Compiles a string as a Lua chunk. Returns the compiled function on success,
/// or nil + error message on failure.
///
/// Reference: `luaB_loadstring` in `lbaselib.c`.
pub fn lua_loadstring(state: &mut LuaState) -> LuaResult<u32> {
    check_args("loadstring", state, 1)?;
    let src_val = arg(state, 0);
    let name_val = arg(state, 1);

    let Val::Str(src_ref) = src_val else {
        return Err(bad_argument("loadstring", 1, "string expected"));
    };

    let source = state
        .gc
        .string_arena
        .get(src_ref)
        .map(|s| s.data().to_vec())
        .unwrap_or_default();

    // Chunk name: defaults to the source string itself.
    let chunk_name = if let Val::Str(r) = name_val {
        state.gc.string_arena.get(r).map_or_else(
            || String::from_utf8_lossy(&source).into_owned(),
            |s| String::from_utf8_lossy(s.data()).into_owned(),
        )
    } else {
        String::from_utf8_lossy(&source).into_owned()
    };

    load_string_impl(state, &source, &chunk_name)
}

/// Implements Lua's `loadfile([filename])`.
///
/// Reads and compiles a Lua file. If no filename is given, reads from stdin.
/// Returns the compiled function on success, or nil + error message on failure.
///
/// Reference: `luaB_loadfile` in `lbaselib.c`.
pub fn lua_loadfile(state: &mut LuaState) -> LuaResult<u32> {
    let filename_val = arg(state, 0);

    let source = if filename_val.is_nil() {
        // Read from stdin as bytes.
        use std::io::Read;
        let mut buf = Vec::new();
        if std::io::stdin().read_to_end(&mut buf).is_err() {
            state.push(Val::Nil);
            let msg = state.gc.intern_string(b"cannot read stdin");
            state.push(Val::Str(msg));
            return Ok(2);
        }
        (buf, "=stdin".to_string())
    } else {
        let Val::Str(r) = filename_val else {
            return Err(bad_argument("loadfile", 1, "string expected"));
        };
        let filename = state
            .gc
            .string_arena
            .get(r)
            .map(|s| String::from_utf8_lossy(s.data()).into_owned())
            .unwrap_or_default();

        match std::fs::read(&filename) {
            Ok(contents) => (contents, format!("@{filename}")),
            Err(e) => {
                state.push(Val::Nil);
                let msg = state
                    .gc
                    .intern_string(format!("cannot open {filename}: {e}").as_bytes());
                state.push(Val::Str(msg));
                return Ok(2);
            }
        }
    };

    load_string_impl(state, &source.0, &source.1)
}

/// Implements Lua's `dofile([filename])`.
///
/// Reads, compiles, and executes a Lua file. Unlike `loadfile`, raises
/// errors instead of returning nil+msg. Returns all values from the chunk.
///
/// Reference: `luaB_dofile` in `lbaselib.c`.
pub fn lua_dofile(state: &mut LuaState) -> LuaResult<u32> {
    let filename_val = arg(state, 0);

    let (source, chunk_name) = if filename_val.is_nil() {
        use std::io::Read;
        let mut buf = Vec::new();
        std::io::stdin()
            .read_to_end(&mut buf)
            .map_err(|e| simple_error(format!("cannot read stdin: {e}")))?;
        (buf, "=stdin".to_string())
    } else {
        let Val::Str(r) = filename_val else {
            return Err(bad_argument("dofile", 1, "string expected"));
        };
        let filename = state
            .gc
            .string_arena
            .get(r)
            .map(|s| String::from_utf8_lossy(s.data()).into_owned())
            .unwrap_or_default();

        let contents = std::fs::read(&filename)
            .map_err(|e| simple_error(format!("cannot open {filename}: {e}")))?;
        (contents, format!("@{filename}"))
    };

    // Compile or undump the source.
    let proto = crate::compile_or_undump(&source, &chunk_name)?;

    // Patch string constants.
    let mut proto =
        crate::vm::proto::ProtoRef::try_unwrap(proto).unwrap_or_else(|rc| (*rc).clone());
    crate::patch_string_constants(&mut proto, &mut state.gc);
    let proto = crate::vm::proto::ProtoRef::new(proto);

    // Create closure with the current global table as environment.
    let lua_cl = crate::vm::closure::LuaClosure::new(proto, state.global);
    let closure_ref = state
        .gc
        .alloc_closure(crate::vm::closure::Closure::Lua(lua_cl));

    // Set up the call.
    let call_base = state.top;
    state.ensure_stack(call_base + 2);
    state.stack_set(call_base, Val::Function(closure_ref));
    state.top = call_base + 1;

    state.call_function(call_base, LUA_MULTRET)?;

    // Return all results.
    let n_results = state.top - call_base;
    Ok(n_results as u32)
}

/// Calls the Lua reader function once and returns its result as bytes.
///
/// Returns `Ok(Some(bytes))` for data, `Ok(None)` for end-of-input (nil
/// or empty string), or `Err` for reader errors.
fn call_load_reader(state: &mut LuaState, func_val: Val) -> Result<Option<Vec<u8>>, LuaError> {
    let call_base = state.top;
    state.ensure_stack(call_base + 2);
    state.stack_set(call_base, func_val);
    state.top = call_base + 1;

    state.call_function(call_base, 1)?;

    let result = state.stack_get(call_base);
    state.top = call_base;

    match result {
        Val::Nil => Ok(None),
        Val::Str(r) => {
            let chunk = state
                .gc
                .string_arena
                .get(r)
                .map(|s| s.data().to_vec())
                .unwrap_or_default();
            if chunk.is_empty() {
                Ok(None)
            } else {
                Ok(Some(chunk))
            }
        }
        _ => Err(simple_error(
            "reader function must return a string".to_string(),
        )),
    }
}

/// Implements Lua's `load(func [, chunkname])`.
///
/// Loads a chunk by streaming data from a reader function. The reader is
/// called on demand as the lexer needs more input, matching PUC-Rio's ZIO
/// model where `luaZ_fill` calls the reader when the buffer is exhausted.
///
/// Binary chunks (starting with `\x1bLua`) are collected eagerly since the
/// undump module requires contiguous data. Text source is streamed through
/// the lexer -> parser -> codegen pipeline.
///
/// Reference: `luaB_load` in `lbaselib.c`.
pub fn lua_load(state: &mut LuaState) -> LuaResult<u32> {
    check_args("load", state, 1)?;
    let func_val = arg(state, 0);
    let name_val = arg(state, 1);

    if !matches!(func_val, Val::Function(_)) {
        return Err(bad_argument("load", 1, "function expected"));
    }

    let chunk_name = if let Val::Str(r) = name_val {
        state.gc.string_arena.get(r).map_or_else(
            || "=(load)".to_string(),
            |s| String::from_utf8_lossy(s.data()).to_string(),
        )
    } else {
        "=(load)".to_string()
    };

    // Save state for error recovery (reader calls may modify the call stack).
    let saved_top = state.top;
    let saved_ci = state.ci;
    let saved_n_ccalls = state.n_ccalls;
    let saved_call_depth = state.call_depth;

    // Call the reader once to get the first chunk. This determines whether
    // we have binary data (needs eager collection) or text (can stream).
    let first_chunk = match call_load_reader(state, func_val) {
        Ok(Some(bytes)) => bytes,
        Ok(None) => {
            // Empty input: compile empty source.
            return load_string_impl(state, b"", &chunk_name);
        }
        Err(e) => {
            // Reader error on first call. Restore state and return nil + msg.
            restore_state(state, saved_ci, saved_n_ccalls, saved_call_depth);
            state.top = saved_top;
            state.push(Val::Nil);
            let msg = state.gc.intern_string(e.to_string().as_bytes());
            state.push(Val::Str(msg));
            return Ok(2);
        }
    };

    // Binary chunk detection: first byte is \x1b (LUA_SIGNATURE prefix).
    // The undump module requires the complete binary data upfront.
    if first_chunk.first() == Some(&0x1b) {
        return load_binary_from_reader(
            state,
            func_val,
            first_chunk,
            &chunk_name,
            saved_top,
            saved_ci,
            saved_n_ccalls,
            saved_call_depth,
        );
    }

    // Text source: stream through the lexer on demand.
    //
    // The reader closure reborrows `state` for its entire lifetime. Once
    // compilation finishes (or fails), the Lexer and closure are dropped,
    // releasing the borrow so `state` can be used again.
    let compile_result = {
        let state_ref: &mut LuaState = state;
        let mut reader =
            move || -> Result<Option<Vec<u8>>, LuaError> { call_load_reader(state_ref, func_val) };
        let lexer =
            crate::compiler::lexer::Lexer::from_reader(first_chunk, &mut reader, &chunk_name);
        crate::compiler::compile_with_lexer(lexer, &chunk_name)
    };
    // Borrow of `state` released here; `state` is usable again.

    match compile_result {
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
            let closure_ref = state
                .gc
                .alloc_closure(crate::vm::closure::Closure::Lua(lua_cl));
            state.push(Val::Function(closure_ref));
            Ok(1)
        }
        Err(e) => {
            state.push(Val::Nil);
            let msg_bytes = match &e {
                crate::error::LuaError::Syntax(syn) => syn.to_lua_bytes(),
                _ => e.to_string().into_bytes(),
            };
            let msg = state.gc.intern_string(&msg_bytes);
            state.push(Val::Str(msg));
            Ok(2)
        }
    }
}

/// Eagerly collects remaining binary data from the reader and undumps it.
///
/// Binary chunks (precompiled bytecode from `string.dump`) cannot be streamed
/// because the undump module requires a contiguous byte slice.
#[allow(clippy::too_many_arguments)]
fn load_binary_from_reader(
    state: &mut LuaState,
    func_val: Val,
    first_chunk: Vec<u8>,
    chunk_name: &str,
    saved_top: usize,
    saved_ci: usize,
    saved_n_ccalls: u16,
    saved_call_depth: u16,
) -> LuaResult<u32> {
    const MAX_LOAD_SIZE: usize = 10 * 1024 * 1024;
    let mut collected = first_chunk;

    loop {
        match call_load_reader(state, func_val) {
            Ok(Some(bytes)) => {
                collected.extend_from_slice(&bytes);
                if collected.len() > MAX_LOAD_SIZE {
                    break;
                }
            }
            Ok(None) => break,
            Err(e) => {
                restore_state(state, saved_ci, saved_n_ccalls, saved_call_depth);
                state.top = saved_top;
                state.push(Val::Nil);
                let msg = state.gc.intern_string(e.to_string().as_bytes());
                state.push(Val::Str(msg));
                return Ok(2);
            }
        }
    }

    load_string_impl(state, &collected, chunk_name)
}

/// Restores call stack state after a failed reader call or compilation.
fn restore_state(
    state: &mut LuaState,
    saved_ci: usize,
    saved_n_ccalls: u16,
    saved_call_depth: u16,
) {
    state.ci = saved_ci;
    state.base = state.call_stack[state.ci].base;
    state.n_ccalls = saved_n_ccalls;
    state.call_depth = saved_call_depth;
    if state.ci < crate::vm::state::MAXCALLS {
        state.ci_overflow = false;
    }
}

/// Shared implementation for loadstring/loadfile/load: compiles source
/// and pushes either the function or nil+error.
#[allow(clippy::unnecessary_wraps)]
fn load_string_impl(state: &mut LuaState, source: &[u8], name: &str) -> LuaResult<u32> {
    match crate::compile_or_undump(source, name) {
        Ok(proto) => {
            let mut proto =
                crate::vm::proto::ProtoRef::try_unwrap(proto).unwrap_or_else(|rc| (*rc).clone());
            crate::patch_string_constants(&mut proto, &mut state.gc);
            let proto = crate::vm::proto::ProtoRef::new(proto);

            let num_upvalues = proto.num_upvalues as usize;
            let mut lua_cl = crate::vm::closure::LuaClosure::new(proto, state.global);
            // Binary-loaded functions (string.dump) may have upvalues that
            // need fresh nil-valued closed slots. Matches PUC-Rio's
            // luaU_undump which calls luaF_newupval for each nups.
            for _ in 0..num_upvalues {
                let uv = crate::vm::closure::Upvalue::new_closed(Val::Nil);
                let uv_ref = state.gc.alloc_upvalue(uv);
                lua_cl.upvalues.push(uv_ref);
            }
            let closure_ref = state
                .gc
                .alloc_closure(crate::vm::closure::Closure::Lua(lua_cl));
            state.push(Val::Function(closure_ref));
            Ok(1)
        }
        Err(e) => {
            state.push(Val::Nil);
            // Use raw bytes for syntax errors to preserve non-UTF-8 bytes
            // (e.g. \xFF in token names). Falls back to Display for others.
            let msg_bytes = match &e {
                crate::error::LuaError::Syntax(syn) => syn.to_lua_bytes(),
                _ => e.to_string().into_bytes(),
            };
            let msg = state.gc.intern_string(&msg_bytes);
            state.push(Val::Str(msg));
            Ok(2)
        }
    }
}

// ---------------------------------------------------------------------------
// collectgarbage
// ---------------------------------------------------------------------------

/// Implements Lua's `collectgarbage([opt [, arg]])`.
///
/// GC control interface. Until Phase 7 (GC collector), most operations
/// are stubs that return 0.
///
/// Reference: `luaB_collectgarbage` in `lbaselib.c`.
pub fn lua_collectgarbage(state: &mut LuaState) -> LuaResult<u32> {
    let opt_val = arg(state, 0);

    let opt = if opt_val.is_nil() {
        "collect"
    } else if let Val::Str(r) = opt_val {
        // We need to match the string. Extract it temporarily.
        let s = state
            .gc
            .string_arena
            .get(r)
            .map(|s| String::from_utf8_lossy(s.data()).to_string())
            .unwrap_or_default();
        // Match against known options.
        return collectgarbage_dispatch(state, &s);
    } else {
        return Err(bad_argument("collectgarbage", 1, "string expected"));
    };

    collectgarbage_dispatch(state, opt)
}

fn collectgarbage_dispatch(state: &mut LuaState, opt: &str) -> LuaResult<u32> {
    match opt {
        "collect" => {
            state.full_gc()?;
            state.push(Val::Num(0.0));
            Ok(1)
        }
        "stop" => {
            // PUC-Rio sets GCthreshold = MAX_LUMEM, preventing auto-GC.
            // A subsequent full GC (collectgarbage()) resets the threshold
            // via setthreshold(), re-enabling auto-GC.
            state.gc.gc_state.gc_threshold = usize::MAX;
            state.push(Val::Num(0.0));
            Ok(1)
        }
        "restart" => {
            // PUC-Rio sets GCthreshold = totalbytes, triggering auto-GC
            // on the next allocation.
            state.gc.gc_state.gc_threshold = state.gc.gc_state.total_bytes;
            state.push(Val::Num(0.0));
            Ok(1)
        }
        "count" => {
            let bytes = state.gc.gc_state.total_bytes;
            let kb = bytes as f64 / 1024.0;
            // PUC-Rio returns KB as first result, remainder bytes as second.
            state.push(Val::Num(kb));
            state.push(Val::Num((bytes % 1024) as f64));
            Ok(2)
        }
        "step" => {
            // Perform incremental GC work. The argument (data) controls how
            // much work: `(data << 10)` bytes worth of allocation are simulated.
            // Returns true if a full cycle completed.
            //
            // Reference: `lua_gc(LUA_GCSTEP)` in `lapi.c`.
            use crate::vm::gc::collector::{GCSTEPSIZE, GcPhase};

            let data_val = arg(state, 1);
            let data = match data_val {
                Val::Num(n) => n as u64,
                _ => 0,
            };

            // Simulate allocation debt: (data << 10) bytes.
            let simulated_alloc = data << 10;
            if simulated_alloc <= state.gc.gc_state.total_bytes as u64 {
                state.gc.gc_state.gc_threshold =
                    state.gc.gc_state.total_bytes - simulated_alloc as usize;
            } else {
                state.gc.gc_state.gc_threshold = 0;
            }

            // Run luaC_step-equivalent while threshold is exceeded.
            while state.gc.gc_state.gc_threshold <= state.gc.gc_state.total_bytes {
                // Compute budget: (GCSTEPSIZE/100) * stepmul.
                let stepmul = i64::from(state.gc.gc_state.gc_stepmul);
                let budget = if stepmul == 0 {
                    i64::MAX / 2
                } else {
                    (GCSTEPSIZE as i64 / 100) * stepmul
                };
                // Accumulate debt (PUC-Rio: gcdept += totalbytes - GCthreshold).
                state.gc.gc_state.gc_debt +=
                    state.gc.gc_state.total_bytes as i64 - state.gc.gc_state.gc_threshold as i64;
                let completed = state.gc_step(budget)?;
                if completed {
                    break;
                }
                // gc_step already adjusted gc_threshold on partial cycle,
                // so the while condition will exit.
            }

            let completed = state.gc.gc_state.phase == GcPhase::Pause;
            state.push(Val::Bool(completed));
            Ok(1)
        }
        "setpause" => {
            let arg_val = arg(state, 1);
            let new_pause = match arg_val {
                Val::Num(n) => n as u32,
                _ => 200,
            };
            let old = state.gc.gc_state.gc_pause;
            state.gc.gc_state.gc_pause = new_pause;
            state.push(Val::Num(f64::from(old)));
            Ok(1)
        }
        "setstepmul" => {
            let arg_val = arg(state, 1);
            let new_mul = match arg_val {
                Val::Num(n) => n as u32,
                _ => 200,
            };
            let old = state.gc.gc_state.gc_stepmul;
            state.gc.gc_state.gc_stepmul = new_mul;
            state.push(Val::Num(f64::from(old)));
            Ok(1)
        }
        _ => Err(bad_argument(
            "collectgarbage",
            1,
            &format!("invalid option '{opt}'"),
        )),
    }
}

// ---------------------------------------------------------------------------
// setfenv / getfenv
// ---------------------------------------------------------------------------

/// Implements Lua's `getfenv([f])`.
///
/// Returns the environment table of a function. If `f` is a number,
/// returns the environment of the function at that stack level.
/// Default level is 1 (the calling function).
///
/// Reference: `luaB_getfenv` in `lbaselib.c`.
pub fn lua_getfenv(state: &mut LuaState) -> LuaResult<u32> {
    let val = arg(state, 0);

    // Default: level 1.
    let func_val = match val {
        Val::Nil => get_func_at_level(state, 1)?,
        Val::Num(n) => {
            #[allow(clippy::cast_possible_truncation)]
            let level = n as i64;
            if level == 0 {
                // Level 0: return the thread's global environment.
                state.push(Val::Table(state.global));
                return Ok(1);
            }
            get_func_at_level(state, level as u32)?
        }
        Val::Function(_) => val,
        _ => {
            return Err(bad_argument("getfenv", 1, "number or function expected"));
        }
    };

    // For Rust functions, return the global environment.
    // For Lua functions, return the closure's environment.
    if let Val::Function(r) = func_val {
        let env = state.gc.closures.get(r).map(|cl| match cl {
            crate::vm::closure::Closure::Lua(lua_cl) => lua_cl.env,
            crate::vm::closure::Closure::Rust(_) => state.global,
        });
        state.push(Val::Table(env.unwrap_or(state.global)));
    } else {
        state.push(Val::Table(state.global));
    }
    Ok(1)
}

/// Implements Lua's `setfenv(f, table)`.
///
/// Sets the environment table of a function. `f` can be a function or
/// a stack level number. Level 0 changes the thread's global environment.
///
/// Reference: `luaB_setfenv` in `lbaselib.c`.
pub fn lua_setfenv(state: &mut LuaState) -> LuaResult<u32> {
    check_args("setfenv", state, 2)?;
    let f_val = arg(state, 0);
    let table_val = arg(state, 1);

    let Val::Table(new_env) = table_val else {
        return Err(bad_argument("setfenv", 2, "table expected"));
    };

    match f_val {
        Val::Num(n) => {
            #[allow(clippy::cast_possible_truncation)]
            let level = n as i64;
            if level == 0 {
                // Change thread's global environment.
                state.global = new_env;
                return Ok(0);
            }

            // Get the function at this stack level and set its env.
            let func_val = get_func_at_level(state, level as u32)?;
            set_func_env(state, func_val, new_env)?;
            state.push(func_val);
            Ok(1)
        }
        Val::Function(_) => {
            set_func_env(state, f_val, new_env)?;
            state.push(f_val);
            Ok(1)
        }
        _ => Err(bad_argument("setfenv", 1, "number or function expected")),
    }
}

/// Gets the function value at the given call stack level.
///
/// Level 1 = the direct caller, level 2 = its caller, etc.
/// Uses tail-call-aware level resolution matching PUC-Rio's
/// `lua_getstack`.
fn get_func_at_level(state: &LuaState, level: u32) -> LuaResult<Val> {
    use crate::stdlib::debug::{StackLevel, resolve_stack_level};

    match resolve_stack_level(state, level as usize) {
        Some(StackLevel::Real(ci_idx)) => {
            let func_idx = state.call_stack[ci_idx].func;
            Ok(state.stack_get(func_idx))
        }
        Some(StackLevel::TailCall) => {
            // Tail call virtual frames have no function -- error.
            Err(bad_argument(
                "getfenv",
                1,
                &format!("invalid level {level}"),
            ))
        }
        None => Err(bad_argument(
            "getfenv",
            1,
            &format!("invalid level {level}"),
        )),
    }
}

/// Sets the environment of a function value.
fn set_func_env(
    state: &mut LuaState,
    func_val: Val,
    new_env: crate::vm::gc::arena::GcRef<Table>,
) -> LuaResult<()> {
    let Val::Function(closure_ref) = func_val else {
        return Err(simple_error(
            "'setfenv' cannot change environment of given object".to_string(),
        ));
    };

    let cl = state.gc.closures.get_mut(closure_ref).ok_or_else(|| {
        simple_error("'setfenv' cannot change environment of given object".to_string())
    })?;

    match cl {
        crate::vm::closure::Closure::Lua(lua_cl) => {
            lua_cl.env = new_env;
            Ok(())
        }
        crate::vm::closure::Closure::Rust(_) => Err(simple_error(
            "'setfenv' cannot change environment of given object".to_string(),
        )),
    }
}

// ---------------------------------------------------------------------------
// newproxy
// ---------------------------------------------------------------------------

/// Implements Lua's `newproxy([boolean])`.
///
/// Creates a zero-size userdata. If `true` is passed, attaches an empty
/// metatable. This is an undocumented but present function in Lua 5.1.1.
///
/// Creates a zero-size userdata. With `true` argument, attaches an empty
/// metatable. With another proxy as argument, shares its metatable.
///
/// Reference: `luaB_newproxy` in `lbaselib.c`.
pub fn lua_newproxy(state: &mut LuaState) -> LuaResult<u32> {
    let arg_val = arg(state, 0);

    // Create a zero-size userdata.
    let ud = Userdata::new(Box::new(()));
    let ud_ref = state.gc.alloc_userdata(ud);

    if arg_val == Val::Bool(true) {
        // Attach an empty metatable.
        let mt = state.gc.alloc_table(Table::new());
        if let Some(u) = state.gc.userdata.get_mut(ud_ref) {
            u.set_metatable(Some(mt));
        }
    } else if let Val::Userdata(other_ref) = arg_val {
        // Share the metatable from an existing proxy.
        let other_mt = state
            .gc
            .userdata
            .get(other_ref)
            .and_then(Userdata::metatable);
        if let (Some(mt), Some(u)) = (other_mt, state.gc.userdata.get_mut(ud_ref)) {
            u.set_metatable(Some(mt));
        }
    }

    state.push(Val::Userdata(ud_ref));
    Ok(1)
}

// ---------------------------------------------------------------------------
// gcinfo (deprecated)
// ---------------------------------------------------------------------------

/// Implements Lua's deprecated `gcinfo()`.
///
/// Returns the total memory in use (in Kbytes).
/// Equivalent to `collectgarbage("count")` but returns an integer.
///
/// Reference: `luaB_gcinfo` in `lbaselib.c`.
pub fn lua_gcinfo(state: &mut LuaState) -> LuaResult<u32> {
    let bytes = state.gc.estimate_memory();
    let kb = bytes as f64 / 1024.0;
    #[allow(clippy::cast_possible_truncation)]
    state.push(Val::Num(f64::from(kb.floor() as i32)));
    Ok(1)
}
