//! `lualib.h`: `luaopen_*`, `luaL_openlibs`, plus C-module support for
//! `require` and `package.loadlib` (rilua itself cannot load C modules).

use std::ffi::{CStr, c_int};

use rilua::stdlib::StdLib;
use rilua::vm::closure::{Closure, RustClosure};
use rilua::vm::gc::arena::GcRef;
use rilua::vm::state::LuaState;
use rilua::vm::table::Table;
use rilua::{LuaResult, Val};

use crate::global::{Global, check, gref, new_cclosure, push, rt_error};
use crate::*;

// ---------------------------------------------------------------------------
// Small table helpers on the raw rilua state
// ---------------------------------------------------------------------------

fn tget(s: &mut LuaState, t: GcRef<Table>, k: &str) -> Val {
    let key = s.gc.intern_string(k.as_bytes());
    s.gc.tables
        .get(t)
        .map_or(Val::Nil, |tb| tb.get_str(key, &s.gc.string_arena))
}

fn tset(s: &mut LuaState, t: GcRef<Table>, k: &str, v: Val) {
    let key = s.gc.intern_string(k.as_bytes());
    if let Some(tb) = s.gc.tables.get_mut(t) {
        let _ = tb.raw_set(Val::Str(key), v, &s.gc.string_arena);
    }
    s.gc.barrier_back(t);
}

fn global_table(s: &mut LuaState, name: &str) -> Val {
    let gt = s.global;
    tget(s, gt, name)
}

/// Records `t` as `package.loaded[name]` (registry `_LOADED`), like
/// `luaL_register` does.
fn set_loaded(s: &mut LuaState, name: &str, v: Val) {
    let reg = s.registry;
    if let Val::Table(loaded) = tget(s, reg, "_LOADED") {
        tset(s, loaded, name, v);
    }
}

fn str_of(s: &LuaState, v: Val) -> Option<String> {
    match v {
        Val::Str(r) => s
            .gc
            .string_arena
            .get(r)
            .map(|x| String::from_utf8_lossy(x.data()).into_owned()),
        Val::Num(_) => Some(format!("{v}")),
        _ => None,
    }
}

unsafe fn open_lib(l: *mut lua_State, flag: StdLib, name: &str) -> c_int {
    let g = gref(l);
    let r = rilua::stdlib::open_libs_selective(&mut g.state, flag);
    check(l, r);
    let s = &mut gref(l).state;
    let t = global_table(s, name);
    set_loaded(s, name, t);
    push(l, t);
    1
}

// ---------------------------------------------------------------------------
// luaopen_*
// ---------------------------------------------------------------------------

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn luaopen_base(l: *mut lua_State) -> c_int {
    let g = gref(l);
    let r = rilua::stdlib::open_libs_selective(&mut g.state, StdLib::BASE | StdLib::COROUTINE);
    check(l, r);
    let s = &mut gref(l).state;
    let gt = Val::Table(s.global);
    let co = global_table(s, "coroutine");
    set_loaded(s, "_G", gt);
    set_loaded(s, "coroutine", co);
    push(l, gt);
    push(l, co);
    2
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn luaopen_table(l: *mut lua_State) -> c_int {
    open_lib(l, StdLib::TABLE, "table")
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn luaopen_io(l: *mut lua_State) -> c_int {
    open_lib(l, StdLib::IO, "io")
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn luaopen_os(l: *mut lua_State) -> c_int {
    open_lib(l, StdLib::OS, "os")
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn luaopen_string(l: *mut lua_State) -> c_int {
    open_lib(l, StdLib::STRING, "string")
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn luaopen_math(l: *mut lua_State) -> c_int {
    open_lib(l, StdLib::MATH, "math")
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn luaopen_debug(l: *mut lua_State) -> c_int {
    open_lib(l, StdLib::DEBUG, "debug")
}

#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn luaopen_package(l: *mut lua_State) -> c_int {
    let g = gref(l);
    let r = rilua::stdlib::open_libs_selective(&mut g.state, StdLib::PACKAGE);
    check(l, r);
    let s = &mut gref(l).state;
    let Val::Table(pkg) = global_table(s, "package") else {
        push(l, Val::Nil);
        return 1;
    };
    install_c_loaders(s, pkg);
    #[cfg(windows)]
    windows_paths(s, pkg);
    set_loaded(s, "package", Val::Table(pkg));
    push(l, Val::Table(pkg));
    1
}

/// `luaL_openlibs` (linit.c).
#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn luaL_openlibs(l: *mut lua_State) {
    let libs: [(&CStr, lua_CFunction); 8] = [
        (c"", luaopen_base),
        (c"package", luaopen_package),
        (c"table", luaopen_table),
        (c"io", luaopen_io),
        (c"os", luaopen_os),
        (c"string", luaopen_string),
        (c"math", luaopen_math),
        (c"debug", luaopen_debug),
    ];
    for (name, f) in libs {
        api::lua_pushcclosure(l, f, 0);
        api::lua_pushstring(l, name.as_ptr());
        api::lua_call(l, 1, 0);
    }
}

// ---------------------------------------------------------------------------
// C modules
// ---------------------------------------------------------------------------

#[cfg(windows)]
const DIRSEP: &str = "\\";
#[cfg(not(windows))]
const DIRSEP: &str = "/";

#[cfg(windows)]
fn windows_paths(s: &mut LuaState, pkg: GcRef<Table>) {
    // luaconf.h defaults for Windows; '!' is the executable's directory.
    const LDIR: &str = "!\\lua\\";
    const CDIR: &str = "!\\";
    let path_default =
        format!(".\\?.lua;{LDIR}?.lua;{LDIR}?\\init.lua;{CDIR}?.lua;{CDIR}?\\init.lua");
    let cpath_default = format!(".\\?.dll;{CDIR}?.dll;{CDIR}loadall.dll");
    let progdir = crate::sys::progdir();
    let mk = |env: &str, def: &str| -> String {
        let p = match std::env::var(env) {
            Ok(v) => v.replace(";;", &format!(";{def};")),
            Err(_) => def.to_string(),
        };
        match &progdir {
            Some(d) => p.replace('!', d),
            None => p,
        }
    };
    let path = mk("LUA_PATH", &path_default);
    let cpath = mk("LUA_CPATH", &cpath_default);
    let pv = Val::Str(s.gc.intern_string(path.as_bytes()));
    tset(s, pkg, "path", pv);
    let cv = Val::Str(s.gc.intern_string(cpath.as_bytes()));
    tset(s, pkg, "cpath", cv);
    let conf = Val::Str(s.gc.intern_string(b"\\\n;\n?\n!\n-\n"));
    tset(s, pkg, "config", conf);
}

fn rust_closure(s: &mut LuaState, f: fn(&mut LuaState) -> LuaResult<u32>, name: &str, ups: Vec<Val>) -> Val {
    let cl = Closure::Rust(RustClosure {
        func: f,
        upvalues: ups,
        name: name.to_string(),
        env: None,
        native: 0,
    });
    Val::Function(s.gc.alloc_closure(cl))
}

/// Replaces rilua's placeholder C loaders and `package.loadlib` with ones
/// that load real Lua 5.1 C modules.
fn install_c_loaders(s: &mut LuaState, pkg: GcRef<Table>) {
    let pv = Val::Table(pkg);
    let f = rust_closure(s, ll_loadlib, "loadlib", vec![]);
    tset(s, pkg, "loadlib", f);
    if let Val::Table(loaders) = tget(s, pkg, "loaders") {
        let c = rust_closure(s, loader_c, "loader_C", vec![pv]);
        let croot = rust_closure(s, loader_croot, "loader_Croot", vec![pv]);
        if let Some(t) = s.gc.tables.get_mut(loaders) {
            let _ = t.raw_set(Val::Num(3.0), c, &s.gc.string_arena);
            let _ = t.raw_set(Val::Num(4.0), croot, &s.gc.string_arena);
        }
        s.gc.barrier_back(loaders);
    }
}

fn arg(s: &LuaState, n: usize) -> Val {
    let i = s.base + n;
    if i < s.top { s.stack_get(i) } else { Val::Nil }
}

fn upvalue(s: &LuaState, n: usize) -> Val {
    let fidx = s.call_stack[s.ci].func;
    match s.stack_get(fidx) {
        Val::Function(r) => match s.gc.closures.get(r) {
            Some(Closure::Rust(c)) => c.upvalues.get(n).copied().unwrap_or(Val::Nil),
            _ => Val::Nil,
        },
        _ => Val::Nil,
    }
}

fn push_str(s: &mut LuaState, msg: &str) {
    let v = Val::Str(s.gc.intern_string(msg.as_bytes()));
    s.push(v);
}

enum LoadErr {
    Lib(String),
    Func(String),
}

/// PUC-Rio `ll_loadfunc`: opens (or reuses) a library and returns the
/// function `sym` as a C function value.
fn ll_loadfunc(s: &mut LuaState, path: &str, sym: &str) -> Result<Val, LoadErr> {
    let g = unsafe { &mut *Global::from_state(s) };
    let idx = match g.libs.iter().position(|lib| lib.path == path) {
        Some(i) => i,
        None => {
            let lib = crate::sys::Library::open(path).map_err(LoadErr::Lib)?;
            g.libs.push(lib);
            g.libs.len() - 1
        }
    };
    let p = g.libs[idx].sym(sym).map_err(LoadErr::Func)?;
    let f: lua_CFunction = unsafe { std::mem::transmute::<*mut std::ffi::c_void, lua_CFunction>(p) };
    let env = g.state.global;
    Ok(new_cclosure(g, f, Vec::new(), env))
}

/// `package.loadlib(path, funcname)`.
fn ll_loadlib(s: &mut LuaState) -> LuaResult<u32> {
    let path = str_of(s, arg(s, 0)).ok_or_else(|| s.type_error(1, "string"))?;
    let init = str_of(s, arg(s, 1)).ok_or_else(|| s.type_error(2, "string"))?;
    match ll_loadfunc(s, &path, &init) {
        Ok(f) => {
            s.push(f);
            Ok(1)
        }
        Err(e) => {
            let (msg, kind) = match e {
                LoadErr::Lib(m) => (m, "open"),
                LoadErr::Func(m) => (m, "init"),
            };
            s.push(Val::Nil);
            push_str(s, &msg);
            push_str(s, kind);
            Ok(3)
        }
    }
}

/// PUC-Rio `findfile` for `package.cpath`.
fn findfile(s: &mut LuaState, name: &str) -> LuaResult<Result<String, String>> {
    let pkg = upvalue(s, 0);
    let path = match pkg {
        Val::Table(t) => {
            let v = tget(s, t, "cpath");
            str_of(s, v)
        }
        _ => None,
    };
    let Some(path) = path else {
        return Err(rt_error("'package.cpath' must be a string"));
    };
    let name = name.replace('.', DIRSEP);
    let mut errs = String::new();
    for template in path.split(';').filter(|t| !t.is_empty()) {
        let filename = template.replace('?', &name);
        if std::fs::File::open(&filename).is_ok() {
            return Ok(Ok(filename));
        }
        errs.push_str(&format!("\n\tno file '{filename}'"));
    }
    Ok(Err(errs))
}

fn mkfuncname(modname: &str) -> String {
    let m = match modname.find('-') {
        Some(i) => &modname[i + 1..],
        None => modname,
    };
    format!("luaopen_{}", m.replace('.', "_"))
}

fn loader_c(s: &mut LuaState) -> LuaResult<u32> {
    let name = str_of(s, arg(s, 0)).ok_or_else(|| s.type_error(1, "string"))?;
    let filename = match findfile(s, &name)? {
        Ok(f) => f,
        Err(msg) => {
            push_str(s, &msg);
            return Ok(1);
        }
    };
    match ll_loadfunc(s, &filename, &mkfuncname(&name)) {
        Ok(f) => {
            s.push(f);
            Ok(1)
        }
        Err(LoadErr::Lib(m) | LoadErr::Func(m)) => Err(rt_error(format!(
            "error loading module '{name}' from file '{filename}':\n\t{m}"
        ))),
    }
}

fn loader_croot(s: &mut LuaState) -> LuaResult<u32> {
    let name = str_of(s, arg(s, 0)).ok_or_else(|| s.type_error(1, "string"))?;
    let Some(dot) = name.find('.') else {
        return Ok(0);
    };
    let root = name[..dot].to_string();
    let filename = match findfile(s, &root)? {
        Ok(f) => f,
        Err(msg) => {
            push_str(s, &msg);
            return Ok(1);
        }
    };
    match ll_loadfunc(s, &filename, &mkfuncname(&name)) {
        Ok(f) => {
            s.push(f);
            Ok(1)
        }
        Err(LoadErr::Lib(m)) => Err(rt_error(format!(
            "error loading module '{name}' from file '{filename}':\n\t{m}"
        ))),
        Err(LoadErr::Func(_)) => {
            push_str(s, &format!("\n\tno module '{name}' in file '{filename}'"));
            Ok(1)
        }
    }
}

