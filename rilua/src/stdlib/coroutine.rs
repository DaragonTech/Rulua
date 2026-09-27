//! Coroutine library: create, resume, yield, wrap, status, running.
//!
//! Reference: coroutine functions in `lbaselib.c`, `lua_resume`/`lua_yield`
//! in `ldo.c`, PUC-Rio Lua 5.1.1.
//!
//! ## Architecture
//!
//! rilua uses a "swap model" for coroutines: `LuaState` always represents
//! the currently executing thread. When resuming a coroutine, the resumer's
//! per-thread state (stack, call_stack, etc.) is saved to a `LuaThread`
//! on the Rust stack, the coroutine's state is loaded from the GC arena
//! into `LuaState`, execution proceeds, and then the states are swapped
//! back.
//!
//! Yield is implemented via `LuaError::Yield(n_results)`. This error
//! propagates through the Rust call stack back to the resume handler,
//! which catches it and treats it as a successful yield.
//!
//! The `n_ccalls` counter (managed by `call_function`, the `luaD_call`
//! equivalent) determines the yield boundary: yield is only allowed when
//! `n_ccalls == 0`, preventing yield across C-call boundaries.

use crate::error::{LuaError, LuaResult, RuntimeError};
use crate::vm::callinfo::LUA_MULTRET;
use crate::vm::closure::{Closure, RustClosure, Upvalue};
use crate::vm::execute::{self, CallResult};
use crate::vm::gc::arena::GcRef;
use crate::vm::state::{LuaState, LuaThread, ThreadStatus};
use crate::vm::value::Val;

// ---------------------------------------------------------------------------
// Argument helpers (same pattern as other stdlib modules)
// ---------------------------------------------------------------------------

/// Returns the number of arguments passed to the current Rust function.
fn nargs(state: &LuaState) -> usize {
    let func = state.call_stack[state.ci].func;
    if state.top > func + 1 {
        state.top - func - 1
    } else {
        0
    }
}

/// Returns argument `n` (0-based) to the current Rust function.
fn arg(state: &LuaState, n: usize) -> Val {
    let func = state.call_stack[state.ci].func;
    state.stack_get(func + 1 + n)
}

/// Creates a simple runtime error.
fn simple_error(msg: String) -> LuaError {
    LuaError::Runtime(RuntimeError {
        message: msg,
        level: 0,
        traceback: vec![],
    })
}

// ---------------------------------------------------------------------------
// coroutine.create(f)
// ---------------------------------------------------------------------------

/// Creates a new coroutine with body `f`. `f` must be a Lua function.
///
/// Returns the new coroutine (as a thread value).
///
/// Reference: `luaB_cocreate` in `lbaselib.c`.
pub fn co_create(state: &mut LuaState) -> LuaResult<u32> {
    let func_val = arg(state, 0);

    // Validate: must be a Lua function (not a Rust function).
    match func_val {
        Val::Function(r) => {
            let cl = state
                .gc
                .closures
                .get(r)
                .ok_or_else(|| simple_error("invalid function reference".into()))?;
            if matches!(cl, Closure::Rust(_)) {
                return Err(simple_error(
                    "bad argument #1 to 'create' (Lua function expected)".into(),
                ));
            }
        }
        _ => {
            return Err(simple_error(
                "bad argument #1 to 'create' (Lua function expected)".into(),
            ));
        }
    }

    // Create a new thread with the function on its stack.
    let thread = LuaThread::new(func_val, state.global);
    let thread_ref = state.gc.alloc_thread(thread);

    state.push(Val::Thread(thread_ref));
    Ok(1)
}

// ---------------------------------------------------------------------------
// coroutine.resume(co, ...)
// ---------------------------------------------------------------------------

/// Resumes a coroutine. Returns `true, results...` on success or yield,
/// `false, error_message` on error.
///
/// Reference: `luaB_coresume` + `auxresume` in `lbaselib.c`,
/// `lua_resume` in `ldo.c`.
pub fn co_resume(state: &mut LuaState) -> LuaResult<u32> {
    let co_val = arg(state, 0);
    let Val::Thread(co_ref) = co_val else {
        return Err(simple_error(
            "bad argument #1 to 'resume' (coroutine expected)".into(),
        ));
    };

    // Collect arguments to pass to the coroutine.
    let n_resume_args = if nargs(state) > 1 {
        nargs(state) - 1
    } else {
        0
    };
    let mut resume_args = Vec::with_capacity(n_resume_args);
    for i in 0..n_resume_args {
        resume_args.push(arg(state, 1 + i));
    }

    // Run auxresume and translate the result.
    match auxresume(state, co_ref, &resume_args) {
        Ok(results) => {
            // Success or yield: push true + all results.
            let base = state.base;
            state.stack_set(base, Val::Bool(true));
            for (i, val) in results.iter().enumerate() {
                state.stack_set(base + 1 + i, *val);
            }
            state.top = base + 1 + results.len();
            Ok((1 + results.len()) as u32)
        }
        Err(error_val) => {
            // Error: push false + error value (preserved as-is).
            // PUC-Rio: lua_pushboolean(L, 0); lua_insert(L, -2); return 2;
            let base = state.base;
            state.stack_set(base, Val::Bool(false));
            state.stack_set(base + 1, error_val);
            state.top = base + 2;
            Ok(2)
        }
    }
}

/// Closes ALL open upvalues from the current thread and returns pairs
/// of `(upvalue_ref, stack_index)` for later reopening.
///
/// In rilua's swap model, the active thread's stack is about to be saved
/// and another thread's stack loaded. ANY open upvalue pointing into the
/// current stack would read from the wrong thread after the swap. We
/// close them all, capturing their current values into `Closed` state.
///
/// The returned pairs allow `load_thread_by_ref` to reopen the upvalues
/// when this thread becomes active again.
///
/// Must be called BEFORE `save_thread_state` while the thread's stack
/// is still active in `state.stack`.
fn close_thread_upvalues(state: &mut LuaState) -> Vec<(GcRef<Upvalue>, usize)> {
    let mut suspended = Vec::new();
    for &uv_ref in &state.open_upvalues {
        if let Some(uv) = state.gc.upvalues.get(uv_ref)
            && let Some(idx) = uv.stack_index()
        {
            suspended.push((uv_ref, idx));
        }
    }
    for &(uv_ref, _) in &suspended {
        if let Some(uv) = state.gc.upvalues.get_mut(uv_ref) {
            uv.close(&state.stack);
        }
    }
    suspended
}

/// Core resume logic shared by `coroutine.resume` and `coroutine.wrap`.
///
/// Returns `Ok(results)` on success/yield, `Err(error_val)` on error.
/// The error value is preserved as a `Val` to match PUC-Rio behavior
/// where non-string error objects (functions, tables, etc.) pass through
/// `resume`/`wrap` without stringification.
///
/// Reference: `auxresume` in `lbaselib.c`.
pub fn auxresume(
    state: &mut LuaState,
    co_ref: GcRef<LuaThread>,
    args: &[Val],
) -> Result<Vec<Val>, Val> {
    // Check coroutine status.
    let co_status = state
        .gc
        .threads
        .get(co_ref)
        .map_or(ThreadStatus::Dead, |t| t.status);

    match co_status {
        ThreadStatus::Dead => {
            let r = state.gc.intern_string(b"cannot resume dead coroutine");
            return Err(Val::Str(r));
        }
        ThreadStatus::Running | ThreadStatus::Normal => {
            let r = state
                .gc
                .intern_string(b"cannot resume non-suspended coroutine");
            return Err(Val::Str(r));
        }
        ThreadStatus::Initial | ThreadStatus::Suspended => {
            // OK to resume.
        }
    }

    // Close ALL open upvalues from the current (resumer) thread before the
    // stack swap. Any open upvalue pointing into this thread's stack would
    // read from the wrong data after the coroutine's stack is loaded.
    // The suspended pairs are stored on the resumer so they can be reopened
    // when control returns to this thread.
    let resumer_suspended = close_thread_upvalues(state);

    // Save the identity of the calling thread (for nested resume tracking).
    let saved_current_thread = state.current_thread;

    // Save the resumer's state onto saved_threads so the GC can
    // traverse it during coroutine execution. Without this, the
    // resumer's stack values (closures, strings, etc.) would be
    // invisible to the GC and could be incorrectly freed.
    let mut resumer = state.save_thread_state();
    resumer.suspended_upvals = resumer_suspended;
    state.saved_threads.push(resumer);

    // Load the coroutine's state into LuaState.
    state.load_thread_by_ref(co_ref, ThreadStatus::Running);

    // Track which thread is active.
    state.current_thread = Some(co_ref);

    // Transfer arguments to the coroutine's stack.
    if co_status == ThreadStatus::Initial {
        // First resume: push args after the function (they become function args).
        for &val in args {
            state.push(val);
        }
    } else {
        // Resuming from yield: the yield's CI is still on the call stack.
        // Push resume args as the "return values" of yield.
        // The yielded values area starts at state.base. Replace with resume args.
        for (i, &val) in args.iter().enumerate() {
            state.stack_set(state.base + i, val);
        }
        state.top = state.base + args.len();
    }

    // Execute the coroutine.
    let exec_result = if co_status == ThreadStatus::Initial {
        // First resume: call the function at stack[0].
        // Use precall+execute directly (not call_function) so n_ccalls stays 0.
        // This allows yield within the coroutine body.
        let func_idx = 0;
        (|| -> LuaResult<()> {
            match state.precall(func_idx, LUA_MULTRET)? {
                CallResult::Lua => execute::execute(state),
                CallResult::Rust => Ok(()),
            }
        })()
    } else if state.yielded_in_hook {
        // Resuming from a hook yield: the execute loop yielded directly
        // at a hook dispatch point (via yield_on_hook). No Rust/Lua hook
        // function was called, so there's no CI to pop with poscall.
        // Just restore base and continue execution from the saved PC.
        state.base = state.call_stack[state.ci].base;
        if state.top < state.base {
            state.top = state.base;
        }
        state.yielded_in_hook = false;

        (|| -> LuaResult<()> {
            while state.ci > 0 {
                execute::execute(state)?;
            }
            Ok(())
        })()
    } else {
        // Resuming from yield: the coroutine was suspended inside a Rust
        // function (yield). We need to complete the interrupted call.
        //
        // In PUC-Rio, resume() calls poscall for the interrupted C function,
        // then luaV_execute(L, ci - base_ci). The nexeccalls parameter makes
        // PUC-Rio's flat loop process ALL remaining CI levels.
        //
        // In rilua's recursive model, each execute() handles one function
        // level. When yield unwound the Rust call stack, the nested
        // execute() frames were lost. We must loop, calling execute() for
        // each CI level until the coroutine's base function completes
        // (ci == 0) or a new yield occurs.
        //
        // The resume args are at state.base..state.top. poscall reads from
        // first_result and places results at the caller's expected position.
        let first_result = state.base;
        if state.poscall(first_result) {
            state.top = state.call_stack[state.ci].top;
        }

        // Continue execution from where we left off.
        // Loop to handle all CI levels that were active when yield happened.
        // ci[0] is the sentinel base CI — execute only runs for ci > 0.
        // Each execute() handles one function level (OP_RETURN pops the CI).
        // When ci reaches 0, the coroutine's function has returned normally.
        (|| -> LuaResult<()> {
            while state.ci > 0 {
                execute::execute(state)?;
            }
            Ok(())
        })()
    };

    // Determine outcome and collect results.
    match exec_result {
        Ok(()) => {
            // Coroutine returned normally. Collect return values.
            // After execute returns from OP_RETURN at the top level,
            // results are at the coroutine's stack base area.
            let mut results = Vec::new();
            let ci_func = state.call_stack[state.ci].func;
            for i in ci_func..state.top {
                results.push(state.stack_get(i));
            }

            // Save coroutine state back (dead) and restore resumer.
            // Pop is safe: we push exactly once before execute, pop once after.
            let Some(resumer) = state.saved_threads.pop() else {
                let r = state
                    .gc
                    .intern_string(b"internal error: missing resumer state");
                return Err(Val::Str(r));
            };
            state.save_and_restore_by_ref(co_ref, ThreadStatus::Dead, resumer);
            state.current_thread = saved_current_thread;
            Ok(results)
        }
        Err(LuaError::Yield(n_results)) => {
            // Coroutine yielded. Collect yielded values.
            // The values are the top n_results on the stack.
            let mut results = Vec::new();
            let start = state.top.saturating_sub(n_results as usize);
            for i in start..state.top {
                results.push(state.stack_get(i));
            }

            // Save coroutine state as suspended and restore resumer.
            // Pop is safe: we push exactly once before execute, pop once after.
            let Some(resumer) = state.saved_threads.pop() else {
                let r = state
                    .gc
                    .intern_string(b"internal error: missing resumer state");
                return Err(Val::Str(r));
            };
            state.save_and_restore_by_ref(co_ref, ThreadStatus::Suspended, resumer);
            state.current_thread = saved_current_thread;
            Ok(results)
        }
        Err(err) => {
            // Coroutine errored. Preserve the error value as-is.
            // PUC-Rio: lua_xmove(co, L, 1) moves the error value directly.
            let error_val = state.error_object.take().unwrap_or_else(|| {
                let r = state.gc.intern_string(err.to_string().as_bytes());
                Val::Str(r)
            });

            // Save coroutine state as dead and restore resumer.
            // Pop is safe: we push exactly once before execute, pop once after.
            let Some(resumer) = state.saved_threads.pop() else {
                let r = state
                    .gc
                    .intern_string(b"internal error: missing resumer state");
                return Err(Val::Str(r));
            };
            state.save_and_restore_by_ref(co_ref, ThreadStatus::Dead, resumer);
            state.current_thread = saved_current_thread;
            Err(error_val)
        }
    }
}

// ---------------------------------------------------------------------------
// coroutine.yield(...)
// ---------------------------------------------------------------------------

/// Suspends the currently running coroutine.
///
/// All arguments become the results of `coroutine.resume()` in the
/// resumer. When the coroutine is resumed again, `yield` returns
/// the arguments passed to `resume`.
///
/// Reference: `luaB_yield` + `lua_yield` in PUC-Rio Lua 5.1.1.
pub fn co_yield(state: &mut LuaState) -> LuaResult<u32> {
    // Check yield boundary: can't yield from the main thread or across
    // C-call boundaries. PUC-Rio checks nCcalls > baseCcalls; here we
    // use current_thread == None as the main-thread indicator (the main
    // thread's initial call always increments nCcalls in PUC-Rio, making
    // yield impossible there).
    if state.current_thread.is_none() || state.n_ccalls > 0 {
        return Err(simple_error(
            "attempt to yield across metamethod/C-call boundary".into(),
        ));
    }

    // The yielded values are the arguments to yield (already on the stack).
    let n = nargs(state) as u32;

    // Signal yield by returning a special error.
    // The resume handler will catch this and collect the yielded values.
    Err(LuaError::Yield(n))
}

// ---------------------------------------------------------------------------
// coroutine.wrap(f)
// ---------------------------------------------------------------------------

/// Creates a coroutine and returns a function that resumes it each time
/// it is called. Arguments to the function go to `resume`; results of
/// `yield` become results of the function. Errors propagate.
///
/// Reference: `luaB_cowrap` + `luaB_auxwrap` in `lbaselib.c`.
pub fn co_wrap(state: &mut LuaState) -> LuaResult<u32> {
    // Create the coroutine (reuse co_create logic).
    let func_val = arg(state, 0);

    // Validate: must be a Lua function.
    match func_val {
        Val::Function(r) => {
            let cl = state
                .gc
                .closures
                .get(r)
                .ok_or_else(|| simple_error("invalid function reference".into()))?;
            if matches!(cl, Closure::Rust(_)) {
                return Err(simple_error(
                    "bad argument #1 to 'wrap' (Lua function expected)".into(),
                ));
            }
        }
        _ => {
            return Err(simple_error(
                "bad argument #1 to 'wrap' (Lua function expected)".into(),
            ));
        }
    }

    let thread = LuaThread::new(func_val, state.global);
    let thread_ref = state.gc.alloc_thread(thread);

    // Create a Rust closure with the thread as upvalue[0].
    let mut wrapper = RustClosure::new(wrap_aux, "wrap_aux");
    wrapper.upvalues.push(Val::Thread(thread_ref));
    let closure_ref = state.gc.alloc_closure(Closure::Rust(wrapper));

    state.push(Val::Function(closure_ref));
    Ok(1)
}

/// The wrapped coroutine resume function (upvalue[0] = thread).
///
/// Reference: `luaB_auxwrap` in `lbaselib.c`.
fn wrap_aux(state: &mut LuaState) -> LuaResult<u32> {
    // Get the thread from upvalue[0].
    let ci_func = state.call_stack[state.ci].func;
    let func_val = state.stack_get(ci_func);
    let Val::Function(closure_ref) = func_val else {
        return Err(simple_error("invalid wrap closure".into()));
    };

    let co_ref = {
        let cl = state
            .gc
            .closures
            .get(closure_ref)
            .ok_or_else(|| simple_error("invalid closure reference".into()))?;
        match cl {
            Closure::Rust(rc) => {
                if let Some(Val::Thread(r)) = rc.upvalues.first() {
                    *r
                } else {
                    return Err(simple_error("wrap: missing thread upvalue".into()));
                }
            }
            _ => return Err(simple_error("wrap: expected Rust closure".into())),
        }
    };

    // Collect arguments.
    let n = nargs(state);
    let mut args = Vec::with_capacity(n);
    for i in 0..n {
        args.push(arg(state, i));
    }

    // Resume the coroutine.
    match auxresume(state, co_ref, &args) {
        Ok(results) => {
            // Push results directly (no true/false prefix).
            let base = state.base;
            for (i, val) in results.iter().enumerate() {
                state.stack_set(base + i, *val);
            }
            state.top = base + results.len();
            Ok(results.len() as u32)
        }
        Err(error_val) => {
            // Propagate error (unlike resume which returns false+error_val).
            // PUC-Rio luaB_auxwrap: if error is a string, prepend location.
            // Then lua_error(L) re-raises with the error value.
            let final_val = if let Val::Str(r) = error_val {
                // String error: prepend source location (luaL_where pattern).
                let where_prefix = execute::get_where(state, 1);
                if where_prefix.is_empty() {
                    error_val
                } else {
                    let original = state
                        .gc
                        .string_arena
                        .get(r)
                        .map(|s| String::from_utf8_lossy(s.data()).to_string())
                        .unwrap_or_default();
                    let full = format!("{where_prefix}{original}");
                    Val::Str(state.gc.intern_string(full.as_bytes()))
                }
            } else {
                // Non-string error: re-raise as-is.
                error_val
            };
            state.error_object = Some(final_val);
            let display = match final_val {
                Val::Str(r) => state
                    .gc
                    .string_arena
                    .get(r)
                    .map(|s| String::from_utf8_lossy(s.data()).to_string())
                    .unwrap_or_default(),
                _ => format!("{final_val}"),
            };
            Err(LuaError::Runtime(RuntimeError {
                message: display,
                level: 0,
                traceback: vec![],
            }))
        }
    }
}

// ---------------------------------------------------------------------------
// coroutine.status(co)
// ---------------------------------------------------------------------------

/// Returns the status of a coroutine as a string:
/// `"running"`, `"suspended"`, `"normal"`, or `"dead"`.
///
/// Reference: `luaB_costatus` in `lbaselib.c`.
pub fn co_status(state: &mut LuaState) -> LuaResult<u32> {
    let co_val = arg(state, 0);
    let Val::Thread(co_ref) = co_val else {
        return Err(simple_error(
            "bad argument #1 to 'status' (coroutine expected)".into(),
        ));
    };

    // Check if this coroutine is the currently running one.
    if state.current_thread == Some(co_ref) {
        let s = state.gc.intern_string(b"running");
        state.push(Val::Str(s));
        return Ok(1);
    }

    let status_str = match state.gc.threads.get(co_ref).map(|t| t.status) {
        Some(ThreadStatus::Running) => "running",
        Some(ThreadStatus::Initial) | Some(ThreadStatus::Suspended) => "suspended",
        Some(ThreadStatus::Normal) => "normal",
        Some(ThreadStatus::Dead) | None => "dead",
    };

    let s = state.gc.intern_string(status_str.as_bytes());
    state.push(Val::Str(s));
    Ok(1)
}

// ---------------------------------------------------------------------------
// coroutine.running()
// ---------------------------------------------------------------------------

/// Returns the running coroutine, or nil if called from the main thread.
///
/// Reference: `luaB_corunning` in `lbaselib.c`.
pub fn co_running(state: &mut LuaState) -> LuaResult<u32> {
    match state.current_thread {
        Some(co_ref) => {
            state.push(Val::Thread(co_ref));
            Ok(1)
        }
        None => {
            // Main thread: return nothing (nil).
            Ok(0)
        }
    }
}
