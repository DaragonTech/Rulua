//! Standard library: modular implementation of Lua 5.1.1's built-in libraries.

pub mod base;
pub mod coroutine;
pub mod debug;
pub mod io;
pub mod math;
pub mod os;
pub mod package;
pub mod string;
pub mod table;
pub mod testlib;

use std::ops::{BitOr, BitOrAssign};

use crate::error::{LuaError, LuaResult, RuntimeError};
use crate::vm::closure::{Closure, RustClosure, RustFn};
use crate::vm::debug_info;
use crate::vm::gc::arena::GcRef;
use crate::vm::state::LuaState;
use crate::vm::table::Table;
use crate::vm::value::{Userdata, Val};

// ---------------------------------------------------------------------------
// StdLib bitflags for selective library loading
// ---------------------------------------------------------------------------

/// Bitflag set for selecting which standard libraries to load.
///
/// Combines with `|` for ergonomic selective loading:
///
/// ```rust
/// use rilua::{Lua, StdLib};
///
/// let lua = Lua::new_with(StdLib::BASE | StdLib::STRING | StdLib::TABLE).unwrap();
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StdLib(u16);

impl StdLib {
    /// Base library: `print`, `type`, `tostring`, `error`, `pcall`, etc.
    pub const BASE: Self = Self(1 << 0);
    /// String library: `string.format`, `string.find`, etc.
    pub const STRING: Self = Self(1 << 1);
    /// Table library: `table.insert`, `table.sort`, etc.
    pub const TABLE: Self = Self(1 << 2);
    /// Math library: `math.sin`, `math.random`, etc.
    pub const MATH: Self = Self(1 << 3);
    /// I/O library: `io.open`, `io.read`, etc.
    pub const IO: Self = Self(1 << 4);
    /// OS library: `os.time`, `os.clock`, etc.
    pub const OS: Self = Self(1 << 5);
    /// Debug library: `debug.getinfo`, `debug.traceback`, etc.
    pub const DEBUG: Self = Self(1 << 6);
    /// Package library: `require`, `module`, `package.path`, etc.
    pub const PACKAGE: Self = Self(1 << 7);
    /// Coroutine library: `coroutine.create`, `coroutine.resume`, etc.
    pub const COROUTINE: Self = Self(1 << 8);
    /// Test library: internal VM introspection (`T` global table).
    /// Not included in ALL; must be explicitly requested.
    pub const TEST: Self = Self(1 << 9);
    /// No libraries.
    pub const NONE: Self = Self(0);
    /// All standard libraries (excludes TEST).
    pub const ALL: Self = Self(
        Self::BASE.0
            | Self::STRING.0
            | Self::TABLE.0
            | Self::MATH.0
            | Self::IO.0
            | Self::OS.0
            | Self::DEBUG.0
            | Self::PACKAGE.0
            | Self::COROUTINE.0,
    );

    /// Returns true if `self` contains all flags in `other`.
    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

impl BitOr for StdLib {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for StdLib {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

/// Registers all standard library functions into the global table.
///
/// Equivalent to `open_libs_selective(state, StdLib::ALL)`.
pub fn open_libs(state: &mut LuaState) -> LuaResult<()> {
    open_libs_selective(state, StdLib::ALL)
}

/// Registers selected standard libraries into the global table.
///
/// Only loads libraries whose flags are set in `libs`. The package library
/// is always loaded last (when requested) because it populates
/// `package.loaded` with the other library tables.
pub fn open_libs_selective(state: &mut LuaState, libs: StdLib) -> LuaResult<()> {
    if libs.contains(StdLib::BASE) {
        open_base_lib(state)?;
    }
    if libs.contains(StdLib::STRING) {
        open_string_lib(state)?;
    }
    if libs.contains(StdLib::TABLE) {
        open_table_lib(state)?;
    }
    if libs.contains(StdLib::MATH) {
        open_math_lib(state)?;
    }
    if libs.contains(StdLib::OS) {
        open_os_lib(state)?;
    }
    if libs.contains(StdLib::IO) {
        io::open_io_lib(state)?;
    }
    if libs.contains(StdLib::COROUTINE) {
        open_coroutine_lib(state)?;
    }
    if libs.contains(StdLib::DEBUG) {
        open_debug_lib(state)?;
    }
    // Package must be last: populates package.loaded with other libs.
    if libs.contains(StdLib::PACKAGE) {
        package::open_package_lib(state)?;
    }
    // Test library is not in ALL; must be explicitly requested.
    if libs.contains(StdLib::TEST) {
        open_test_lib(state)?;
    }
    Ok(())
}

/// Registers the base library: global functions, `_G`, `_VERSION`.
///
/// Follows PUC-Rio's `luaopen_base` pattern from `lbaselib.c`.
fn open_base_lib(state: &mut LuaState) -> LuaResult<()> {
    register_global_fn(state, "print", base::lua_print)?;
    register_global_fn(state, "type", base::lua_type)?;
    register_global_fn(state, "tostring", base::lua_tostring)?;
    register_global_fn(state, "tonumber", base::lua_tonumber)?;
    register_global_fn(state, "assert", base::lua_assert)?;
    register_global_fn(state, "error", base::lua_error)?;
    register_global_fn(state, "pcall", base::lua_pcall)?;
    register_global_fn(state, "xpcall", base::lua_xpcall)?;
    register_global_fn(state, "setmetatable", base::lua_setmetatable)?;
    register_global_fn(state, "getmetatable", base::lua_getmetatable)?;
    register_global_fn(state, "rawget", base::lua_rawget)?;
    register_global_fn(state, "rawset", base::lua_rawset)?;
    register_global_fn(state, "rawequal", base::lua_rawequal)?;
    register_global_fn(state, "select", base::lua_select)?;
    register_global_fn(state, "unpack", base::lua_unpack)?;
    register_global_fn(state, "next", base::lua_next)?;
    register_global_fn(state, "pairs", base::lua_pairs)?;
    register_global_fn(state, "ipairs", base::lua_ipairs)?;
    register_global_fn(state, "loadstring", base::lua_loadstring)?;
    register_global_fn(state, "loadfile", base::lua_loadfile)?;
    register_global_fn(state, "dofile", base::lua_dofile)?;
    register_global_fn(state, "load", base::lua_load)?;
    register_global_fn(state, "collectgarbage", base::lua_collectgarbage)?;
    register_global_fn(state, "setfenv", base::lua_setfenv)?;
    register_global_fn(state, "getfenv", base::lua_getfenv)?;
    register_global_fn(state, "newproxy", base::lua_newproxy)?;
    register_global_fn(state, "gcinfo", base::lua_gcinfo)?;

    // Global values: _G (self-referential) and _VERSION.
    register_global_val(state, "_G", Val::Table(state.global))?;
    let version_str = state.gc.intern_string(b"Lua 5.1");
    register_global_val(state, "_VERSION", Val::Str(version_str))?;

    Ok(())
}

/// Registers the table library as `table` global.
///
/// Follows PUC-Rio's `luaopen_table` pattern from `ltablib.c`.
fn open_table_lib(state: &mut LuaState) -> LuaResult<()> {
    let table_table = state.gc.alloc_table(Table::new());
    register_table_fn(state, table_table, "concat", table::tab_concat)?;
    register_table_fn(state, table_table, "foreach", table::tab_foreach)?;
    register_table_fn(state, table_table, "foreachi", table::tab_foreachi)?;
    register_table_fn(state, table_table, "getn", table::tab_getn)?;
    register_table_fn(state, table_table, "insert", table::tab_insert)?;
    register_table_fn(state, table_table, "maxn", table::tab_maxn)?;
    register_table_fn(state, table_table, "remove", table::tab_remove)?;
    register_table_fn(state, table_table, "setn", table::tab_setn)?;
    register_table_fn(state, table_table, "sort", table::tab_sort)?;
    register_global_val(state, "table", Val::Table(table_table))?;
    Ok(())
}

/// Registers the math library as `math` global.
///
/// Follows PUC-Rio's `luaopen_math` pattern from `lmathlib.c`:
/// 28 functions + `math.pi` + `math.huge` + `math.mod` alias.
fn open_math_lib(state: &mut LuaState) -> LuaResult<()> {
    let math_table = state.gc.alloc_table(Table::new());

    register_table_fn(state, math_table, "abs", math::math_abs)?;
    register_table_fn(state, math_table, "acos", math::math_acos)?;
    register_table_fn(state, math_table, "asin", math::math_asin)?;
    register_table_fn(state, math_table, "atan", math::math_atan)?;
    register_table_fn(state, math_table, "atan2", math::math_atan2)?;
    register_table_fn(state, math_table, "ceil", math::math_ceil)?;
    register_table_fn(state, math_table, "cos", math::math_cos)?;
    register_table_fn(state, math_table, "cosh", math::math_cosh)?;
    register_table_fn(state, math_table, "deg", math::math_deg)?;
    register_table_fn(state, math_table, "exp", math::math_exp)?;
    register_table_fn(state, math_table, "floor", math::math_floor)?;
    register_table_fn(state, math_table, "fmod", math::math_fmod)?;
    register_table_fn(state, math_table, "frexp", math::math_frexp)?;
    register_table_fn(state, math_table, "ldexp", math::math_ldexp)?;
    register_table_fn(state, math_table, "log", math::math_log)?;
    register_table_fn(state, math_table, "log10", math::math_log10)?;
    register_table_fn(state, math_table, "max", math::math_max)?;
    register_table_fn(state, math_table, "min", math::math_min)?;
    register_table_fn(state, math_table, "modf", math::math_modf)?;
    register_table_fn(state, math_table, "pow", math::math_pow)?;
    register_table_fn(state, math_table, "rad", math::math_rad)?;
    register_table_fn(state, math_table, "random", math::math_random)?;
    register_table_fn(state, math_table, "randomseed", math::math_randomseed)?;
    register_table_fn(state, math_table, "sin", math::math_sin)?;
    register_table_fn(state, math_table, "sinh", math::math_sinh)?;
    register_table_fn(state, math_table, "sqrt", math::math_sqrt)?;
    register_table_fn(state, math_table, "tan", math::math_tan)?;
    register_table_fn(state, math_table, "tanh", math::math_tanh)?;

    // Deprecated alias: mod = fmod (LUA_COMPAT_MOD, enabled by default).
    register_table_fn(state, math_table, "mod", math::math_fmod)?;

    // Constants: math.pi and math.huge.
    let pi_key = state.gc.intern_string(b"pi");
    let huge_key = state.gc.intern_string(b"huge");
    let mt = state.gc.tables.get_mut(math_table).ok_or_else(|| {
        LuaError::Runtime(RuntimeError {
            message: "math table not found".into(),
            level: 0,
            traceback: vec![],
        })
    })?;
    mt.raw_set(
        Val::Str(pi_key),
        Val::Num(std::f64::consts::PI),
        &state.gc.string_arena,
    )?;
    mt.raw_set(
        Val::Str(huge_key),
        Val::Num(f64::INFINITY),
        &state.gc.string_arena,
    )?;

    register_global_val(state, "math", Val::Table(math_table))?;
    Ok(())
}

/// Registers the OS library as `os` global.
///
/// Follows PUC-Rio's `luaopen_os` pattern from `loslib.c`:
/// 11 functions.
fn open_os_lib(state: &mut LuaState) -> LuaResult<()> {
    let os_table = state.gc.alloc_table(Table::new());

    register_table_fn(state, os_table, "clock", os::os_clock)?;
    register_table_fn(state, os_table, "date", os::os_date)?;
    register_table_fn(state, os_table, "difftime", os::os_difftime)?;
    register_table_fn(state, os_table, "execute", os::os_execute)?;
    register_table_fn(state, os_table, "exit", os::os_exit)?;
    register_table_fn(state, os_table, "getenv", os::os_getenv)?;
    register_table_fn(state, os_table, "remove", os::os_remove)?;
    register_table_fn(state, os_table, "rename", os::os_rename)?;
    register_table_fn(state, os_table, "setlocale", os::os_setlocale)?;
    register_table_fn(state, os_table, "time", os::os_time)?;
    register_table_fn(state, os_table, "tmpname", os::os_tmpname)?;

    register_global_val(state, "os", Val::Table(os_table))?;
    Ok(())
}

/// Registers the string library as `string` global and sets up the string
/// type metatable so that `("hello"):upper()` method syntax works.
///
/// Follows PUC-Rio's `luaopen_string` + `createmetatable` pattern from
/// `lstrlib.c`.
fn open_string_lib(state: &mut LuaState) -> LuaResult<()> {
    // Create the string library table and populate it with functions.
    let string_table = state.gc.alloc_table(Table::new());
    register_table_fn(state, string_table, "len", string::str_len)?;
    register_table_fn(state, string_table, "byte", string::str_byte)?;
    register_table_fn(state, string_table, "char", string::str_char)?;
    register_table_fn(state, string_table, "sub", string::str_sub)?;
    register_table_fn(state, string_table, "rep", string::str_rep)?;
    register_table_fn(state, string_table, "reverse", string::str_reverse)?;
    register_table_fn(state, string_table, "lower", string::str_lower)?;
    register_table_fn(state, string_table, "upper", string::str_upper)?;
    register_table_fn(state, string_table, "format", string::str_format)?;
    register_table_fn(state, string_table, "find", string::str_find)?;
    register_table_fn(state, string_table, "match", string::str_match)?;
    register_table_fn(state, string_table, "gmatch", string::str_gmatch)?;
    register_table_fn(state, string_table, "gsub", string::str_gsub)?;
    register_table_fn(state, string_table, "dump", string::str_dump)?;
    // LUA_COMPAT_GFIND: string.gfind = string.gmatch (same closure object).
    // PUC-Rio copies the value via lua_getfield/lua_setfield so gfind == gmatch.
    let gmatch_key = state.gc.intern_string(b"gmatch");
    let gfind_key = state.gc.intern_string(b"gfind");
    let gmatch_val = state.gc.tables.get(string_table).map_or(Val::Nil, |t| {
        t.get(Val::Str(gmatch_key), &state.gc.string_arena)
    });
    if let Some(t) = state.gc.tables.get_mut(string_table) {
        t.raw_set(Val::Str(gfind_key), gmatch_val, &state.gc.string_arena)?;
    }

    // Register as global "string".
    register_global_val(state, "string", Val::Table(string_table))?;

    // Create a metatable for the string type: { __index = string_table }.
    // This enables method syntax: ("hello"):upper() resolves via __index.
    let mt = state.gc.alloc_table(Table::new());
    let index_key = state.gc.intern_string(b"__index");
    let mt_table = state.gc.tables.get_mut(mt).ok_or_else(|| {
        LuaError::Runtime(RuntimeError {
            message: "string metatable not found".into(),
            level: 0,
            traceback: vec![],
        })
    })?;
    mt_table.raw_set(
        Val::Str(index_key),
        Val::Table(string_table),
        &state.gc.string_arena,
    )?;

    // Set the string type metatable (type tag 3 = String).
    state.gc.type_metatables[3] = Some(mt);

    Ok(())
}

/// Registers the coroutine library as `coroutine` global.
///
/// Follows PUC-Rio's `luaopen_base` coroutine registration from `lbaselib.c`.
fn open_coroutine_lib(state: &mut LuaState) -> LuaResult<()> {
    let co_table = state.gc.alloc_table(Table::new());

    register_table_fn(state, co_table, "create", coroutine::co_create)?;
    register_table_fn(state, co_table, "resume", coroutine::co_resume)?;
    register_table_fn(state, co_table, "yield", coroutine::co_yield)?;
    register_table_fn(state, co_table, "wrap", coroutine::co_wrap)?;
    register_table_fn(state, co_table, "status", coroutine::co_status)?;
    register_table_fn(state, co_table, "running", coroutine::co_running)?;

    register_global_val(state, "coroutine", Val::Table(co_table))?;
    Ok(())
}

/// Registers the test library as `T` global table.
///
/// Provides internal VM introspection functions used by PUC-Rio's test suite.
/// Not included in `StdLib::ALL`; must be explicitly loaded via `StdLib::TEST`.
fn open_test_lib(state: &mut LuaState) -> LuaResult<()> {
    let t_table = state.gc.alloc_table(Table::new());

    register_table_fn(state, t_table, "querytab", testlib::t_querytab)?;
    register_table_fn(state, t_table, "hash", testlib::t_hash)?;
    register_table_fn(state, t_table, "int2fb", testlib::t_int2fb)?;
    register_table_fn(state, t_table, "log2", testlib::t_log2)?;
    register_table_fn(state, t_table, "listcode", testlib::t_listcode)?;
    register_table_fn(state, t_table, "setyhook", testlib::t_setyhook)?;
    register_table_fn(state, t_table, "resume", testlib::t_resume)?;
    register_table_fn(state, t_table, "d2s", testlib::t_d2s)?;
    register_table_fn(state, t_table, "s2d", testlib::t_s2d)?;

    // T.testC mini-interpreter and additional test functions.
    register_table_fn(state, t_table, "testC", testlib::t_testc)?;
    register_table_fn(state, t_table, "newuserdata", testlib::t_newuserdata)?;
    register_table_fn(state, t_table, "udataval", testlib::t_udataval)?;
    register_table_fn(state, t_table, "pushuserdata", testlib::t_pushuserdata)?;
    register_table_fn(state, t_table, "ref", testlib::t_ref)?;
    register_table_fn(state, t_table, "unref", testlib::t_unref)?;
    register_table_fn(state, t_table, "getref", testlib::t_getref)?;
    register_table_fn(state, t_table, "upvalue", testlib::t_upvalue)?;
    register_table_fn(state, t_table, "checkmemory", testlib::t_checkmemory)?;
    register_table_fn(state, t_table, "gsub", testlib::t_gsub)?;
    register_table_fn(state, t_table, "doonnewstack", testlib::t_doonnewstack)?;
    register_table_fn(state, t_table, "newstate", testlib::t_newstate)?;
    register_table_fn(state, t_table, "closestate", testlib::t_closestate)?;
    register_table_fn(state, t_table, "doremote", testlib::t_doremote)?;
    register_table_fn(state, t_table, "loadlib", testlib::t_loadlib)?;
    register_table_fn(state, t_table, "totalmem", testlib::t_totalmem)?;

    register_global_val(state, "T", Val::Table(t_table))?;
    Ok(())
}

/// Registers an arbitrary value in the global table.
fn register_global_val(state: &mut LuaState, name: &str, val: Val) -> LuaResult<()> {
    let key_ref = state.gc.intern_string(name.as_bytes());
    let key = Val::Str(key_ref);
    let global = state.global;
    let table = state.gc.tables.get_mut(global).ok_or_else(|| {
        LuaError::Runtime(RuntimeError {
            message: "global table not found".into(),
            level: 0,
            traceback: vec![],
        })
    })?;
    table.raw_set(key, val, &state.gc.string_arena)?;
    Ok(())
}

/// Registers the debug library as `debug` global.
///
/// Follows PUC-Rio's `luaopen_debug` pattern from `ldblib.c`.
fn open_debug_lib(state: &mut LuaState) -> LuaResult<()> {
    let debug_table = state.gc.alloc_table(Table::new());

    register_table_fn(state, debug_table, "debug", debug::db_debug)?;
    register_table_fn(state, debug_table, "getfenv", debug::db_getfenv)?;
    register_table_fn(state, debug_table, "gethook", debug::db_gethook)?;
    register_table_fn(state, debug_table, "getinfo", debug::db_getinfo)?;
    register_table_fn(state, debug_table, "getlocal", debug::db_getlocal)?;
    register_table_fn(state, debug_table, "getregistry", debug::db_getregistry)?;
    register_table_fn(state, debug_table, "getmetatable", debug::db_getmetatable)?;
    register_table_fn(state, debug_table, "getupvalue", debug::db_getupvalue)?;
    register_table_fn(state, debug_table, "setfenv", debug::db_setfenv)?;
    register_table_fn(state, debug_table, "sethook", debug::db_sethook)?;
    register_table_fn(state, debug_table, "setlocal", debug::db_setlocal)?;
    register_table_fn(state, debug_table, "setmetatable", debug::db_setmetatable)?;
    register_table_fn(state, debug_table, "setupvalue", debug::db_setupvalue)?;
    register_table_fn(state, debug_table, "traceback", debug::db_traceback)?;

    register_global_val(state, "debug", Val::Table(debug_table))?;
    Ok(())
}

/// Creates a `RustClosure`, interns the name string, and sets it
/// in the global table.
fn register_global_fn(state: &mut LuaState, name: &str, func: RustFn) -> LuaResult<()> {
    let global = state.global;
    register_table_fn(state, global, name, func)
}

/// Creates a `RustClosure` and sets it in an arbitrary table.
fn register_table_fn(
    state: &mut LuaState,
    table_ref: GcRef<Table>,
    name: &str,
    func: RustFn,
) -> LuaResult<()> {
    let closure = Closure::Rust(RustClosure::new(func, name));
    let closure_ref = state.gc.alloc_closure(closure);

    let key_ref = state.gc.intern_string(name.as_bytes());
    let key = Val::Str(key_ref);
    let val = Val::Function(closure_ref);

    let table = state.gc.tables.get_mut(table_ref).ok_or_else(|| {
        LuaError::Runtime(RuntimeError {
            message: "table not found for function registration".into(),
            level: 0,
            traceback: vec![],
        })
    })?;
    table.raw_set(key, val, &state.gc.string_arena)?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Registry helpers for named metatables (used by I/O library, etc.)
// ---------------------------------------------------------------------------

/// Creates or fetches a named metatable from the registry.
///
/// If the registry already has a table at `name`, returns it. Otherwise
/// creates a new table, stores it in the registry at `name`, and returns it.
///
/// Matches PUC-Rio's `luaL_newmetatable` from `lauxlib.c`.
pub fn new_metatable(state: &mut LuaState, name: &str) -> LuaResult<GcRef<Table>> {
    // Check if the name is already in the registry.
    if let Some(existing) = get_registry_metatable(state, name) {
        return Ok(existing);
    }

    // Create a new table and store it in the registry.
    let mt = state.gc.alloc_table(Table::new());
    let key_ref = state.gc.intern_string(name.as_bytes());
    let registry = state.registry;
    let reg_table = state.gc.tables.get_mut(registry).ok_or_else(|| {
        LuaError::Runtime(RuntimeError {
            message: "registry not found".into(),
            level: 0,
            traceback: vec![],
        })
    })?;
    reg_table.raw_set(Val::Str(key_ref), Val::Table(mt), &state.gc.string_arena)?;
    Ok(mt)
}

/// Looks up a named metatable in the registry.
///
/// Returns `Some(table_ref)` if found, `None` otherwise. Uses `intern_string`
/// to find the key (idempotent -- returns the existing interned string).
///
/// Matches PUC-Rio's `luaL_getmetatable` macro from `lauxlib.h`.
pub fn get_registry_metatable(state: &mut LuaState, name: &str) -> Option<GcRef<Table>> {
    let key_ref = state.gc.intern_string(name.as_bytes());
    let registry = state.gc.tables.get(state.registry)?;
    let val = registry.get_str(key_ref, &state.gc.string_arena);
    match val {
        Val::Table(r) => Some(r),
        _ => None,
    }
}

/// Formats a "bad argument" error with function name resolution.
///
/// Takes a 1-based argument number and an error message. Resolves the
/// function name from the current call frame via `getfuncname`. Handles
/// method calls by decrementing `narg` and using "calling 'name' on bad self"
/// for self-argument errors.
///
/// Matches PUC-Rio's `luaL_argerror` from `lauxlib.c`.
pub fn arg_error(state: &LuaState, narg: usize, msg: &str) -> LuaError {
    let name_info = debug_info::getfuncname(state, state.ci, &state.gc.string_arena);

    let message = match name_info {
        Some(("method", ref name)) => {
            let adjusted = narg.saturating_sub(1);
            if adjusted == 0 {
                format!("calling '{name}' on bad self ({msg})")
            } else {
                format!("bad argument #{adjusted} to '{name}' ({msg})")
            }
        }
        Some((_, ref name)) => {
            format!("bad argument #{narg} to '{name}' ({msg})")
        }
        None => {
            format!("bad argument #{narg} to '?' ({msg})")
        }
    };

    LuaError::Runtime(RuntimeError {
        message,
        level: 0,
        traceback: vec![],
    })
}

/// Formats a type mismatch error with "no value" for missing arguments.
///
/// Takes a 0-based argument index and the expected type name. Checks whether
/// the argument slot exists on the stack: if not, reports "no value" instead
/// of "nil". Wraps the inner message via `arg_error`.
///
/// Matches PUC-Rio's `luaL_typerror` from `lauxlib.c`.
pub fn type_error(state: &LuaState, arg_idx: usize, expected: &str) -> LuaError {
    let actual = if state.base + arg_idx < state.top {
        state.stack_get(state.base + arg_idx).type_name()
    } else {
        "no value"
    };

    let msg = format!("{expected} expected, got {actual}");
    arg_error(state, arg_idx + 1, &msg)
}

/// Validates that the argument at `arg_num` (0-based) is a userdata with
/// the named metatable from the registry.
///
/// Returns the userdata `GcRef` on success. On failure, produces an error
/// with `arg_error` formatting including "no value" for missing arguments.
///
/// Matches PUC-Rio's `luaL_checkudata` from `lauxlib.c`.
pub fn check_userdata(
    state: &mut LuaState,
    arg_num: usize,
    name: &str,
) -> LuaResult<GcRef<Userdata>> {
    let val = if state.base + arg_num < state.top {
        state.stack_get(state.base + arg_num)
    } else {
        return Err(type_error(state, arg_num, name));
    };

    let Val::Userdata(ud_ref) = val else {
        return Err(type_error(state, arg_num, name));
    };

    let expected_mt = get_registry_metatable(state, name);
    let actual_mt = state.gc.userdata.get(ud_ref).and_then(Userdata::metatable);

    match (expected_mt, actual_mt) {
        (Some(expected), Some(actual)) if expected == actual => Ok(ud_ref),
        _ => Err(type_error(state, arg_num, name)),
    }
}
