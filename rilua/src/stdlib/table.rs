//! Table library: concat, insert, remove, sort, maxn, getn, setn, foreach, foreachi.
//!
//! Reference: `ltablib.c` in PUC-Rio Lua 5.1.1.

use crate::error::{LuaError, LuaResult, RuntimeError};
use crate::vm::gc::arena::GcRef;
use crate::vm::metatable::{TMS, gettmbyobj, val_raw_equal};
use crate::vm::state::LuaState;
use crate::vm::table::Table;
use crate::vm::value::Val;

// ---------------------------------------------------------------------------
// Argument helpers (same pattern as string.rs / base.rs)
// ---------------------------------------------------------------------------

#[inline]
fn nargs(state: &LuaState) -> usize {
    state.top.saturating_sub(state.base)
}

#[inline]
fn arg(state: &LuaState, n: usize) -> Val {
    let idx = state.base + n;
    if idx < state.top {
        state.stack_get(idx)
    } else {
        Val::Nil
    }
}

fn bad_argument(name: &str, n: usize, msg: &str) -> LuaError {
    LuaError::Runtime(RuntimeError {
        message: format!("bad argument #{n} to '{name}' ({msg})"),
        level: 0,
        traceback: vec![],
    })
}

#[allow(dead_code)]
fn check_args(name: &str, state: &LuaState, min: usize) -> LuaResult<()> {
    if nargs(state) < min {
        Err(bad_argument(name, min, "value expected"))
    } else {
        Ok(())
    }
}

fn simple_error(msg: String) -> LuaError {
    LuaError::Runtime(RuntimeError {
        message: msg,
        level: 0,
        traceback: vec![],
    })
}

/// Validates that argument `n` (0-indexed) is a table and returns its `GcRef`.
fn check_table(state: &LuaState, name: &str, n: usize) -> LuaResult<GcRef<Table>> {
    match arg(state, n) {
        Val::Table(r) => Ok(r),
        _ => Err(bad_argument(name, n + 1, "table expected")),
    }
}

// ---------------------------------------------------------------------------
// Raw table access helpers
// ---------------------------------------------------------------------------

/// Raw get by integer key.
fn get_raw(state: &LuaState, tref: GcRef<Table>, idx: i64) -> Val {
    state
        .gc
        .tables
        .get(tref)
        .map_or(Val::Nil, |t| t.get_int(idx))
}

/// Raw set by numeric key.
fn set_raw(state: &mut LuaState, tref: GcRef<Table>, idx: i64, val: Val) -> LuaResult<()> {
    let strings = &state.gc.string_arena;
    let t = state
        .gc
        .tables
        .get_mut(tref)
        .ok_or_else(|| simple_error("table not found".into()))?;
    #[allow(clippy::cast_precision_loss)]
    t.raw_set(Val::Num(idx as f64), val, strings)
}

/// Get the length of a table (#t).
fn table_len(state: &LuaState, tref: GcRef<Table>) -> usize {
    state
        .gc
        .tables
        .get(tref)
        .map_or(0, |t| t.len(&state.gc.string_arena))
}

// ---------------------------------------------------------------------------
// table.getn(table) -> #table
// ---------------------------------------------------------------------------

pub fn tab_getn(state: &mut LuaState) -> LuaResult<u32> {
    let tref = check_table(state, "getn", 0)?;
    let n = table_len(state, tref);
    #[allow(clippy::cast_precision_loss)]
    state.stack_set(state.base, Val::Num(n as f64));
    state.top = state.base + 1;
    Ok(1)
}

// ---------------------------------------------------------------------------
// table.setn -> error (obsolete)
// ---------------------------------------------------------------------------

pub fn tab_setn(state: &mut LuaState) -> LuaResult<u32> {
    check_table(state, "setn", 0)?;
    Err(simple_error("'setn' is obsolete".into()))
}

// ---------------------------------------------------------------------------
// table.maxn(table) -> largest positive numeric key
// ---------------------------------------------------------------------------

pub fn tab_maxn(state: &mut LuaState) -> LuaResult<u32> {
    let tref = check_table(state, "maxn", 0)?;
    let mut max = 0.0_f64;

    // Iterate all keys using raw next.
    let mut key = Val::Nil;
    loop {
        let next = {
            let t = state
                .gc
                .tables
                .get(tref)
                .ok_or_else(|| simple_error("table not found".into()))?;
            t.next(key, &state.gc.string_arena)?
        };
        match next {
            Some((k, _v)) => {
                if let Val::Num(n) = k
                    && n > max
                {
                    max = n;
                }
                key = k;
            }
            None => break,
        }
    }

    state.stack_set(state.base, Val::Num(max));
    state.top = state.base + 1;
    Ok(1)
}

// ---------------------------------------------------------------------------
// table.concat(table [, sep [, i [, j]]])
// ---------------------------------------------------------------------------

#[allow(clippy::many_single_char_names)]
pub fn tab_concat(state: &mut LuaState) -> LuaResult<u32> {
    let tref = check_table(state, "concat", 0)?;

    // sep (default "")
    let sep = match arg(state, 1) {
        Val::Nil => Vec::new(),
        Val::Str(r) => state
            .gc
            .string_arena
            .get(r)
            .map_or_else(Vec::new, |s| s.data().to_vec()),
        Val::Num(n) => format!("{}", Val::Num(n)).into_bytes(),
        _ => return Err(bad_argument("concat", 2, "string expected")),
    };

    // i (default 1)
    let i = match arg(state, 2) {
        Val::Nil => 1_i64,
        Val::Num(n) => n as i64,
        _ => return Err(bad_argument("concat", 3, "number expected")),
    };

    // j (default #table)
    let j = match arg(state, 3) {
        Val::Nil => {
            #[allow(clippy::cast_possible_wrap)]
            let len = table_len(state, tref) as i64;
            len
        }
        Val::Num(n) => n as i64,
        _ => return Err(bad_argument("concat", 4, "number expected")),
    };

    let mut result: Vec<u8> = Vec::new();
    let mut idx = i;
    while idx <= j {
        if idx != i {
            result.extend_from_slice(&sep);
        }
        let val = get_raw(state, tref, idx);
        match val {
            Val::Str(r) => {
                let data = state
                    .gc
                    .string_arena
                    .get(r)
                    .map_or(b"".as_slice(), |s| s.data());
                result.extend_from_slice(data);
            }
            Val::Num(n) => {
                let formatted = format!("{}", Val::Num(n));
                result.extend_from_slice(formatted.as_bytes());
            }
            _ => {
                return Err(bad_argument("concat", 1, "table contains non-strings"));
            }
        }
        idx += 1;
    }

    let str_ref = state.gc.intern_string(&result);
    state.stack_set(state.base, Val::Str(str_ref));
    state.top = state.base + 1;
    Ok(1)
}

// ---------------------------------------------------------------------------
// table.insert(table, [pos,] value)
// ---------------------------------------------------------------------------

pub fn tab_insert(state: &mut LuaState) -> LuaResult<u32> {
    let tref = check_table(state, "insert", 0)?;
    #[allow(clippy::cast_possible_wrap)]
    let e = table_len(state, tref) as i64 + 1; // first empty element

    let n = nargs(state);
    match n {
        2 => {
            // table.insert(t, value) -> append at end
            let value = arg(state, 1);
            set_raw(state, tref, e, value)?;
        }
        3 => {
            // table.insert(t, pos, value)
            let pos = match arg(state, 1) {
                Val::Num(n) => n as i64,
                _ => return Err(bad_argument("insert", 2, "number expected")),
            };
            let value = arg(state, 2);
            let end = if pos > e { pos } else { e };
            // Shift elements up
            let mut idx = end;
            while idx > pos {
                let v = get_raw(state, tref, idx - 1);
                set_raw(state, tref, idx, v)?;
                idx -= 1;
            }
            set_raw(state, tref, pos, value)?;
        }
        _ => {
            return Err(simple_error("wrong number of arguments to 'insert'".into()));
        }
    }
    // Write barrier: table was mutated.
    state.gc.barrier_back(tref);

    state.top = state.base;
    Ok(0)
}

// ---------------------------------------------------------------------------
// table.remove(table [, pos])
// ---------------------------------------------------------------------------

pub fn tab_remove(state: &mut LuaState) -> LuaResult<u32> {
    let tref = check_table(state, "remove", 0)?;
    #[allow(clippy::cast_possible_wrap)]
    let e = table_len(state, tref) as i64;

    if e == 0 {
        state.top = state.base;
        return Ok(0);
    }

    let pos = match arg(state, 1) {
        Val::Nil => e,
        Val::Num(n) => n as i64,
        _ => return Err(bad_argument("remove", 2, "number expected")),
    };

    // Save the removed value.
    let removed = get_raw(state, tref, pos);

    // Shift elements down.
    let mut p = pos;
    while p < e {
        let v = get_raw(state, tref, p + 1);
        set_raw(state, tref, p, v)?;
        p += 1;
    }
    // Set last element to nil.
    set_raw(state, tref, e, Val::Nil)?;
    // Write barrier: table was mutated.
    state.gc.barrier_back(tref);

    state.stack_set(state.base, removed);
    state.top = state.base + 1;
    Ok(1)
}

// ---------------------------------------------------------------------------
// table.foreach(table, f) -- deprecated
// ---------------------------------------------------------------------------

pub fn tab_foreach(state: &mut LuaState) -> LuaResult<u32> {
    let tref = check_table(state, "foreach", 0)?;
    let func_val @ Val::Function(_) = arg(state, 1) else {
        return Err(bad_argument("foreach", 2, "function expected"));
    };

    // Iterate all keys via raw next.
    let mut key = Val::Nil;
    loop {
        let next = {
            let t = state
                .gc
                .tables
                .get(tref)
                .ok_or_else(|| simple_error("table not found".into()))?;
            t.next(key, &state.gc.string_arena)?
        };
        let Some((k, v)) = next else { break };

        // Call f(key, value).
        let call_base = state.top;
        state.ensure_stack(call_base + 4);
        state.stack_set(call_base, func_val);
        state.stack_set(call_base + 1, k);
        state.stack_set(call_base + 2, v);
        state.top = call_base + 3;

        state.call_function(call_base, 1)?;

        let result = state.stack_get(call_base);
        state.top = call_base;

        if !result.is_nil() {
            // Return the non-nil result.
            state.stack_set(state.base, result);
            state.top = state.base + 1;
            return Ok(1);
        }

        key = k;
    }

    state.top = state.base;
    Ok(0)
}

// ---------------------------------------------------------------------------
// table.foreachi(table, f) -- deprecated
// ---------------------------------------------------------------------------

pub fn tab_foreachi(state: &mut LuaState) -> LuaResult<u32> {
    let tref = check_table(state, "foreachi", 0)?;
    let func_val @ Val::Function(_) = arg(state, 1) else {
        return Err(bad_argument("foreachi", 2, "function expected"));
    };

    #[allow(clippy::cast_possible_wrap)]
    let n = table_len(state, tref) as i64;

    for i in 1..=n {
        let v = get_raw(state, tref, i);

        // Call f(i, value).
        let call_base = state.top;
        state.ensure_stack(call_base + 4);
        state.stack_set(call_base, func_val);
        #[allow(clippy::cast_precision_loss)]
        state.stack_set(call_base + 1, Val::Num(i as f64));
        state.stack_set(call_base + 2, v);
        state.top = call_base + 3;

        state.call_function(call_base, 1)?;

        let result = state.stack_get(call_base);
        state.top = call_base;

        if !result.is_nil() {
            state.stack_set(state.base, result);
            state.top = state.base + 1;
            return Ok(1);
        }
    }

    state.top = state.base;
    Ok(0)
}

// ---------------------------------------------------------------------------
// table.sort(table [, comp])
// ---------------------------------------------------------------------------

/// Entry point for `table.sort(table [, comp])`.
///
/// Implements PUC-Rio's `sort` from `ltablib.c`: validates arguments,
/// extracts the optional comparator, and delegates to `auxsort`.
pub fn tab_sort(state: &mut LuaState) -> LuaResult<u32> {
    let tref = check_table(state, "sort", 0)?;
    #[allow(clippy::cast_possible_wrap)]
    let n = table_len(state, tref) as i64;

    // Optional comparison function (arg 1).
    let comp = match arg(state, 1) {
        Val::Nil => None,
        v @ Val::Function(_) => Some(v),
        _ => return Err(bad_argument("sort", 2, "function expected")),
    };

    if n > 1 {
        auxsort(state, tref, 1, n, comp)?;
        // Write barrier: table was mutated during sort.
        state.gc.barrier_back(tref);
    }

    state.top = state.base;
    Ok(0)
}

/// Compare two values using a custom comparison function or default less-than.
///
/// If `comp` is `Some(f)`, calls `f(a, b)` and returns the truthiness of the result.
/// Otherwise, uses default less-than semantics (matching `lua_lessthan`).
fn sort_comp(state: &mut LuaState, a: Val, b: Val, comp: Option<Val>) -> LuaResult<bool> {
    if let Some(func_val) = comp {
        // Call comp(a, b).
        let call_base = state.top;
        state.ensure_stack(call_base + 4);
        state.stack_set(call_base, func_val);
        state.stack_set(call_base + 1, a);
        state.stack_set(call_base + 2, b);
        state.top = call_base + 3;

        state.call_function(call_base, 1)?;

        let result = state.stack_get(call_base);
        state.top = call_base;
        Ok(result.is_truthy())
    } else {
        default_less_than(state, a, b)
    }
}

/// Default less-than comparison, replicating `luaV_lessthan` logic.
///
/// - (Num, Num) -> x < y
/// - (Str, Str) -> lexicographic byte comparison
/// - Same-type with `__lt` metamethod -> call metamethod
/// - Otherwise -> error
fn default_less_than(state: &mut LuaState, a: Val, b: Val) -> LuaResult<bool> {
    match (&a, &b) {
        (Val::Num(x), Val::Num(y)) => Ok(*x < *y),
        (Val::Str(x), Val::Str(y)) => {
            let xdata = state
                .gc
                .string_arena
                .get(*x)
                .map_or(b"".as_slice(), |s| s.data());
            let ydata = state
                .gc
                .string_arena
                .get(*y)
                .map_or(b"".as_slice(), |s| s.data());
            Ok(xdata < ydata)
        }
        _ => {
            // Try __lt metamethod on both operands.
            let tm_a = gettmbyobj(
                a,
                TMS::Lt,
                &state.gc.tables,
                &state.gc.string_arena,
                &state.gc.type_metatables,
                &state.gc.tm_names,
                &state.gc.userdata,
            );
            let tm_b = gettmbyobj(
                b,
                TMS::Lt,
                &state.gc.tables,
                &state.gc.string_arena,
                &state.gc.type_metatables,
                &state.gc.tm_names,
                &state.gc.userdata,
            );

            match (tm_a, tm_b) {
                (Some(tm_val_a), Some(tm_val_b)) => {
                    // Both must have the same __lt metamethod.
                    if !val_raw_equal(tm_val_a, tm_val_b, &state.gc.tables, &state.gc.string_arena)
                    {
                        return Err(simple_error(format!(
                            "attempt to compare two {} values",
                            a.type_name()
                        )));
                    }
                    // Call the metamethod.
                    let call_base = state.top;
                    state.ensure_stack(call_base + 4);
                    state.stack_set(call_base, tm_val_a);
                    state.stack_set(call_base + 1, a);
                    state.stack_set(call_base + 2, b);
                    state.top = call_base + 3;

                    state.call_function(call_base, 1)?;

                    let result = state.stack_get(call_base);
                    state.top = call_base;
                    Ok(result.is_truthy())
                }
                _ => Err(simple_error(format!(
                    "attempt to compare two {} values",
                    a.type_name()
                ))),
            }
        }
    }
}

/// Quicksort implementation, faithful port of PUC-Rio's `auxsort`.
///
/// Uses median-of-three pivot selection, Hoare partitioning, and tail
/// recursion on the larger partition.
fn auxsort(
    state: &mut LuaState,
    tref: GcRef<Table>,
    mut l: i64,
    mut u: i64,
    comp: Option<Val>,
) -> LuaResult<()> {
    while l < u {
        // Sort elements a[l], a[u].
        let al = get_raw(state, tref, l);
        let au = get_raw(state, tref, u);
        if sort_comp(state, au, al, comp)? {
            // a[u] < a[l]: swap
            set_raw(state, tref, l, au)?;
            set_raw(state, tref, u, al)?;
        }
        if u - l == 1 {
            break; // Only 2 elements.
        }

        let mid = i64::midpoint(l, u);
        let amid = get_raw(state, tref, mid);
        let al = get_raw(state, tref, l);

        if sort_comp(state, amid, al, comp)? {
            // a[mid] < a[l]: swap
            set_raw(state, tref, mid, al)?;
            set_raw(state, tref, l, amid)?;
        } else {
            let au = get_raw(state, tref, u);
            if sort_comp(state, au, amid, comp)? {
                // a[u] < a[mid]: swap
                set_raw(state, tref, mid, au)?;
                set_raw(state, tref, u, amid)?;
            }
        }
        if u - l == 2 {
            break; // Only 3 elements.
        }

        // Pivot = a[mid]. Swap pivot with a[u-1].
        let pivot = get_raw(state, tref, mid);
        let au1 = get_raw(state, tref, u - 1);
        set_raw(state, tref, mid, au1)?;
        set_raw(state, tref, u - 1, pivot)?;

        // Partition: a[l..i] <= pivot <= a[j..u]
        let mut i = l;
        let mut j = u - 1;

        loop {
            // Scan right: find first a[i] >= pivot
            i += 1;
            while sort_comp(state, get_raw(state, tref, i), pivot, comp)? {
                if i > u {
                    return Err(simple_error("invalid order function for sorting".into()));
                }
                i += 1;
            }
            // Scan left: find first a[j] <= pivot
            j -= 1;
            while sort_comp(state, pivot, get_raw(state, tref, j), comp)? {
                if j < l {
                    return Err(simple_error("invalid order function for sorting".into()));
                }
                j -= 1;
            }

            if j < i {
                break;
            }

            // Swap a[i] and a[j].
            let ai = get_raw(state, tref, i);
            let aj = get_raw(state, tref, j);
            set_raw(state, tref, i, aj)?;
            set_raw(state, tref, j, ai)?;
        }

        // Place pivot at position i.
        let au1 = get_raw(state, tref, u - 1);
        let ai = get_raw(state, tref, i);
        set_raw(state, tref, u - 1, ai)?;
        set_raw(state, tref, i, au1)?;

        // Recurse on smaller partition, loop on larger (tail recursion).
        if i - l < u - i {
            auxsort(state, tref, l, i - 1, comp)?;
            l = i + 1;
        } else {
            auxsort(state, tref, i + 1, u, comp)?;
            u = i - 1;
        }
    }
    Ok(())
}
