//! The `lua_*` functions of `lua.h` (Lua 5.1).

use std::ffi::{CStr, c_char, c_int, c_void};

use rilua::api::LuaApiMut;
use rilua::error::RuntimeError;
use rilua::vm::callinfo::LUA_MULTRET as RILUA_MULTRET;
use rilua::vm::closure::Closure;
use rilua::vm::gc::arena::GcRef;
use rilua::vm::metatable::{self, TMS};
use rilua::vm::state::{LuaState, LuaThread, MAXCALLS, ThreadStatus};
use rilua::vm::table::Table;
use rilua::vm::value::Userdata;
use rilua::{LuaError, LuaResult, Val};

use crate::global::{
    Global, Protect, check, error_status, error_value, frame, gref, index2val, new_cclosure,
    payload_to_error, pop, protected, push, rt_error, set_index, stack_pos, take_top, throw,
    top_val,
};
use crate::*;

// ---------------------------------------------------------------------------
// Full userdata memory block (what lua_newuserdata hands to C)
// ---------------------------------------------------------------------------

/// Raw, 16-byte aligned memory block owned by a full userdata.
pub struct CBlock {
    pub ptr: *mut u8,
    pub size: usize,
}

const UD_ALIGN: usize = 16;

impl CBlock {
    fn new(size: usize) -> Option<Self> {
        let layout = std::alloc::Layout::from_size_align(size.max(1), UD_ALIGN).ok()?;
        let ptr = unsafe { std::alloc::alloc_zeroed(layout) };
        if ptr.is_null() {
            return None;
        }
        Some(CBlock { ptr, size })
    }
}

impl Drop for CBlock {
    fn drop(&mut self) {
        if let Ok(layout) = std::alloc::Layout::from_size_align(self.size.max(1), UD_ALIGN) {
            unsafe { std::alloc::dealloc(self.ptr, layout) };
        }
    }
}

/// Pointer to the memory of a full userdata (C block or rilua-native data).
pub fn ud_ptr(g: &Global, r: GcRef<Userdata>) -> *mut c_void {
    match g.state.gc.userdata.get(r) {
        Some(ud) => match ud.downcast_ref::<CBlock>() {
            Some(b) => b.ptr.cast(),
            None => ud.data_ptr().cast_mut().cast(),
        },
        None => std::ptr::null_mut(),
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

#[inline]
unsafe fn val_or_nil(l: *mut lua_State, idx: c_int) -> Val {
    index2val(l, idx).unwrap_or(Val::Nil)
}

/// Converts a relative stack index to an absolute (positive) one.
pub(crate) unsafe fn absindex(l: *mut lua_State, idx: c_int) -> c_int {
    if idx < 0 && idx > LUA_REGISTRYINDEX {
        let (base, top) = frame(l);
        (top - base) as c_int + idx + 1
    } else {
        idx
    }
}

unsafe fn gc_check(l: *mut lua_State) {
    let g = gref(l);
    if g.state.gc.gc_state.total_bytes >= g.state.gc.gc_state.gc_threshold
        && !g.state.gc.gc_state.gc_running
    {
        check(l, g.state.gc_check());
    }
}

fn intern(g: &mut Global, bytes: &[u8]) -> Val {
    Val::Str(g.state.gc.intern_string(bytes))
}

fn metatable_of(g: &Global, v: Val) -> Option<GcRef<Table>> {
    match v {
        Val::Table(r) => g.state.gc.tables.get(r).and_then(Table::metatable),
        Val::Userdata(r) => g.state.gc.userdata.get(r).and_then(Userdata::metatable),
        _ => g.state.gc.type_metatables[metatable::type_tag(v)],
    }
}

fn get_tm(g: &Global, v: Val, event: TMS) -> Option<Val> {
    let gc = &g.state.gc;
    metatable::gettmbyobj(
        v,
        event,
        &gc.tables,
        &gc.string_arena,
        &gc.type_metatables,
        &gc.tm_names,
        &gc.userdata,
    )
}

/// Calls `f(args...)` on the running thread, returning its first result.
fn call_meta(state: &mut LuaState, f: Val, args: &[Val]) -> LuaResult<Val> {
    let saved_top = state.top;
    let base = state.top;
    state.ensure_stack(args.len() + 2);
    state.stack_set(base, f);
    for (i, a) in args.iter().enumerate() {
        state.stack_set(base + 1 + i, *a);
    }
    state.top = base + 1 + args.len();
    let r = state.call_function(base, 1);
    let res = state.stack_get(base);
    state.top = saved_top;
    r.map(|()| res)
}

/// Full `luaV_gettable`: `t[key]` with `__index` handling for any type.
pub(crate) fn gettable(g: &mut Global, t: Val, key: Val) -> LuaResult<Val> {
    let mut cur = t;
    for _ in 0..100 {
        let tm = if let Val::Table(tr) = cur {
            let gc = &g.state.gc;
            let Some(tbl) = gc.tables.get(tr) else {
                return Ok(Val::Nil);
            };
            let res = tbl.get(key, &gc.string_arena);
            if !res.is_nil() {
                return Ok(res);
            }
            match tbl.metatable() {
                Some(mt) => match metatable::fasttm(
                    &gc.tables,
                    &gc.string_arena,
                    mt,
                    TMS::Index,
                    &gc.tm_names,
                ) {
                    Some(tm) => tm,
                    None => return Ok(Val::Nil),
                },
                None => return Ok(Val::Nil),
            }
        } else {
            match get_tm(g, cur, TMS::Index) {
                Some(tm) => tm,
                None => {
                    return Err(rt_error(format!(
                        "attempt to index a {} value",
                        cur.type_name()
                    )));
                }
            }
        };
        if matches!(tm, Val::Function(_)) {
            return call_meta(&mut g.state, tm, &[cur, key]);
        }
        cur = tm;
    }
    Err(rt_error("loop in gettable"))
}

fn raw_table(l: *mut lua_State, v: Val) -> GcRef<Table> {
    match v {
        Val::Table(r) => r,
        _ => unsafe { throw(l, rt_error("table expected")) },
    }
}

/// Moves `n_in` values from a non-running thread onto the running stack,
/// runs `f` there (which must leave its results at the running top), and
/// moves the results back.
unsafe fn on_running(
    l: *mut lua_State,
    n_in: usize,
    f: impl FnOnce(&mut LuaState, usize) -> LuaResult<()>,
) {
    let vals = take_top(l, n_in);
    let g = gref(l);
    let s = &mut g.state;
    let start = s.top;
    for v in vals {
        s.push(v);
    }
    let r = f(s, start);
    let s = &mut gref(l).state;
    match r {
        Ok(()) => {
            let res: Vec<Val> = (start..s.top).map(|i| s.stack_get(i)).collect();
            s.top = start;
            for v in res {
                push(l, v);
            }
        }
        Err(e) => {
            s.top = start;
            throw(l, e)
        }
    }
}

// ---------------------------------------------------------------------------
// State manipulation
// ---------------------------------------------------------------------------

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_newstate(f: lua_Alloc, ud: *mut c_void) -> *mut lua_State {
    crate::sys::install_panic_hook();
    let g = Global::new(f, ud);
    (*g).main
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_close(l: *mut lua_State) {
    if l.is_null() {
        return;
    }
    let g = (*l).g;
    close_state(g);
    (*g).free_lstates();
    let libs = std::mem::take(&mut (*g).libs);
    drop(Box::from_raw(g));
    crate::sys::flush_all_c_streams();
    // Unload C libraries only after all objects (and their code) are gone.
    drop(libs);
}

/// Calls pending `__gc` finalizers for all userdata (PUC-Rio `luaC_callGCTM`
/// on close), newest first.
unsafe fn close_state(g: *mut Global) {
    let s = &mut (*g).state;
    s.gc.gc_state.gc_threshold = usize::MAX; // no collection while closing
    s.hook.allow_hook = false; // PUC-Rio GCTM: no debug hooks in finalizers
    let mut list: Vec<(u64, GcRef<Userdata>)> = s
        .gc
        .userdata
        .iter()
        .filter(|(_, ud, _)| !ud.finalized())
        .filter_map(|(r, ud, _)| {
            let mt = ud.metatable()?;
            metatable::fasttm(
                &s.gc.tables,
                &s.gc.string_arena,
                mt,
                TMS::Gc,
                &s.gc.tm_names,
            )?;
            Some((ud.alloc_seq(), r))
        })
        .collect();
    list.sort_by(|a, b| b.0.cmp(&a.0));
    for (_, r) in list {
        let s = &mut (*g).state;
        let Some(ud) = s.gc.userdata.get_mut(r) else { continue };
        if ud.finalized() {
            continue;
        }
        ud.set_finalized(true);
        let Some(mt) = ud.metatable() else { continue };
        let Some(tm) =
            metatable::fasttm(&s.gc.tables, &s.gc.string_arena, mt, TMS::Gc, &s.gc.tm_names)
        else {
            continue;
        };
        let top = s.top;
        let _ = protected(g, || call_meta(&mut (*g).state, tm, &[Val::Userdata(r)]));
        let s = &mut (*g).state;
        s.top = top;
        s.error_object = None;
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_newthread(l: *mut lua_State) -> *mut lua_State {
    let g = gref(l);
    let gt = g.thread_global((*l).thread);
    let mut th = LuaThread::new(Val::Nil, gt);
    // New threads inherit the creator's hook (PUC-Rio lua_newthread).
    if g.is_running((*l).thread) {
        th.hook = g.state.hook.clone();
    }
    let r = g.state.gc.alloc_thread(th);
    push(l, Val::Thread(r));
    gc_check(l);
    gref(l).lstate_for(Some(r))
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_atpanic(l: *mut lua_State, panicf: Option<lua_CFunction>) -> Option<lua_CFunction> {
    let g = gref(l);
    std::mem::replace(&mut g.panicf, panicf)
}

// ---------------------------------------------------------------------------
// Basic stack manipulation
// ---------------------------------------------------------------------------

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_gettop(l: *mut lua_State) -> c_int {
    let (base, top) = frame(l);
    top.saturating_sub(base) as c_int
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_settop(l: *mut lua_State, idx: c_int) {
    let t = (*l).thread;
    gref(l).with_mut(t, |p| {
        if idx >= 0 {
            let nt = *p.base + idx as usize;
            if nt > p.stack.len() {
                p.stack.resize(nt, Val::Nil);
            }
            for i in *p.top..nt {
                p.stack[i] = Val::Nil;
            }
            *p.top = nt;
        } else {
            let nt = (*p.top as isize + idx as isize + 1).max(*p.base as isize) as usize;
            *p.top = nt;
        }
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_pushvalue(l: *mut lua_State, idx: c_int) {
    let v = val_or_nil(l, idx);
    push(l, v);
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_remove(l: *mut lua_State, idx: c_int) {
    let t = (*l).thread;
    gref(l).with_mut(t, |p| {
        if let Some(pos) = stack_pos(*p.base, *p.top, idx) {
            p.stack.copy_within(pos + 1..*p.top, pos);
            *p.top -= 1;
        }
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_insert(l: *mut lua_State, idx: c_int) {
    let t = (*l).thread;
    gref(l).with_mut(t, |p| {
        if let Some(pos) = stack_pos(*p.base, *p.top, idx) {
            let v = p.stack[*p.top - 1];
            p.stack.copy_within(pos..*p.top - 1, pos + 1);
            p.stack[pos] = v;
        }
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_replace(l: *mut lua_State, idx: c_int) {
    let v = top_val(l, 1);
    set_index(l, idx, v);
    pop(l, 1);
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_checkstack(l: *mut lua_State, sz: c_int) -> c_int {
    let (base, top) = frame(l);
    if sz < 0 || (top - base) as c_int + sz > LUAI_MAXCSTACK {
        return 0;
    }
    let t = (*l).thread;
    gref(l).with_mut(t, |p| {
        let need = *p.top + sz as usize;
        if need > p.stack.len() {
            p.stack.resize(need, Val::Nil);
        }
    });
    let g = gref(l);
    if g.is_running(t) {
        let ci = g.state.ci;
        let need = g.state.top + sz as usize;
        if g.state.call_stack[ci].top < need {
            g.state.call_stack[ci].top = need;
        }
    }
    1
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_xmove(from: *mut lua_State, to: *mut lua_State, n: c_int) {
    if from == to || n <= 0 {
        return;
    }
    let vals = take_top(from, n as usize);
    for v in vals {
        push(to, v);
    }
}

// ---------------------------------------------------------------------------
// Access functions (stack -> C)
// ---------------------------------------------------------------------------

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_isnumber(l: *mut lua_State, idx: c_int) -> c_int {
    match index2val(l, idx) {
        Some(Val::Num(_)) => 1,
        Some(v @ Val::Str(_)) => {
            c_int::from(rilua::vm::execute::coerce_to_number(v, &gref(l).state.gc).is_some())
        }
        _ => 0,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_isstring(l: *mut lua_State, idx: c_int) -> c_int {
    c_int::from(matches!(index2val(l, idx), Some(Val::Str(_) | Val::Num(_))))
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_iscfunction(l: *mut lua_State, idx: c_int) -> c_int {
    match index2val(l, idx) {
        Some(Val::Function(r)) => {
            c_int::from(matches!(gref(l).state.gc.closures.get(r), Some(Closure::Rust(_))))
        }
        _ => 0,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_isuserdata(l: *mut lua_State, idx: c_int) -> c_int {
    c_int::from(matches!(
        index2val(l, idx),
        Some(Val::Userdata(_) | Val::LightUserdata(_))
    ))
}

pub(crate) fn type_code(v: Option<Val>) -> c_int {
    match v {
        None => LUA_TNONE,
        Some(Val::Nil) => LUA_TNIL,
        Some(Val::Bool(_)) => LUA_TBOOLEAN,
        Some(Val::LightUserdata(_)) => LUA_TLIGHTUSERDATA,
        Some(Val::Num(_)) => LUA_TNUMBER,
        Some(Val::Str(_)) => LUA_TSTRING,
        Some(Val::Table(_)) => LUA_TTABLE,
        Some(Val::Function(_)) => LUA_TFUNCTION,
        Some(Val::Userdata(_)) => LUA_TUSERDATA,
        Some(Val::Thread(_)) => LUA_TTHREAD,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_type(l: *mut lua_State, idx: c_int) -> c_int {
    type_code(index2val(l, idx))
}

static TYPE_NAMES: [&CStr; 11] = [
    c"nil",
    c"boolean",
    c"userdata",
    c"number",
    c"string",
    c"table",
    c"function",
    c"userdata",
    c"thread",
    c"proto",
    c"upval",
];

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_typename(_l: *mut lua_State, t: c_int) -> *const c_char {
    if t < 0 || t as usize >= TYPE_NAMES.len() {
        c"no value".as_ptr()
    } else {
        TYPE_NAMES[t as usize].as_ptr()
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_equal(l: *mut lua_State, idx1: c_int, idx2: c_int) -> c_int {
    let (Some(a), Some(b)) = (index2val(l, idx1), index2val(l, idx2)) else {
        return 0;
    };
    let r = gref(l).state.api_equal(a, b);
    c_int::from(check(l, r))
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_rawequal(l: *mut lua_State, idx1: c_int, idx2: c_int) -> c_int {
    let (Some(a), Some(b)) = (index2val(l, idx1), index2val(l, idx2)) else {
        return 0;
    };
    let gc = &gref(l).state.gc;
    c_int::from(metatable::val_raw_equal(a, b, &gc.tables, &gc.string_arena))
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_lessthan(l: *mut lua_State, idx1: c_int, idx2: c_int) -> c_int {
    let (Some(a), Some(b)) = (index2val(l, idx1), index2val(l, idx2)) else {
        return 0;
    };
    let r = gref(l).state.api_lessthan(a, b);
    c_int::from(check(l, r))
}

pub(crate) fn tonumber(g: &Global, v: Option<Val>) -> Option<f64> {
    match v {
        Some(Val::Num(n)) => Some(n),
        Some(v @ Val::Str(_)) => rilua::vm::execute::coerce_to_number(v, &g.state.gc),
        _ => None,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_tonumber(l: *mut lua_State, idx: c_int) -> lua_Number {
    tonumber(gref(l), index2val(l, idx)).unwrap_or(0.0)
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_tointeger(l: *mut lua_State, idx: c_int) -> lua_Integer {
    match tonumber(gref(l), index2val(l, idx)) {
        Some(n) => n as lua_Integer,
        None => 0,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_toboolean(l: *mut lua_State, idx: c_int) -> c_int {
    c_int::from(index2val(l, idx).is_some_and(Val::is_truthy))
}

/// Converts a number at `idx` to a string in place (like `luaV_tostring`).
unsafe fn number_to_string_in_place(l: *mut lua_State, idx: c_int, v: Val) -> Val {
    let g = gref(l);
    let s = format!("{v}");
    let sv = intern(g, s.as_bytes());
    set_index(l, idx, sv);
    sv
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_tolstring(
    l: *mut lua_State,
    idx: c_int,
    len: *mut usize,
) -> *const c_char {
    let v = match index2val(l, idx) {
        Some(v @ Val::Str(_)) => v,
        Some(v @ Val::Num(_)) => number_to_string_in_place(l, idx, v),
        _ => {
            if !len.is_null() {
                *len = 0;
            }
            return std::ptr::null();
        }
    };
    let Val::Str(r) = v else { unreachable!() };
    match gref(l).state.gc.string_arena.get(r) {
        Some(s) => {
            if !len.is_null() {
                *len = s.len();
            }
            s.as_c_ptr().cast()
        }
        None => {
            if !len.is_null() {
                *len = 0;
            }
            std::ptr::null()
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_objlen(l: *mut lua_State, idx: c_int) -> usize {
    let g = gref(l);
    match index2val(l, idx) {
        Some(Val::Str(r)) => g.state.gc.string_arena.get(r).map_or(0, |s| s.len()),
        Some(Val::Table(r)) => g
            .state
            .gc
            .tables
            .get(r)
            .map_or(0, |t| t.len(&g.state.gc.string_arena)),
        Some(Val::Userdata(r)) => g.state.gc.userdata.get(r).map_or(0, |u| {
            u.downcast_ref::<CBlock>().map_or(0, |b| b.size)
        }),
        Some(v @ Val::Num(_)) => {
            let Val::Str(r) = number_to_string_in_place(l, idx, v) else { return 0 };
            gref(l).state.gc.string_arena.get(r).map_or(0, |s| s.len())
        }
        _ => 0,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_tocfunction(l: *mut lua_State, idx: c_int) -> Option<lua_CFunction> {
    let v = index2val(l, idx)?;
    crate::global::cfunction_of(gref(l), v)
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_touserdata(l: *mut lua_State, idx: c_int) -> *mut c_void {
    match index2val(l, idx) {
        Some(Val::Userdata(r)) => ud_ptr(gref(l), r),
        Some(Val::LightUserdata(p)) => p as *mut c_void,
        _ => std::ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_tothread(l: *mut lua_State, idx: c_int) -> *mut lua_State {
    match index2val(l, idx) {
        Some(Val::Thread(r)) => gref(l).lstate_for(Some(r)),
        _ => std::ptr::null_mut(),
    }
}

fn pseudo_ptr(tag: usize, index: u32, generation: u32) -> *const c_void {
    #[cfg(target_pointer_width = "64")]
    let v = (tag << 56) | ((generation as usize & 0xff_ffff) << 32) | index as usize;
    #[cfg(not(target_pointer_width = "64"))]
    let v = (tag << 28) | (index as usize & 0x0fff_ffff);
    v as *const c_void
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_topointer(l: *mut lua_State, idx: c_int) -> *const c_void {
    match index2val(l, idx) {
        Some(Val::Table(r)) => pseudo_ptr(1, r.index(), r.generation()),
        Some(Val::Function(r)) => pseudo_ptr(2, r.index(), r.generation()),
        Some(Val::Thread(r)) => pseudo_ptr(3, r.index(), r.generation()),
        Some(Val::Userdata(r)) => ud_ptr(gref(l), r),
        Some(Val::LightUserdata(p)) => p as *const c_void,
        _ => std::ptr::null(),
    }
}

// ---------------------------------------------------------------------------
// Push functions (C -> stack)
// ---------------------------------------------------------------------------

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_pushnil(l: *mut lua_State) {
    push(l, Val::Nil);
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_pushnumber(l: *mut lua_State, n: lua_Number) {
    push(l, Val::Num(n));
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_pushinteger(l: *mut lua_State, n: lua_Integer) {
    push(l, Val::Num(n as f64));
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_pushlstring(l: *mut lua_State, s: *const c_char, len: usize) {
    let bytes: &[u8] = if len == 0 || s.is_null() {
        &[]
    } else {
        std::slice::from_raw_parts(s.cast(), len)
    };
    let v = intern(gref(l), bytes);
    push(l, v);
    gc_check(l);
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_pushstring(l: *mut lua_State, s: *const c_char) {
    if s.is_null() {
        push(l, Val::Nil);
    } else {
        let bytes = CStr::from_ptr(s).to_bytes();
        lua_pushlstring(l, s, bytes.len());
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_pushcclosure(l: *mut lua_State, f: lua_CFunction, n: c_int) {
    let ups = take_top(l, n.max(0) as usize);
    let g = gref(l);
    let env = g.current_env((*l).thread);
    let v = new_cclosure(g, f, ups, env);
    push(l, v);
    gc_check(l);
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_pushboolean(l: *mut lua_State, b: c_int) {
    push(l, Val::Bool(b != 0));
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_pushlightuserdata(l: *mut lua_State, p: *mut c_void) {
    push(l, Val::LightUserdata(p as usize));
}

/// Returns (creating it on first use) the object representing the main thread.
pub(crate) fn main_thread_obj(g: &mut Global) -> GcRef<LuaThread> {
    if let Some(r) = g.main_obj
        && g.state.gc.threads.is_valid(r)
    {
        return r;
    }
    let mut th = LuaThread::new(Val::Nil, g.state.global);
    th.status = ThreadStatus::Running;
    let r = g.state.gc.alloc_thread(th);
    // Anchor it in the registry so it is never collected.
    let key = Val::LightUserdata(std::ptr::from_mut(g) as usize);
    let reg = g.state.registry;
    if let Some(t) = g.state.gc.tables.get_mut(reg) {
        let _ = t.raw_set(key, Val::Thread(r), &g.state.gc.string_arena);
    }
    g.main_obj = Some(r);
    r
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_pushthread(l: *mut lua_State) -> c_int {
    let g = gref(l);
    match g.norm((*l).thread) {
        Some(r) => {
            push(l, Val::Thread(r));
            0
        }
        None => {
            let r = main_thread_obj(g);
            push(l, Val::Thread(r));
            1
        }
    }
}

// ---------------------------------------------------------------------------
// Get functions (Lua -> stack)
// ---------------------------------------------------------------------------

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_gettable(l: *mut lua_State, idx: c_int) {
    let t = val_or_nil(l, idx);
    let key = top_val(l, 1);
    let r = gettable(gref(l), t, key);
    let v = check(l, r);
    set_index(l, -1, v);
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_getfield(l: *mut lua_State, idx: c_int, k: *const c_char) {
    let idx = absindex(l, idx);
    let key = intern(gref(l), CStr::from_ptr(k).to_bytes());
    push(l, key);
    lua_gettable(l, idx);
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_rawget(l: *mut lua_State, idx: c_int) {
    let t = raw_table(l, val_or_nil(l, idx));
    let key = top_val(l, 1);
    let g = gref(l);
    let v = g
        .state
        .gc
        .tables
        .get(t)
        .map_or(Val::Nil, |tb| tb.get(key, &g.state.gc.string_arena));
    set_index(l, -1, v);
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_rawgeti(l: *mut lua_State, idx: c_int, n: c_int) {
    let t = raw_table(l, val_or_nil(l, idx));
    let g = gref(l);
    let v = g
        .state
        .gc
        .tables
        .get(t)
        .map_or(Val::Nil, |tb| tb.get_int(i64::from(n)));
    push(l, v);
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_createtable(l: *mut lua_State, narr: c_int, nrec: c_int) {
    let g = gref(l);
    let t = Table::with_sizes(narr.max(0) as usize, nrec.max(0) as usize);
    let r = g.state.gc.alloc_table(t);
    push(l, Val::Table(r));
    gc_check(l);
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_newuserdata(l: *mut lua_State, size: usize) -> *mut c_void {
    let Some(block) = CBlock::new(size) else {
        throw(l, LuaError::Memory);
    };
    let ptr = block.ptr;
    let g = gref(l);
    let env = g.current_env((*l).thread);
    let mut ud = Userdata::new(Box::new(block));
    ud.set_env(Some(env));
    let r = g.state.gc.alloc_userdata(ud);
    g.state.gc.gc_state.track_alloc(size);
    push(l, Val::Userdata(r));
    gc_check(l);
    ptr.cast()
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_getmetatable(l: *mut lua_State, idx: c_int) -> c_int {
    let Some(v) = index2val(l, idx) else { return 0 };
    match metatable_of(gref(l), v) {
        Some(mt) => {
            push(l, Val::Table(mt));
            1
        }
        None => 0,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_getfenv(l: *mut lua_State, idx: c_int) {
    let g = gref(l);
    let gt = g.thread_global((*l).thread);
    let v = match index2val(l, idx) {
        Some(Val::Function(r)) => match g.state.gc.closures.get(r) {
            Some(Closure::Lua(c)) => Val::Table(c.env),
            Some(Closure::Rust(c)) => Val::Table(c.env.unwrap_or(gt)),
            None => Val::Nil,
        },
        Some(Val::Userdata(r)) => g
            .state
            .gc
            .userdata
            .get(r)
            .map_or(Val::Nil, |u| Val::Table(u.env().unwrap_or(gt))),
        Some(Val::Thread(r)) => {
            let t = Some(r);
            Val::Table(g.thread_global(t))
        }
        _ => Val::Nil,
    };
    push(l, v);
}

// ---------------------------------------------------------------------------
// Set functions (stack -> Lua)
// ---------------------------------------------------------------------------

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_settable(l: *mut lua_State, idx: c_int) {
    let t = val_or_nil(l, idx);
    let k = top_val(l, 2);
    let v = top_val(l, 1);
    let r = gref(l).state.settable(t, k, v);
    check(l, r);
    pop(l, 2);
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_setfield(l: *mut lua_State, idx: c_int, k: *const c_char) {
    let idx = absindex(l, idx);
    let t = val_or_nil(l, idx);
    let key = intern(gref(l), CStr::from_ptr(k).to_bytes());
    push(l, key); // keep the key anchored while metamethods run
    let v = top_val(l, 2);
    let r = gref(l).state.settable(t, key, v);
    check(l, r);
    pop(l, 2);
}

unsafe fn rawset_val(l: *mut lua_State, t: GcRef<Table>, k: Val, v: Val) {
    let g = gref(l);
    let r = match g.state.gc.tables.get_mut(t) {
        Some(tb) => tb.raw_set(k, v, &g.state.gc.string_arena),
        None => Ok(()),
    };
    check(l, r);
    g.state.gc.barrier_back(t);
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_rawset(l: *mut lua_State, idx: c_int) {
    let t = raw_table(l, val_or_nil(l, idx));
    let k = top_val(l, 2);
    let v = top_val(l, 1);
    rawset_val(l, t, k, v);
    pop(l, 2);
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_rawseti(l: *mut lua_State, idx: c_int, n: c_int) {
    let t = raw_table(l, val_or_nil(l, idx));
    let v = top_val(l, 1);
    rawset_val(l, t, Val::Num(f64::from(n)), v);
    pop(l, 1);
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_setmetatable(l: *mut lua_State, idx: c_int) -> c_int {
    let obj = val_or_nil(l, idx);
    let mt = match top_val(l, 1) {
        Val::Table(r) => Some(r),
        _ => None,
    };
    let g = gref(l);
    match obj {
        Val::Table(r) => {
            if let Some(t) = g.state.gc.tables.get_mut(r) {
                t.set_metatable(mt);
            }
            g.state.gc.barrier_back(r);
        }
        Val::Userdata(r) => {
            if let Some(u) = g.state.gc.userdata.get_mut(r) {
                u.set_metatable(mt);
            }
            if let (Some(c), Some(m)) = (g.state.gc.userdata.color(r), mt) {
                g.state.gc.barrier_forward_val(c, Val::Table(m));
            }
        }
        other => {
            g.state.gc.type_metatables[metatable::type_tag(other)] = mt;
        }
    }
    pop(l, 1);
    1
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_setfenv(l: *mut lua_State, idx: c_int) -> c_int {
    let Val::Table(env) = top_val(l, 1) else {
        pop(l, 1);
        return 0;
    };
    let g = gref(l);
    let res = match index2val(l, idx) {
        Some(Val::Function(r)) => {
            match g.state.gc.closures.get_mut(r) {
                Some(Closure::Lua(c)) => c.env = env,
                Some(Closure::Rust(c)) => c.env = Some(env),
                None => {}
            }
            crate::global::barrier_closure(g, r, Val::Table(env));
            1
        }
        Some(Val::Userdata(r)) => {
            if let Some(u) = g.state.gc.userdata.get_mut(r) {
                u.set_env(Some(env));
            }
            if let Some(c) = g.state.gc.userdata.color(r) {
                g.state.gc.barrier_forward_val(c, Val::Table(env));
            }
            1
        }
        Some(Val::Thread(r)) => {
            g.with_mut(Some(r), |p| *p.global = env);
            1
        }
        _ => 0,
    };
    pop(l, 1);
    res
}

// ---------------------------------------------------------------------------
// `load' and `call' functions
// ---------------------------------------------------------------------------

/// Adjusts the running top after a call with `nres` fixed results.
fn fix_results(s: &mut LuaState, func_idx: usize, nres: c_int) {
    if nres >= 0 {
        let want = func_idx + nres as usize;
        if s.top < want {
            s.ensure_stack(want - s.top);
            for i in s.top..want {
                s.stack_set(i, Val::Nil);
            }
        }
        s.top = want;
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_call(l: *mut lua_State, nargs: c_int, nresults: c_int) {
    let g = gref(l);
    let nres = if nresults < 0 { RILUA_MULTRET } else { nresults };
    if g.is_running((*l).thread) {
        let func_idx = g.state.top - nargs as usize - 1;
        let r = g.state.call_function(func_idx, nres);
        check(l, r);
        fix_results(&mut gref(l).state, func_idx, nres);
    } else {
        on_running(l, nargs as usize + 1, |s, start| {
            s.call_function(start, nres)?;
            fix_results(s, start, nres);
            Ok(())
        });
    }
}

/// Core of `lua_pcall` on the running thread. The function and its
/// arguments are at `func_idx..top`.
unsafe fn pcall_running(g: *mut Global, func_idx: usize, nres: c_int, handler: Option<Val>) -> c_int {
    let s = &mut (*g).state;
    let saved_ci = s.ci;
    let saved_n_ccalls = s.n_ccalls;
    let saved_call_depth = s.call_depth;
    let saved_allow_hook = s.hook.allow_hook;
    s.error_object = None;

    let r = protected(g, || (*g).state.call_function(func_idx, nres));
    match r {
        Ok(()) => {
            fix_results(&mut (*g).state, func_idx, nres);
            0
        }
        Err(e) => {
            let s = &mut (*g).state;
            let mut status = error_status(&e);
            let mut ev = error_value(s, &e);
            drop(e);
            if let Some(h) = handler
                && status != LUA_ERRMEM
            {
                // Call the handler before unwinding the stack so that it can
                // inspect it (e.g. debug.traceback).
                let r = protected(g, || call_meta(&mut (*g).state, h, &[ev]));
                let s = &mut (*g).state;
                match r {
                    Ok(v) => ev = v,
                    Err(_) => {
                        s.error_object = None;
                        ev = Val::Str(s.gc.intern_string(b"error in error handling"));
                        status = LUA_ERRERR;
                    }
                }
            }
            let s = &mut (*g).state;
            s.ci = saved_ci;
            s.base = s.call_stack[s.ci].base;
            s.n_ccalls = saved_n_ccalls;
            s.call_depth = saved_call_depth;
            s.hook.allow_hook = saved_allow_hook;
            if s.ci < MAXCALLS {
                s.ci_overflow = false;
            }
            s.close_upvalues(func_idx);
            s.stack_set(func_idx, ev);
            s.top = func_idx + 1;
            status
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_pcall(
    l: *mut lua_State,
    nargs: c_int,
    nresults: c_int,
    errfunc: c_int,
) -> c_int {
    let gp = (*l).g;
    let nres = if nresults < 0 { RILUA_MULTRET } else { nresults };
    let handler = if errfunc == 0 {
        None
    } else {
        index2val(l, errfunc)
    };
    if (*gp).is_running((*l).thread) {
        let func_idx = (*gp).state.top - nargs as usize - 1;
        pcall_running(gp, func_idx, nres, handler)
    } else {
        let vals = take_top(l, nargs as usize + 1);
        let s = &mut (*gp).state;
        let start = s.top;
        for v in vals {
            s.push(v);
        }
        let status = pcall_running(gp, start, nres, handler);
        let s = &mut (*gp).state;
        let res: Vec<Val> = (start..s.top).map(|i| s.stack_get(i)).collect();
        s.top = start;
        for v in res {
            push(l, v);
        }
        status
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_cpcall(l: *mut lua_State, func: lua_CFunction, ud: *mut c_void) -> c_int {
    let g = gref(l);
    let env = g.current_env((*l).thread);
    let f = new_cclosure(g, func, Vec::new(), env);
    push(l, f);
    push(l, Val::LightUserdata(ud as usize));
    lua_pcall(l, 1, 0, 0)
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_load(
    l: *mut lua_State,
    reader: lua_Reader,
    data: *mut c_void,
    chunkname: *const c_char,
) -> c_int {
    let mut buf: Vec<u8> = Vec::new();
    loop {
        let mut sz: usize = 0;
        let p = reader(l, data, &mut sz);
        if p.is_null() || sz == 0 {
            break;
        }
        buf.extend_from_slice(std::slice::from_raw_parts(p.cast::<u8>(), sz));
    }
    let name = if chunkname.is_null() {
        "?".to_string()
    } else {
        String::from_utf8_lossy(CStr::from_ptr(chunkname).to_bytes()).into_owned()
    };
    let g = gref(l);
    let gt = g.thread_global((*l).thread);
    match g.state.load_bytes(&buf, &name) {
        Ok(f) => {
            let r = f.gc_ref();
            if let Some(Closure::Lua(c)) = g.state.gc.closures.get_mut(r) {
                c.env = gt;
            }
            push(l, Val::Function(r));
            0
        }
        Err(e) => {
            let status = if matches!(e, LuaError::Memory) {
                LUA_ERRMEM
            } else {
                LUA_ERRSYNTAX
            };
            let s = &mut gref(l).state;
            s.error_object = None;
            let v = error_value(s, &e);
            push(l, v);
            status
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_dump(l: *mut lua_State, writer: lua_Writer, data: *mut c_void) -> c_int {
    let g = gref(l);
    let bytes = match top_val(l, 1) {
        Val::Function(r) => match g.state.gc.closures.get(r) {
            Some(Closure::Lua(c)) => {
                rilua::vm::dump::dump(&c.proto, Some(&g.state.gc.string_arena), false)
            }
            _ => return 1,
        },
        _ => return 1,
    };
    writer(l, bytes.as_ptr().cast(), bytes.len(), data)
}

// ---------------------------------------------------------------------------
// Coroutine functions
// ---------------------------------------------------------------------------

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_yield(l: *mut lua_State, nresults: c_int) -> c_int {
    let g = gref(l);
    if g.state.current_thread.is_none() || !g.is_running((*l).thread) || g.state.n_ccalls > 0 {
        throw(l, rt_error("attempt to yield across metamethod/C-call boundary"));
    }
    let (base, top) = frame(l);
    let n = (nresults.max(0) as usize).min(top - base);
    g.pending_yield = Some(n as u32);
    -1
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_resume(l: *mut lua_State, narg: c_int) -> c_int {
    let gp = (*l).g;
    let g = &mut *gp;
    let co = g.norm((*l).thread);
    let status = co.and_then(|r| g.state.gc.threads.get(r).map(|t| t.status));
    let Some(co) = co.filter(|_| matches!(status, Some(ThreadStatus::Initial | ThreadStatus::Suspended)))
    else {
        let msg = intern(g, b"cannot resume non-suspended coroutine");
        push(l, msg);
        return LUA_ERRRUN;
    };
    let args = take_top(l, narg.max(0) as usize);
    if status == Some(ThreadStatus::Initial) {
        let f = take_top(l, 1);
        let f = f.first().copied().unwrap_or(Val::Nil);
        g.with_mut(Some(co), |p| {
            if p.stack.is_empty() {
                p.stack.resize(super::LUA_MINSTACK as usize * 2, Val::Nil);
            }
            p.stack[0] = f;
            *p.top = 1;
            *p.base = 1;
        });
    }
    let r = {
        let _p = Protect::new(gp);
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            rilua::stdlib::coroutine::auxresume(&mut (*gp).state, co, &args)
        }))
    };
    let g = &mut *gp;
    match r {
        Ok(Ok(results)) => {
            let now = g.state.gc.threads.get(co).map(|t| t.status);
            if now == Some(ThreadStatus::Suspended) {
                let n = results.len();
                g.with_mut(Some(co), |p| {
                    *p.base = p.top.saturating_sub(n);
                });
                LUA_YIELD
            } else {
                // Finished normally. As in PUC-Rio (status 0, ci == base_ci)
                // the thread can be reused: pushing a new function and
                // calling lua_resume again starts it afresh.
                if let Some(th) = g.state.gc.threads.get_mut(co) {
                    th.status = ThreadStatus::Initial;
                    th.ci = 0;
                    th.call_stack.truncate(1);
                    if let Some(c0) = th.call_stack.first_mut() {
                        *c0 = rilua::vm::callinfo::CallInfo::new(
                            0,
                            1,
                            1 + LUA_MINSTACK as usize,
                            RILUA_MULTRET,
                        );
                    }
                    if let Some(s0) = th.stack.first_mut() {
                        *s0 = Val::Nil;
                    }
                }
                g.with_mut(Some(co), |mut p| {
                    *p.base = 1;
                    *p.top = 1;
                    for v in results {
                        p.push(v);
                    }
                });
                0
            }
        }
        Ok(Err(ev)) => {
            g.with_mut(Some(co), |mut p| p.push(ev));
            LUA_ERRRUN
        }
        Err(payload) => {
            let e = payload_to_error(payload);
            let ev = error_value(&mut g.state, &e);
            g.with_mut(Some(co), |mut p| p.push(ev));
            LUA_ERRRUN
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_status(l: *mut lua_State) -> c_int {
    let g = gref(l);
    match g.norm((*l).thread) {
        Some(r) => match g.state.gc.threads.get(r).map(|t| t.status) {
            Some(ThreadStatus::Suspended) => LUA_YIELD,
            _ => 0,
        },
        None => 0,
    }
}

// ---------------------------------------------------------------------------
// Garbage-collection function
// ---------------------------------------------------------------------------

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_gc(l: *mut lua_State, what: c_int, data: c_int) -> c_int {
    use rilua::vm::gc::collector::{GCSTEPSIZE, GcPhase};
    let s = &mut gref(l).state;
    match what {
        LUA_GCSTOP => {
            s.gc.gc_state.gc_threshold = usize::MAX;
            0
        }
        LUA_GCRESTART => {
            s.gc.gc_state.gc_threshold = s.gc.gc_state.total_bytes;
            0
        }
        LUA_GCCOLLECT => {
            let r = s.full_gc();
            check(l, r);
            0
        }
        LUA_GCCOUNT => (s.gc.gc_state.total_bytes >> 10) as c_int,
        LUA_GCCOUNTB => (s.gc.gc_state.total_bytes & 0x3ff) as c_int,
        LUA_GCSTEP => {
            let simulated = (data.max(0) as usize) << 10;
            s.gc.gc_state.gc_threshold = s.gc.gc_state.total_bytes.saturating_sub(simulated);
            while s.gc.gc_state.gc_threshold <= s.gc.gc_state.total_bytes {
                let stepmul = i64::from(s.gc.gc_state.gc_stepmul);
                let budget = if stepmul == 0 {
                    i64::MAX / 2
                } else {
                    (GCSTEPSIZE as i64 / 100) * stepmul
                };
                s.gc.gc_state.gc_debt +=
                    s.gc.gc_state.total_bytes as i64 - s.gc.gc_state.gc_threshold as i64;
                let r = s.gc_step(budget);
                let completed = check(l, r);
                if completed {
                    break;
                }
            }
            c_int::from(gref(l).state.gc.gc_state.phase == GcPhase::Pause)
        }
        LUA_GCSETPAUSE => s.gc_set_pause(data.max(0) as u32) as c_int,
        LUA_GCSETSTEPMUL => s.gc_set_step_multiplier(data.max(0) as u32) as c_int,
        _ => -1,
    }
}

// ---------------------------------------------------------------------------
// Miscellaneous functions
// ---------------------------------------------------------------------------

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_error(l: *mut lua_State) -> c_int {
    let v = top_val(l, 1);
    let g = gref(l);
    let message = match v {
        Val::Str(r) => g
            .state
            .gc
            .string_arena
            .get(r)
            .map(|s| String::from_utf8_lossy(s.data()).into_owned())
            .unwrap_or_default(),
        Val::Nil => "nil".to_string(),
        other => format!("{other}"),
    };
    g.state.error_object = Some(v);
    throw(
        l,
        LuaError::Runtime(RuntimeError {
            message,
            level: 0,
            traceback: vec![],
        }),
    )
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_next(l: *mut lua_State, idx: c_int) -> c_int {
    let t = raw_table(l, val_or_nil(l, idx));
    let key = top_val(l, 1);
    let g = gref(l);
    let r = match g.state.gc.tables.get(t) {
        Some(tb) => tb.next(key, &g.state.gc.string_arena),
        None => Ok(None),
    };
    match check(l, r) {
        Some((k, v)) => {
            set_index(l, -1, k);
            push(l, v);
            1
        }
        None => {
            pop(l, 1);
            0
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_concat(l: *mut lua_State, n: c_int) {
    if n >= 2 {
        let g = gref(l);
        if g.is_running((*l).thread) {
            let r = g.state.api_concat(n as usize);
            check(l, r);
        } else {
            on_running(l, n as usize, |s, _| s.api_concat(n as usize));
        }
        gc_check(l);
    } else if n == 0 {
        let v = intern(gref(l), b"");
        push(l, v);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_getallocf(l: *mut lua_State, ud: *mut *mut c_void) -> lua_Alloc {
    let g = gref(l);
    if !ud.is_null() {
        *ud = g.allocud;
    }
    g.allocf
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_setallocf(l: *mut lua_State, f: lua_Alloc, ud: *mut c_void) {
    let g = gref(l);
    g.allocf = f;
    g.allocud = ud;
}

/// Lua 5.1 "hack" for the `coco` patch; a no-op here.
#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn lua_setlevel(_from: *mut lua_State, _to: *mut lua_State) {}
