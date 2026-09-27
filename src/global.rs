//! Global state, `lua_State` handles, error propagation and trampolines.
//!
//! # Threads
//!
//! rilua uses a "swap model" for coroutines: exactly one thread is *running*
//! and its stack lives in `LuaState`; a thread that resumed another one is
//! parked on `LuaState::saved_threads`; every other coroutine keeps its stack
//! inside its `LuaThread` object in the GC arena. A `lua_State*` handed to C
//! code identifies a thread, and every API call first locates where that
//! thread's stack currently lives ([`Loc`]).
//!
//! # Errors
//!
//! `lua_error` (and any API function that raises) never returns: the error
//! is carried by a Rust unwind (`resume_unwind`) that travels through the C
//! frames of the caller (all entry points use the `C-unwind` ABI) up to the
//! nearest protected boundary: a C-function trampoline (which turns it back
//! into a rilua `Err`), `lua_pcall` or `lua_cpcall`. With no boundary active
//! the panic function is called and the process exits, as in PUC-Rio.

use std::any::Any;
use std::collections::HashMap;
use std::ffi::{c_char, c_int, c_void};
#[allow(unused_imports)]
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};

use rilua::error::RuntimeError;
use rilua::vm::callinfo::CallInfo;
use rilua::vm::closure::Closure;
use rilua::vm::gc::arena::GcRef;
use rilua::vm::state::{LuaState, LuaThread};
use rilua::vm::table::Table;
use rilua::{LuaError, LuaResult, Val};

use crate::{LUA_ENVIRONINDEX, LUA_GLOBALSINDEX, LUA_REGISTRYINDEX, lua_Alloc, lua_CFunction};

/// Opaque handle given to C code. One per Lua thread (main thread or
/// coroutine); the pointer is stable for the lifetime of the thread.
#[repr(C)]
pub struct lua_State {
    pub(crate) g: *mut Global,
    pub(crate) thread: Option<GcRef<LuaThread>>,
}

/// Where a thread's live stack currently resides.
#[derive(Clone, Copy, Debug)]
pub enum Loc {
    /// The thread is executing: its stack is in `LuaState` itself.
    Running,
    /// The thread resumed another coroutine and is parked on
    /// `LuaState::saved_threads[i]`.
    Saved(usize),
    /// The thread is suspended / not started / dead: its stack is in the arena.
    Arena(GcRef<LuaThread>),
}

/// Shared per-`lua_newstate` data. `state` MUST stay the first field: the
/// trampolines recover the `Global` from the `&mut LuaState` rilua gives them.
#[repr(C)]
pub struct Global {
    pub state: LuaState,
    pub main: *mut lua_State,
    /// Lazily created thread object standing for the main thread (rilua has
    /// no value for it) so that `lua_pushthread` works on the main thread.
    pub main_obj: Option<GcRef<LuaThread>>,
    lstates: HashMap<GcRef<LuaThread>, *mut lua_State>,
    purge_at: usize,
    pub panicf: Option<lua_CFunction>,
    pub allocf: lua_Alloc,
    pub allocud: *mut c_void,
    /// Set by `lua_yield`, consumed by the trampoline when the C function
    /// returns.
    pub pending_yield: Option<u32>,
    /// Number of active protected boundaries (trampolines / pcalls).
    pub protect: usize,
    /// 32-bit x86 only: innermost setjmp target (csrc/ljmp.c) and the error
    /// being carried to it by longjmp.
    pub cjmp: *mut c_void,
    pub pending_err: Option<LuaError>,
    cstrings: HashMap<Vec<u8>, Box<[u8]>>,
    /// Dynamic libraries opened by `package.loadlib` / `require`.
    pub libs: Vec<crate::sys::Library>,
}

impl Global {
    pub fn new(allocf: lua_Alloc, allocud: *mut c_void) -> *mut Global {
        let g = Box::into_raw(Box::new(Global {
            state: LuaState::new(),
            main: std::ptr::null_mut(),
            main_obj: None,
            lstates: HashMap::new(),
            purge_at: 64,
            panicf: None,
            allocf,
            allocud,
            pending_yield: None,
            protect: 0,
            cjmp: std::ptr::null_mut(),
            pending_err: None,
            cstrings: HashMap::new(),
            libs: Vec::new(),
        }));
        unsafe {
            (*g).main = Box::into_raw(Box::new(lua_State { g, thread: None }));
        }
        g
    }

    /// Recovers the `Global` that owns a `LuaState`.
    ///
    /// # Safety
    /// `state` must be the `state` field of a live `Global` (always true for
    /// states created by `lua_newstate`).
    #[inline]
    pub unsafe fn from_state(state: *mut LuaState) -> *mut Global {
        state.cast::<Global>()
    }

    /// Maps the main-thread stand-in object back to `None`.
    #[inline]
    pub fn norm(&self, t: Option<GcRef<LuaThread>>) -> Option<GcRef<LuaThread>> {
        if t.is_some() && t == self.main_obj { None } else { t }
    }

    /// Returns the (unique, stable) `lua_State*` for a thread.
    pub fn lstate_for(&mut self, t: Option<GcRef<LuaThread>>) -> *mut lua_State {
        let Some(r) = self.norm(t) else {
            return self.main;
        };
        if let Some(&p) = self.lstates.get(&r) {
            return p;
        }
        if self.lstates.len() >= self.purge_at {
            self.purge_lstates();
        }
        let gp: *mut Global = self;
        let p = Box::into_raw(Box::new(lua_State {
            g: gp,
            thread: Some(r),
        }));
        self.lstates.insert(r, p);
        p
    }

    /// Frees handles of threads that have been garbage collected.
    fn purge_lstates(&mut self) {
        let threads = &self.state.gc.threads;
        self.lstates.retain(|r, p| {
            let alive = threads.is_valid(*r);
            if !alive {
                unsafe { drop(Box::from_raw(*p)) };
            }
            alive
        });
        self.purge_at = (self.lstates.len() * 2).max(64);
    }

    pub fn free_lstates(&mut self) {
        for (_, p) in self.lstates.drain() {
            unsafe { drop(Box::from_raw(p)) };
        }
        if !self.main.is_null() {
            unsafe { drop(Box::from_raw(self.main)) };
            self.main = std::ptr::null_mut();
        }
    }

    /// Returns a NUL-terminated copy of `bytes` that lives as long as the
    /// state (used for debug-info strings handed to C).
    pub fn cstr(&mut self, bytes: &[u8]) -> *const c_char {
        if let Some(b) = self.cstrings.get(bytes) {
            return b.as_ptr().cast();
        }
        let mut v = Vec::with_capacity(bytes.len() + 1);
        v.extend_from_slice(bytes);
        v.push(0);
        let b = v.into_boxed_slice();
        let p = b.as_ptr().cast();
        self.cstrings.insert(bytes.to_vec(), b);
        p
    }

    /// Locates a thread's stack.
    pub fn loc(&self, t: Option<GcRef<LuaThread>>) -> Loc {
        let t = self.norm(t);
        if self.state.current_thread == t {
            return Loc::Running;
        }
        if let Some(i) = self.state.saved_threads.iter().rposition(|s| s.owner_ref == t) {
            return Loc::Saved(i);
        }
        match t {
            Some(r) => Loc::Arena(r),
            // Main thread neither running nor parked: cannot happen, fall
            // back to the live state.
            None => Loc::Running,
        }
    }

    #[inline]
    pub fn is_running(&self, t: Option<GcRef<LuaThread>>) -> bool {
        self.norm(t) == self.state.current_thread
    }

    /// Runs `f` with mutable access to a thread's stack fields.
    pub fn with_mut<R>(&mut self, t: Option<GcRef<LuaThread>>, f: impl FnOnce(Parts<'_>) -> R) -> R {
        let loc = self.loc(t);
        let s = &mut self.state;
        match loc {
            Loc::Running => f(Parts {
                stack: &mut s.stack,
                base: &mut s.base,
                top: &mut s.top,
                call_stack: &mut s.call_stack,
                ci: s.ci,
                global: &mut s.global,
            }),
            Loc::Saved(i) => {
                let th = &mut s.saved_threads[i];
                f(Parts {
                    stack: &mut th.stack,
                    base: &mut th.base,
                    top: &mut th.top,
                    call_stack: &mut th.call_stack,
                    ci: th.ci,
                    global: &mut th.global,
                })
            }
            Loc::Arena(r) => {
                let fallback_global = s.global;
                if let Some(th) = s.gc.threads.get_mut(r) {
                    f(Parts {
                        stack: &mut th.stack,
                        base: &mut th.base,
                        top: &mut th.top,
                        call_stack: &mut th.call_stack,
                        ci: th.ci,
                        global: &mut th.global,
                    })
                } else {
                    let (mut st, mut b, mut tp, mut cs, mut gl) =
                        (Vec::new(), 0, 0, Vec::new(), fallback_global);
                    f(Parts {
                        stack: &mut st,
                        base: &mut b,
                        top: &mut tp,
                        call_stack: &mut cs,
                        ci: 0,
                        global: &mut gl,
                    })
                }
            }
        }
    }

    /// Runs `f` with shared access to a thread's stack fields.
    pub fn with_ref<R>(&self, t: Option<GcRef<LuaThread>>, f: impl FnOnce(RefParts<'_>) -> R) -> R {
        let loc = self.loc(t);
        let s = &self.state;
        match loc {
            Loc::Running => f(RefParts {
                stack: &s.stack,
                base: s.base,
                top: s.top,
                call_stack: &s.call_stack,
                ci: s.ci,
                global: s.global,
            }),
            Loc::Saved(i) => {
                let th = &s.saved_threads[i];
                f(RefParts {
                    stack: &th.stack,
                    base: th.base,
                    top: th.top,
                    call_stack: &th.call_stack,
                    ci: th.ci,
                    global: th.global,
                })
            }
            Loc::Arena(r) => {
                if let Some(th) = s.gc.threads.get(r) {
                    f(RefParts {
                        stack: &th.stack,
                        base: th.base,
                        top: th.top,
                        call_stack: &th.call_stack,
                        ci: th.ci,
                        global: th.global,
                    })
                } else {
                    f(RefParts {
                        stack: &[],
                        base: 0,
                        top: 0,
                        call_stack: &[],
                        ci: 0,
                        global: s.global,
                    })
                }
            }
        }
    }

    /// The globals table of a thread.
    pub fn thread_global(&self, t: Option<GcRef<LuaThread>>) -> GcRef<Table> {
        self.with_ref(t, |p| p.global)
    }

    /// The function running in the thread's current frame (Nil at top level).
    pub fn current_func(&self, t: Option<GcRef<LuaThread>>) -> Val {
        self.with_ref(t, |p| {
            if p.ci == 0 || p.ci >= p.call_stack.len() {
                return Val::Nil;
            }
            let fidx = p.call_stack[p.ci].func;
            p.stack.get(fidx).copied().unwrap_or(Val::Nil)
        })
    }

    /// PUC-Rio `getcurrenv`: the environment new functions/userdata inherit.
    pub fn current_env(&self, t: Option<GcRef<LuaThread>>) -> GcRef<Table> {
        let gt = self.thread_global(t);
        match self.current_func(t) {
            Val::Function(r) => match self.state.gc.closures.get(r) {
                Some(Closure::Rust(c)) => c.env.unwrap_or(gt),
                Some(Closure::Lua(l)) => l.env,
                None => gt,
            },
            _ => gt,
        }
    }
}

/// Mutable view of a thread's stack.
pub struct Parts<'a> {
    pub stack: &'a mut Vec<Val>,
    pub base: &'a mut usize,
    pub top: &'a mut usize,
    pub call_stack: &'a mut Vec<CallInfo>,
    pub ci: usize,
    pub global: &'a mut GcRef<Table>,
}

impl Parts<'_> {
    #[inline]
    pub fn push(&mut self, v: Val) {
        let t = *self.top;
        if t >= self.stack.len() {
            self.stack.resize(t + 1, Val::Nil);
        }
        self.stack[t] = v;
        *self.top = t + 1;
    }
}

/// Shared view of a thread's stack.
pub struct RefParts<'a> {
    pub stack: &'a [Val],
    pub base: usize,
    pub top: usize,
    pub call_stack: &'a [CallInfo],
    pub ci: usize,
    pub global: GcRef<Table>,
}

/// Converts a (non pseudo) API index to an absolute stack slot.
#[inline]
pub fn stack_pos(base: usize, top: usize, idx: c_int) -> Option<usize> {
    if idx > 0 {
        let p = base + (idx as usize) - 1;
        (p < top).then_some(p)
    } else if idx < 0 && idx > LUA_REGISTRYINDEX {
        let p = top as isize + idx as isize;
        (p >= base as isize).then_some(p as usize)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Accessors used by the API layer
// ---------------------------------------------------------------------------

#[inline]
pub unsafe fn gref<'a>(l: *mut lua_State) -> &'a mut Global {
    &mut *(*l).g
}

/// Reads the value at an acceptable index (`None` = no value).
pub unsafe fn index2val(l: *mut lua_State, idx: c_int) -> Option<Val> {
    let g = gref(l);
    let t = (*l).thread;
    if idx > LUA_REGISTRYINDEX {
        return g.with_ref(t, |p| stack_pos(p.base, p.top, idx).map(|i| p.stack[i]));
    }
    match idx {
        LUA_REGISTRYINDEX => Some(Val::Table(g.state.registry)),
        LUA_GLOBALSINDEX => Some(Val::Table(g.thread_global(t))),
        LUA_ENVIRONINDEX => Some(Val::Table(g.current_env(t))),
        _ => {
            let n = (LUA_GLOBALSINDEX - idx) as usize; // 1-based upvalue index
            match g.current_func(t) {
                Val::Function(r) => match g.state.gc.closures.get(r) {
                    Some(Closure::Rust(c)) => c.upvalues.get(n - 1).copied(),
                    _ => None,
                },
                _ => None,
            }
        }
    }
}

/// Writes a value at a valid index (stack slot or pseudo-index).
pub unsafe fn set_index(l: *mut lua_State, idx: c_int, v: Val) {
    let g = gref(l);
    let t = (*l).thread;
    if idx > LUA_REGISTRYINDEX {
        g.with_mut(t, |p| {
            if let Some(i) = stack_pos(*p.base, *p.top, idx) {
                p.stack[i] = v;
            }
        });
        return;
    }
    match idx {
        LUA_REGISTRYINDEX => {}
        LUA_GLOBALSINDEX => {
            if let Val::Table(tr) = v {
                g.with_mut(t, |p| *p.global = tr);
            }
        }
        LUA_ENVIRONINDEX => {
            let f = g.current_func(t);
            let Val::Function(r) = f else {
                throw(l, rt_error("no calling environment"));
            };
            let Val::Table(tr) = v else { return };
            match g.state.gc.closures.get_mut(r) {
                Some(Closure::Rust(c)) => c.env = Some(tr),
                _ => throw(l, rt_error("no calling environment")),
            }
            barrier_closure(g, r, v);
        }
        _ => {
            let n = (LUA_GLOBALSINDEX - idx) as usize;
            if let Val::Function(r) = g.current_func(t) {
                if let Some(Closure::Rust(c)) = g.state.gc.closures.get_mut(r)
                    && let Some(slot) = c.upvalues.get_mut(n - 1)
                {
                    *slot = v;
                }
                barrier_closure(g, r, v);
            }
        }
    }
}

/// Forward write barrier for a value stored into a closure.
pub fn barrier_closure(g: &mut Global, r: GcRef<Closure>, v: Val) {
    if let Some(c) = g.state.gc.closures.color(r) {
        g.state.gc.barrier_forward_val(c, v);
    }
}

/// Pushes a value on a thread's stack.
#[inline]
pub unsafe fn push(l: *mut lua_State, v: Val) {
    let t = (*l).thread;
    gref(l).with_mut(t, |mut p| p.push(v));
}

/// Pops `n` values.
#[inline]
pub unsafe fn pop(l: *mut lua_State, n: usize) {
    let t = (*l).thread;
    gref(l).with_mut(t, |p| {
        *p.top = p.top.saturating_sub(n).max(*p.base);
    });
}

/// Returns `(base, top)` of a thread's current frame.
#[inline]
pub unsafe fn frame(l: *mut lua_State) -> (usize, usize) {
    let t = (*l).thread;
    gref(l).with_ref(t, |p| (p.base, p.top))
}

/// Value `n` slots below the top (`n = 1` is the top value).
#[inline]
pub unsafe fn top_val(l: *mut lua_State, n: usize) -> Val {
    let t = (*l).thread;
    gref(l).with_ref(t, |p| {
        if p.top >= n && p.top - n >= p.base {
            p.stack[p.top - n]
        } else {
            Val::Nil
        }
    })
}

/// Pops the top `n` values of a thread into a vector (bottom first).
pub unsafe fn take_top(l: *mut lua_State, n: usize) -> Vec<Val> {
    let t = (*l).thread;
    gref(l).with_mut(t, |p| {
        let n = n.min(p.top.saturating_sub(*p.base));
        let start = *p.top - n;
        let v = p.stack[start..*p.top].to_vec();
        *p.top = start;
        v
    })
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Unwind payload carrying a Lua error through C frames.
pub struct LuaThrow(pub LuaError);

pub fn rt_error(msg: impl Into<String>) -> LuaError {
    LuaError::Runtime(RuntimeError {
        message: msg.into(),
        level: 0,
        traceback: vec![],
    })
}

/// Converts a caught unwind payload back into a `LuaError`.
pub fn payload_to_error(p: Box<dyn Any + Send>) -> LuaError {
    match p.downcast::<LuaThrow>() {
        Ok(t) => t.0,
        Err(p) => {
            let msg = if let Some(s) = p.downcast_ref::<&str>() {
                (*s).to_string()
            } else if let Some(s) = p.downcast_ref::<String>() {
                s.clone()
            } else {
                "unknown panic".to_string()
            };
            rt_error(format!("internal error (Rust panic): {msg}"))
        }
    }
}

/// Produces the Lua error object for an error (the thrown value if one was
/// recorded, otherwise the message as a string).
pub fn error_value(state: &mut LuaState, err: &LuaError) -> Val {
    if let Some(v) = state.error_object.take() {
        return v;
    }
    let bytes = match err {
        LuaError::Syntax(e) => e.to_lua_bytes(),
        other => other.to_string().into_bytes(),
    };
    Val::Str(state.gc.intern_string(&bytes))
}

/// Maps an error to a `LUA_ERR*` status code.
pub fn error_status(err: &LuaError) -> c_int {
    match err {
        LuaError::Syntax(_) => crate::LUA_ERRSYNTAX,
        LuaError::Memory => crate::LUA_ERRMEM,
        LuaError::ErrorHandler => crate::LUA_ERRERR,
        _ => crate::LUA_ERRRUN,
    }
}

/// Raises a Lua error from an API function. Never returns.
pub unsafe fn throw(l: *mut lua_State, err: LuaError) -> ! {
    let g = (*l).g;
    // On x86 an error is protected iff some C call has a jump target set.
    let unprotected = if cfg!(target_arch = "x86") { (*g).cjmp.is_null() } else { (*g).protect == 0 };
    if unprotected {
        // Unprotected error: call the panic function, then exit (PUC-Rio
        // luaD_throw behaviour).
        let v = error_value(&mut (*g).state, &err);
        drop(err);
        let msg = match v {
            Val::Str(r) => (*g)
                .state
                .gc
                .string_arena
                .get(r)
                .map(|s| String::from_utf8_lossy(s.data()).into_owned())
                .unwrap_or_default(),
            other => format!("{other}"),
        };
        crate::sys::debug_out(&format!(
            "lua5.1 (rilua): UNPROTECTED ERROR (no lua_pcall active): {msg} -- {}",
            if (*g).panicf.is_some() {
                "calling lua_atpanic handler, then exit(1)"
            } else {
                "no panic handler, exit(1)"
            }
        ));
        let rl = (*g).lstate_for((*g).state.current_thread);
        if let Some(pf) = (*g).panicf {
            push(rl, v);
            pf(rl);
        }
        std::process::exit(1);
    }
    #[cfg(target_arch = "x86")]
    {
        (*g).pending_err = Some(err);
        ljmp::lua51rs_throw(&mut (*g).cjmp)
    }
    #[cfg(not(target_arch = "x86"))]
    resume_unwind(Box::new(LuaThrow(err)))
}

/// 32-bit x86 error transport (csrc/ljmp.c).
#[cfg(target_arch = "x86")]
pub mod ljmp {
    use crate::{lua_CFunction, lua_Debug, lua_Hook, lua_State};
    use std::ffi::{c_int, c_void};
    unsafe extern "C-unwind" {
        pub fn lua51rs_ccall(f: lua_CFunction, l: *mut lua_State, chain: *mut *mut c_void, res: *mut c_int) -> c_int;
        pub fn lua51rs_hcall(h: lua_Hook, l: *mut lua_State, ar: *mut lua_Debug, chain: *mut *mut c_void) -> c_int;
        pub fn lua51rs_throw(chain: *mut *mut c_void) -> !;
    }
}

/// Calls a C function at a protected boundary. `Err` carries a Lua error
/// raised inside it (a Rust unwind, or on x86 a longjmp).
pub unsafe fn call_c(g: *mut Global, f: lua_CFunction, l: *mut lua_State) -> Result<c_int, LuaError> {
    let _p = Protect::new(g);
    #[cfg(target_arch = "x86")]
    {
        let mut res: c_int = 0;
        if ljmp::lua51rs_ccall(f, l, &mut (*g).cjmp, &mut res) == 0 {
            Ok(res)
        } else {
            Err((*g).pending_err.take().unwrap_or_else(|| rt_error("error")))
        }
    }
    #[cfg(not(target_arch = "x86"))]
    catch_unwind(AssertUnwindSafe(|| f(l))).map_err(payload_to_error)
}

/// Calls a C hook at a protected boundary.
pub unsafe fn call_hook(g: *mut Global, h: crate::lua_Hook, l: *mut lua_State, ar: *mut crate::lua_Debug) -> Result<(), LuaError> {
    let _p = Protect::new(g);
    #[cfg(target_arch = "x86")]
    {
        if ljmp::lua51rs_hcall(h, l, ar, &mut (*g).cjmp) == 0 {
            Ok(())
        } else {
            Err((*g).pending_err.take().unwrap_or_else(|| rt_error("error")))
        }
    }
    #[cfg(not(target_arch = "x86"))]
    catch_unwind(AssertUnwindSafe(|| h(l, ar))).map_err(payload_to_error)
}

/// Unwraps a rilua result, raising the error through the C API.
#[inline]
pub unsafe fn check<T>(l: *mut lua_State, r: LuaResult<T>) -> T {
    match r {
        Ok(v) => v,
        Err(e) => throw(l, e),
    }
}

/// RAII guard counting active protected boundaries.
pub struct Protect(*mut Global);

impl Protect {
    pub unsafe fn new(g: *mut Global) -> Self {
        (*g).protect += 1;
        Protect(g)
    }
}

impl Drop for Protect {
    fn drop(&mut self) {
        unsafe { (*self.0).protect -= 1 };
    }
}

/// Runs `f` with errors caught (both unwinds and `Err` results).
pub unsafe fn protected<T>(g: *mut Global, f: impl FnOnce() -> LuaResult<T>) -> LuaResult<T> {
    let _p = Protect::new(g);
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(r) => r,
        Err(p) => Err(payload_to_error(p)),
    }
}

// ---------------------------------------------------------------------------
// Trampolines: rilua -> C
// ---------------------------------------------------------------------------

/// The `RustFn` behind every C function / C closure. Looks up the
/// `lua_CFunction` stored in the closure's `native` field and calls it.
pub fn c_trampoline(state: &mut LuaState) -> LuaResult<u32> {
    let sp: *mut LuaState = state;
    unsafe {
        let native = {
            let s = &*sp;
            let fidx = s.call_stack[s.ci].func;
            match s.stack_get(fidx) {
                Val::Function(r) => s
                    .gc
                    .closures
                    .get(r)
                    .and_then(Closure::as_rust)
                    .map_or(0, |c| c.native),
                _ => 0,
            }
        };
        if native == 0 {
            return Err(rt_error("attempt to call an invalid C function"));
        }
        let f: lua_CFunction = std::mem::transmute::<usize, lua_CFunction>(native);
        let g = Global::from_state(sp);
        let l = (*g).lstate_for((*sp).current_thread);
        let r = call_c(g, f, l);
        match r {
            Ok(n) if n >= 0 => {
                let s = &*sp;
                let avail = s.top.saturating_sub(s.base);
                Ok((n as usize).min(avail) as u32)
            }
            Ok(_) => match (*g).pending_yield.take() {
                Some(n) => Err(LuaError::Yield(n)),
                None => Ok(0),
            },
            Err(e) => {
                (*g).pending_yield = None;
                Err(e)
            }
        }
    }
}

/// Creates a C closure value from a `lua_CFunction` and upvalues.
pub fn new_cclosure(g: &mut Global, f: lua_CFunction, upvalues: Vec<Val>, env: GcRef<Table>) -> Val {
    let cl = Closure::Rust(rilua::vm::closure::RustClosure {
        func: c_trampoline,
        upvalues,
        name: String::new(),
        env: Some(env),
        native: f as usize,
    });
    Val::Function(g.state.gc.alloc_closure(cl))
}

/// Returns the `lua_CFunction` of a C closure, if the value is one.
pub fn cfunction_of(g: &Global, v: Val) -> Option<lua_CFunction> {
    if let Val::Function(r) = v
        && let Some(Closure::Rust(c)) = g.state.gc.closures.get(r)
        && c.native != 0
        && c.name != crate::debug::HOOK_NAME
    {
        return Some(unsafe { std::mem::transmute::<usize, lua_CFunction>(c.native) });
    }
    None
}
