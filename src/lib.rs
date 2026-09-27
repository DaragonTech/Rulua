//! Lua 5.1 C API on top of the rilua interpreter.
//!
//! This crate builds a drop-in `lua5.1.dll` (Windows) / `liblua5.1.so`
//! exposing the complete public C API of Lua 5.1 (`lua.h`, `lauxlib.h`,
//! `lualib.h`). The virtual machine, compiler, garbage collector and the
//! standard libraries all come from rilua; this crate only translates the
//! stack-based C API onto rilua's `LuaState`.
//!
//! Layout:
//! - [`global`]  -- `lua_State` handles, thread bookkeeping, error
//!   propagation (`lua_error` is a Rust unwind that travels through C frames
//!   using the `C-unwind` ABI), and the trampoline that lets rilua call
//!   `lua_CFunction`s.
//! - [`api`]     -- the `lua_*` functions from `lua.h`.
//! - [`debug`]   -- the debug interface (`lua_getstack`, `lua_getinfo`, hooks...).
//! - [`libs`]    -- `luaopen_*`, `luaL_openlibs`, and C-module loading for
//!   `require` / `package.loadlib`.
//! - `csrc/lauxlib.c` -- the unmodified Lua 5.1.4 auxiliary library.
//! - `csrc/lapi_fmt.c` -- `lua_pushfstring` / `lua_pushvfstring` (varargs).

#![allow(non_camel_case_types)]
#![allow(clippy::missing_safety_doc)]
#![allow(unsafe_op_in_unsafe_fn)]

pub mod api;
pub mod debug;
pub mod global;
pub mod libs;
mod sys;

use std::ffi::{c_char, c_int, c_void};

pub use global::lua_State;

/// mimalloc as the global allocator: rilua allocates many small objects
/// (and buffers on every coroutine switch); the Windows system heap is
/// several times slower than mimalloc for this pattern.
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

// ---------------------------------------------------------------------------
// Types from lua.h / luaconf.h
// ---------------------------------------------------------------------------

/// `lua_Number` (luaconf.h: `LUA_NUMBER double`).
pub type lua_Number = f64;
/// `lua_Integer` (luaconf.h: `LUA_INTEGER ptrdiff_t`).
pub type lua_Integer = isize;

/// `typedef int (*lua_CFunction) (lua_State *L);`
pub type lua_CFunction = unsafe extern "C-unwind" fn(*mut lua_State) -> c_int;

/// `typedef const char * (*lua_Reader) (lua_State *L, void *ud, size_t *sz);`
pub type lua_Reader =
    unsafe extern "C-unwind" fn(*mut lua_State, *mut c_void, *mut usize) -> *const c_char;

/// `typedef int (*lua_Writer) (lua_State *L, const void* p, size_t sz, void* ud);`
pub type lua_Writer =
    unsafe extern "C-unwind" fn(*mut lua_State, *const c_void, usize, *mut c_void) -> c_int;

/// `typedef void * (*lua_Alloc) (void *ud, void *ptr, size_t osize, size_t nsize);`
pub type lua_Alloc =
    Option<unsafe extern "C-unwind" fn(*mut c_void, *mut c_void, usize, usize) -> *mut c_void>;

/// `typedef void (*lua_Hook) (lua_State *L, lua_Debug *ar);`
pub type lua_Hook = unsafe extern "C-unwind" fn(*mut lua_State, *mut lua_Debug);

/// `LUA_IDSIZE` from luaconf.h.
pub const LUA_IDSIZE: usize = 60;

/// `struct lua_Debug` (lua.h, Lua 5.1).
#[repr(C)]
pub struct lua_Debug {
    pub event: c_int,
    pub name: *const c_char,
    pub namewhat: *const c_char,
    pub what: *const c_char,
    pub source: *const c_char,
    pub currentline: c_int,
    pub nups: c_int,
    pub linedefined: c_int,
    pub lastlinedefined: c_int,
    pub short_src: [c_char; LUA_IDSIZE],
    // private part
    pub i_ci: c_int,
}

// ---------------------------------------------------------------------------
// Constants from lua.h
// ---------------------------------------------------------------------------

pub const LUA_MULTRET: c_int = -1;

pub const LUA_REGISTRYINDEX: c_int = -10000;
pub const LUA_ENVIRONINDEX: c_int = -10001;
pub const LUA_GLOBALSINDEX: c_int = -10002;

pub const LUA_YIELD: c_int = 1;
pub const LUA_ERRRUN: c_int = 2;
pub const LUA_ERRSYNTAX: c_int = 3;
pub const LUA_ERRMEM: c_int = 4;
pub const LUA_ERRERR: c_int = 5;

pub const LUA_TNONE: c_int = -1;
pub const LUA_TNIL: c_int = 0;
pub const LUA_TBOOLEAN: c_int = 1;
pub const LUA_TLIGHTUSERDATA: c_int = 2;
pub const LUA_TNUMBER: c_int = 3;
pub const LUA_TSTRING: c_int = 4;
pub const LUA_TTABLE: c_int = 5;
pub const LUA_TFUNCTION: c_int = 6;
pub const LUA_TUSERDATA: c_int = 7;
pub const LUA_TTHREAD: c_int = 8;

pub const LUA_MINSTACK: c_int = 20;

pub const LUA_GCSTOP: c_int = 0;
pub const LUA_GCRESTART: c_int = 1;
pub const LUA_GCCOLLECT: c_int = 2;
pub const LUA_GCCOUNT: c_int = 3;
pub const LUA_GCCOUNTB: c_int = 4;
pub const LUA_GCSTEP: c_int = 5;
pub const LUA_GCSETPAUSE: c_int = 6;
pub const LUA_GCSETSTEPMUL: c_int = 7;

pub const LUA_HOOKCALL: c_int = 0;
pub const LUA_HOOKRET: c_int = 1;
pub const LUA_HOOKLINE: c_int = 2;
pub const LUA_HOOKCOUNT: c_int = 3;
pub const LUA_HOOKTAILRET: c_int = 4;

pub const LUA_MASKCALL: c_int = 1 << LUA_HOOKCALL;
pub const LUA_MASKRET: c_int = 1 << LUA_HOOKRET;
pub const LUA_MASKLINE: c_int = 1 << LUA_HOOKLINE;
pub const LUA_MASKCOUNT: c_int = 1 << LUA_HOOKCOUNT;

/// `LUAI_MAXCSTACK` from luaconf.h.
pub const LUAI_MAXCSTACK: c_int = 8000;
