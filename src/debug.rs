//! The debug interface of `lua.h`: `lua_getstack`, `lua_getinfo`,
//! `lua_getlocal`/`lua_setlocal`, `lua_getupvalue`/`lua_setupvalue` and
//! C hooks (`lua_sethook` & co).

use std::ffi::{CStr, c_char, c_int};

use rilua::LuaResult;
use rilua::vm::callinfo::CallInfo;
use rilua::vm::closure::{Closure, RustClosure};
use rilua::vm::gc::arena::GcRef;
use rilua::vm::state::{HookState, LuaState, LuaThread};
use rilua::vm::table::Table;
use rilua::vm::value::Val;

use crate::global::{
    Global, Loc, gref, index2val, pop, push, top_val,
};
use crate::*;

/// Name given to the rilua closures that wrap C hooks.
pub const HOOK_NAME: &str = "(C hook)";

// ---------------------------------------------------------------------------
// Stack levels
// ---------------------------------------------------------------------------

/// PUC-Rio `lua_getstack` level resolution. Returns the CallInfo index
/// (`>= 1`), `Some(0)` for a lost tail call, or `None` if there is no such
/// level.
fn resolve_level(
    call_stack: &[CallInfo],
    ci: usize,
    level: c_int,
    is_hook: impl Fn(usize) -> bool,
) -> Option<usize> {
    if level < 0 {
        return None;
    }
    // Frames of C hooks are invisible, as in PUC-Rio (where hooks run
    // without a CallInfo of their own).
    let mut ci_idx = ci;
    while ci_idx > 0 && is_hook(ci_idx) {
        ci_idx -= 1;
    }
    let mut lvl = i64::from(level);
    while lvl > 0 && ci_idx > 0 {
        lvl -= 1;
        if call_stack[ci_idx].is_lua {
            lvl -= i64::from(call_stack[ci_idx].tail_calls);
        }
        ci_idx -= 1;
        while ci_idx > 0 && is_hook(ci_idx) {
            ci_idx -= 1;
        }
    }
    if lvl == 0 && ci_idx > 0 {
        Some(ci_idx)
    } else if lvl < 0 {
        Some(0)
    } else {
        None
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_getstack(l: *mut lua_State, level: c_int, ar: *mut lua_Debug) -> c_int {
    let g = gref(l);
    let closures = &g.state.gc.closures;
    let found = g.with_ref((*l).thread, |p| {
        let is_hook = |i: usize| {
            matches!(
                p.stack.get(p.call_stack[i].func),
                Some(Val::Function(r)) if matches!(
                    closures.get(*r),
                    Some(Closure::Rust(c)) if c.name == HOOK_NAME
                )
            )
        };
        resolve_level(p.call_stack, p.ci, level, is_hook)
    });
    match found {
        Some(ci) => {
            (*ar).i_ci = ci as c_int;
            1
        }
        None => 0,
    }
}

// ---------------------------------------------------------------------------
// lua_getinfo
// ---------------------------------------------------------------------------

fn current_line(call_stack: &[CallInfo], ci: usize, proto_lines: &[u32]) -> c_int {
    let pc = call_stack[ci].saved_pc;
    if pc > 0 && pc <= proto_lines.len() {
        proto_lines[pc - 1] as c_int
    } else if let Some(&first) = proto_lines.first() {
        first as c_int
    } else {
        -1
    }
}

fn copy_short_src(ar: &mut lua_Debug, s: &str) {
    let bytes = s.as_bytes();
    let n = bytes.len().min(LUA_IDSIZE - 1);
    for (i, b) in bytes[..n].iter().enumerate() {
        ar.short_src[i] = *b as c_char;
    }
    ar.short_src[n] = 0;
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_getinfo(l: *mut lua_State, what: *const c_char, ar: *mut lua_Debug) -> c_int {
    let g = gref(l);
    let t = (*l).thread;
    let mut what = CStr::from_ptr(what).to_bytes();
    let ar = &mut *ar;

    // Resolve the function and (optionally) its CallInfo.
    let (func, ci): (Val, Option<usize>) = if what.first() == Some(&b'>') {
        what = &what[1..];
        let f = top_val(l, 1);
        pop(l, 1);
        (f, None)
    } else if ar.i_ci == 0 {
        (Val::Nil, Some(0)) // lost tail call
    } else {
        let ci = ar.i_ci as usize;
        let f = g.with_ref(t, |p| {
            p.call_stack
                .get(ci)
                .and_then(|c| p.stack.get(c.func).copied())
                .unwrap_or(Val::Nil)
        });
        (f, Some(ci))
    };
    let tail = ci == Some(0);

    let mut status = 1;
    for &opt in what {
        match opt {
            b'S' => {
                if tail {
                    ar.source = g.cstr(b"=(tail call)");
                    copy_short_src(ar, "(tail call)");
                    ar.linedefined = -1;
                    ar.lastlinedefined = -1;
                    ar.what = g.cstr(b"tail");
                    continue;
                }
                let info = match func {
                    Val::Function(r) => match g.state.gc.closures.get(r) {
                        Some(Closure::Lua(c)) => Some((
                            c.proto.source.clone(),
                            c.proto.line_defined as c_int,
                            c.proto.last_line_defined as c_int,
                        )),
                        _ => None,
                    },
                    _ => None,
                };
                match info {
                    Some((source, ld, lld)) => {
                        ar.source = g.cstr(source.as_bytes());
                        copy_short_src(ar, &rilua::error::chunkid(&source));
                        ar.linedefined = ld;
                        ar.lastlinedefined = lld;
                        ar.what = g.cstr(if ld == 0 { b"main" as &[u8] } else { b"Lua" });
                    }
                    None => {
                        ar.source = g.cstr(b"=[C]");
                        copy_short_src(ar, "[C]");
                        ar.linedefined = -1;
                        ar.lastlinedefined = -1;
                        ar.what = g.cstr(b"C");
                    }
                }
            }
            b'l' => {
                ar.currentline = -1;
                if let (Some(ci), Val::Function(r)) = (ci, func)
                    && ci > 0
                    && let Some(Closure::Lua(c)) = g.state.gc.closures.get(r)
                {
                    let lines = &c.proto.line_info;
                    ar.currentline = g.with_ref(t, |p| {
                        if ci < p.call_stack.len() {
                            current_line(p.call_stack, ci, lines)
                        } else {
                            -1
                        }
                    });
                }
            }
            b'u' => {
                ar.nups = match func {
                    Val::Function(r) => match g.state.gc.closures.get(r) {
                        Some(Closure::Lua(c)) => c.proto.num_upvalues as c_int,
                        Some(Closure::Rust(c)) => c.upvalues.len() as c_int,
                        None => 0,
                    },
                    _ => 0,
                };
            }
            b'n' => {
                let found = match ci {
                    Some(ci) if ci > 0 => {
                        let gc = &g.state.gc;
                        g.with_ref(t, |p| {
                            if ci < p.call_stack.len() {
                                rilua::vm::debug_info::getfuncname_raw(
                                    p.call_stack,
                                    p.stack,
                                    gc,
                                    ci,
                                    &gc.string_arena,
                                )
                            } else {
                                None
                            }
                        })
                    }
                    _ => None,
                };
                match found {
                    Some((kind, name)) => {
                        ar.namewhat = g.cstr(kind.as_bytes());
                        ar.name = g.cstr(name.as_bytes());
                    }
                    None => {
                        ar.namewhat = g.cstr(b"");
                        ar.name = std::ptr::null();
                    }
                }
            }
            b'f' => {
                push(l, if tail { Val::Nil } else { func });
            }
            b'L' => {
                let lines = match func {
                    Val::Function(r) => match g.state.gc.closures.get(r) {
                        Some(Closure::Lua(c)) => Some(c.proto.line_info.clone()),
                        _ => None,
                    },
                    _ => None,
                };
                match lines {
                    Some(lines) => {
                        let mut tb = Table::new();
                        for ln in lines {
                            let _ = tb.raw_set(
                                Val::Num(f64::from(ln)),
                                Val::Bool(true),
                                &g.state.gc.string_arena,
                            );
                        }
                        let r = g.state.gc.alloc_table(tb);
                        push(l, Val::Table(r));
                    }
                    None => push(l, Val::Nil),
                }
            }
            _ => status = 0,
        }
    }
    status
}

// ---------------------------------------------------------------------------
// Locals and upvalues
// ---------------------------------------------------------------------------

/// PUC-Rio `findlocal`: name and absolute stack slot of local `n` in `ci`.
fn find_local(g: &Global, t: Option<GcRef<LuaThread>>, ci: usize, n: c_int) -> Option<(String, usize)> {
    if n <= 0 || ci == 0 {
        return None;
    }
    let n = n as usize;
    g.with_ref(t, |p| {
        let c = p.call_stack.get(ci)?;
        let slot = c.base + n - 1;
        if let Some(Val::Function(r)) = p.stack.get(c.func).copied()
            && let Some(Closure::Lua(lcl)) = g.state.gc.closures.get(r)
        {
            let pc = c.saved_pc.saturating_sub(1);
            let mut k = n;
            for lv in &lcl.proto.local_vars {
                if lv.start_pc as usize <= pc && pc < lv.end_pc as usize {
                    k -= 1;
                    if k == 0 {
                        return Some((lv.name.clone(), slot));
                    }
                }
            }
        }
        let limit = if ci == p.ci {
            p.top
        } else {
            p.call_stack.get(ci + 1).map_or(p.top, |c| c.func)
        };
        (slot < limit).then(|| ("(*temporary)".to_string(), slot))
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_getlocal(l: *mut lua_State, ar: *const lua_Debug, n: c_int) -> *const c_char {
    let g = gref(l);
    let t = (*l).thread;
    let Some((name, slot)) = find_local(g, t, (*ar).i_ci as usize, n) else {
        return std::ptr::null();
    };
    let v = g.with_ref(t, |p| p.stack.get(slot).copied().unwrap_or(Val::Nil));
    push(l, v);
    g.cstr(name.as_bytes())
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_setlocal(l: *mut lua_State, ar: *const lua_Debug, n: c_int) -> *const c_char {
    let g = gref(l);
    let t = (*l).thread;
    let v = top_val(l, 1);
    let found = find_local(g, t, (*ar).i_ci as usize, n);
    if let Some((_, slot)) = &found {
        g.with_mut(t, |p| {
            if *slot < p.stack.len() {
                p.stack[*slot] = v;
            }
        });
    }
    pop(l, 1);
    match found {
        Some((name, _)) => g.cstr(name.as_bytes()),
        None => std::ptr::null(),
    }
}

enum Upv {
    Lua(GcRef<rilua::vm::closure::Upvalue>, String),
    C(Val),
}

fn find_upvalue(g: &Global, f: Val, n: c_int) -> Option<Upv> {
    let Val::Function(r) = f else { return None };
    if n <= 0 {
        return None;
    }
    let n = n as usize;
    match g.state.gc.closures.get(r)? {
        Closure::Lua(c) => {
            let uv = *c.upvalues.get(n - 1)?;
            let name = c.proto.upvalue_names.get(n - 1).cloned().unwrap_or_default();
            Some(Upv::Lua(uv, name))
        }
        Closure::Rust(c) => c.upvalues.get(n - 1).map(|v| Upv::C(*v)),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_getupvalue(l: *mut lua_State, funcindex: c_int, n: c_int) -> *const c_char {
    let g = gref(l);
    let f = index2val(l, funcindex).unwrap_or(Val::Nil);
    match find_upvalue(g, f, n) {
        Some(Upv::Lua(uv, name)) => {
            let v = g
                .state
                .gc
                .upvalues
                .get(uv)
                .map_or(Val::Nil, |u| u.get(&g.state.stack));
            push(l, v);
            g.cstr(name.as_bytes())
        }
        Some(Upv::C(v)) => {
            push(l, v);
            g.cstr(b"")
        }
        None => std::ptr::null(),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_setupvalue(l: *mut lua_State, funcindex: c_int, n: c_int) -> *const c_char {
    let g = gref(l);
    let f = index2val(l, funcindex).unwrap_or(Val::Nil);
    let v = top_val(l, 1);
    let res = match find_upvalue(g, f, n) {
        Some(Upv::Lua(uv, name)) => {
            let s = &mut g.state;
            if let Some(u) = s.gc.upvalues.get_mut(uv) {
                u.set(&mut s.stack, v);
            }
            if let Some(c) = s.gc.upvalues.color(uv) {
                s.gc.barrier_forward_val(c, v);
            }
            g.cstr(name.as_bytes())
        }
        Some(Upv::C(_)) => {
            if let Val::Function(r) = f {
                if let Some(Closure::Rust(c)) = g.state.gc.closures.get_mut(r) {
                    c.upvalues[n as usize - 1] = v;
                }
                crate::global::barrier_closure(g, r, v);
            }
            g.cstr(b"")
        }
        None => std::ptr::null(),
    };
    pop(l, 1);
    res
}

// ---------------------------------------------------------------------------
// Hooks
// ---------------------------------------------------------------------------

fn with_hook<R>(g: &mut Global, t: Option<GcRef<LuaThread>>, f: impl FnOnce(&mut HookState) -> R) -> Option<R> {
    match g.loc(t) {
        Loc::Running => Some(f(&mut g.state.hook)),
        Loc::Saved(i) => Some(f(&mut g.state.saved_threads[i].hook)),
        Loc::Arena(r) => g.state.gc.threads.get_mut(r).map(|th| f(&mut th.hook)),
    }
}

/// rilua hook function (`hook_func`) that forwards to a C `lua_Hook`.
fn hook_trampoline(state: &mut LuaState) -> LuaResult<u32> {
    let sp: *mut LuaState = state;
    unsafe {
        let s = &*sp;
        let fidx = s.call_stack[s.ci].func;
        let native = match s.stack_get(fidx) {
            Val::Function(r) => s
                .gc
                .closures
                .get(r)
                .and_then(Closure::as_rust)
                .map_or(0, |c| c.native),
            _ => 0,
        };
        if native == 0 {
            return Ok(0);
        }
        let hook: lua_Hook = std::mem::transmute::<usize, lua_Hook>(native);
        let event = match s.stack_get(s.base) {
            Val::Str(r) => s.gc.string_arena.get(r).map(|x| x.data().to_vec()).unwrap_or_default(),
            _ => Vec::new(),
        };
        let line = match s.stack_get(s.base + 1) {
            Val::Num(n) => n as c_int,
            _ => -1,
        };
        let code = match event.as_slice() {
            b"call" => LUA_HOOKCALL,
            b"return" => LUA_HOOKRET,
            b"line" => LUA_HOOKLINE,
            b"count" => LUA_HOOKCOUNT,
            b"tail return" => LUA_HOOKTAILRET,
            _ => LUA_HOOKCALL,
        };
        let mut ar: lua_Debug = std::mem::zeroed();
        ar.event = code;
        ar.currentline = line;
        ar.i_ci = s.ci.saturating_sub(1) as c_int;
        let g = Global::from_state(sp);
        let l = (*g).lstate_for((*sp).current_thread);
        match crate::global::call_hook(g, hook, l, &mut ar) {
            Ok(()) => {
                let s = &mut *sp;
                s.top = s.base;
                Ok(0)
            }
            Err(e) => Err(e),
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_sethook(l: *mut lua_State, func: Option<lua_Hook>, mask: c_int, count: c_int) -> c_int {
    let g = gref(l);
    let t = (*l).thread;
    let (hook_val, mask) = match func {
        Some(f) if mask != 0 => {
            let cl = Closure::Rust(RustClosure {
                func: hook_trampoline,
                upvalues: Vec::new(),
                name: HOOK_NAME.to_string(),
                env: None,
                native: f as usize,
            });
            (Val::Function(g.state.gc.alloc_closure(cl)), mask)
        }
        _ => (Val::Nil, 0),
    };
    with_hook(g, t, |h| {
        h.hook_func = hook_val;
        h.hook_mask = (mask & 0x0f) as u8;
        h.base_hook_count = count;
        h.hook_count = count;
    });
    1
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_gethook(l: *mut lua_State) -> Option<lua_Hook> {
    let g = gref(l);
    let hv = with_hook(g, (*l).thread, |h| h.hook_func)?;
    if let Val::Function(r) = hv
        && let Some(Closure::Rust(c)) = g.state.gc.closures.get(r)
        && c.name == HOOK_NAME
        && c.native != 0
    {
        return Some(std::mem::transmute::<usize, lua_Hook>(c.native));
    }
    None
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_gethookmask(l: *mut lua_State) -> c_int {
    let g = gref(l);
    with_hook(g, (*l).thread, |h| c_int::from(h.hook_mask)).unwrap_or(0)
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_gethookcount(l: *mut lua_State) -> c_int {
    let g = gref(l);
    with_hook(g, (*l).thread, |h| h.base_hook_count).unwrap_or(0)
}
