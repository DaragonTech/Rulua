//! Package library: require, module, loaders, path searching.
//!
//! Reference: `loadlib.c` in PUC-Rio Lua 5.1.1.

use crate::error::{LuaError, LuaResult, RuntimeError};
use crate::vm::closure::{Closure, RustClosure};
use crate::vm::gc::arena::GcRef;
use crate::vm::state::LuaState;
use crate::vm::table::Table;
use crate::vm::value::Val;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Directory separator (Linux/macOS: `/`, Windows: `\`).
const LUA_DIRSEP: &str = "/";

/// Path separator in search paths.
const LUA_PATHSEP: char = ';';

/// Template mark in path templates (replaced with module name).
const LUA_PATH_MARK: &str = "?";

/// Exec dir placeholder (not supported, but present in config).
const LUA_EXECDIR: &str = "!";

/// Ignore mark: prefix stripped before building C function name.
const LUA_IGMARK: &str = "-";

/// Default Lua search path.
const LUA_PATH_DEFAULT: &str = "./?.lua;/usr/local/share/lua/5.1/?.lua;/usr/local/share/lua/5.1/?/init.lua;/usr/local/lib/lua/5.1/?.lua;/usr/local/lib/lua/5.1/?/init.lua";

/// Default C library search path.
const LUA_CPATH_DEFAULT: &str =
    "./?.so;/usr/local/lib/lua/5.1/?.so;/usr/local/lib/lua/5.1/loadall.so";

/// Sentinel value for circular `require` detection.
/// A lightuserdata with a distinctive address; truthy so it triggers
/// the "already loaded" branch, but recognizable for the loop check.
const SENTINEL: usize = 0xDEAD_CAFE;

/// Registry key for the `_LOADED` table.
const LOADED_KEY: &str = "_LOADED";

// ---------------------------------------------------------------------------
// Argument helpers (same pattern as base.rs / os.rs)
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

fn simple_error(msg: String) -> LuaError {
    LuaError::Runtime(RuntimeError {
        message: msg,
        level: 0,
        traceback: vec![],
    })
}

/// Extract a Lua string argument, returning its bytes as a `String`.
fn check_string(state: &LuaState, name: &str, n: usize) -> LuaResult<String> {
    let val = arg(state, n);
    match val {
        Val::Str(r) => Ok(state
            .gc
            .string_arena
            .get(r)
            .map(|s| String::from_utf8_lossy(s.data()).to_string())
            .unwrap_or_default()),
        _ => Err(bad_argument(name, n + 1, "string expected")),
    }
}

// ---------------------------------------------------------------------------
// Package table accessor (reads upvalue[0] from current RustClosure)
// ---------------------------------------------------------------------------

/// Reads the package table from `upvalues[0]` of the currently executing
/// Rust closure. All package library functions that need the package table
/// are registered as `RustClosure` with the package table as their first
/// upvalue.
fn get_package_table(state: &LuaState) -> LuaResult<GcRef<Table>> {
    let ci = &state.call_stack[state.ci];
    let func_val = state.stack_get(ci.func);
    let Val::Function(closure_ref) = func_val else {
        return Err(simple_error(
            "package: cannot get package table from non-function".into(),
        ));
    };
    let cl = state
        .gc
        .closures
        .get(closure_ref)
        .ok_or_else(|| simple_error("package: closure not found".into()))?;
    match cl {
        Closure::Rust(rc) => {
            if let Some(Val::Table(pkg)) = rc.upvalues.first() {
                Ok(*pkg)
            } else {
                Err(simple_error("package: upvalue[0] is not a table".into()))
            }
        }
        Closure::Lua(_) => Err(simple_error("package: expected Rust closure".into())),
    }
}

// ---------------------------------------------------------------------------
// Table field helpers
// ---------------------------------------------------------------------------

/// Gets a named field from a GC table.
///
/// Uses `intern_string` to look up the key (idempotent if already interned).
/// This is slightly wasteful if the key doesn't exist, but matches the
/// pattern used throughout the stdlib.
fn get_field(state: &mut LuaState, table_ref: GcRef<Table>, key: &str) -> Val {
    let key_ref = state.gc.intern_string(key.as_bytes());
    let Some(table) = state.gc.tables.get(table_ref) else {
        return Val::Nil;
    };
    table.get_str(key_ref, &state.gc.string_arena)
}

/// Sets a named field in a GC table.
fn set_field(state: &mut LuaState, table_ref: GcRef<Table>, key: &str, val: Val) -> LuaResult<()> {
    let key_ref = state.gc.intern_string(key.as_bytes());
    let table = state
        .gc
        .tables
        .get_mut(table_ref)
        .ok_or_else(|| simple_error("package: table not found".into()))?;
    table.raw_set(Val::Str(key_ref), val, &state.gc.string_arena)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// find_or_create_table (luaL_findtable equivalent)
// ---------------------------------------------------------------------------

/// Walks a dot-separated path through nested tables starting from `root`,
/// creating intermediate tables as needed. Returns the final table ref.
///
/// If a non-table value blocks the path, returns `Err` with a message
/// naming the conflicting field.
///
/// Matches PUC-Rio's `luaL_findtable` from `lauxlib.c`.
fn find_or_create_table(
    state: &mut LuaState,
    root: GcRef<Table>,
    path: &str,
) -> LuaResult<GcRef<Table>> {
    let mut current = root;

    for part in path.split('.') {
        let key_ref = state.gc.intern_string(part.as_bytes());
        let key = Val::Str(key_ref);

        let val = {
            let table = state.gc.tables.get(current).ok_or_else(|| {
                simple_error("package: table not found in find_or_create_table".into())
            })?;
            table.get(key, &state.gc.string_arena)
        };

        match val {
            Val::Table(t) => {
                current = t;
            }
            Val::Nil => {
                // Create a new intermediate table.
                let new_table = state.gc.alloc_table(Table::new());
                let table = state
                    .gc
                    .tables
                    .get_mut(current)
                    .ok_or_else(|| simple_error("package: table not found".into()))?;
                table.raw_set(key, Val::Table(new_table), &state.gc.string_arena)?;
                current = new_table;
            }
            _ => {
                return Err(simple_error(format!("name conflict for module '{path}'")));
            }
        }
    }

    Ok(current)
}

// ---------------------------------------------------------------------------
// Path searching
// ---------------------------------------------------------------------------

/// Reads an environment variable, applies `;;` -> default expansion,
/// and stores the result in `package[field]`.
///
/// Matches PUC-Rio's `setpath` from `loadlib.c`.
fn set_path(
    state: &mut LuaState,
    pkg_table: GcRef<Table>,
    field: &str,
    env_var: &str,
    default: &str,
) -> LuaResult<()> {
    let path = match std::env::var(env_var) {
        Ok(val) => {
            // Replace ";;" with ";AUXMARK;" then AUXMARK with default.
            let aux = val.replace(";;", &format!(";{default};"));
            // Handle leading/trailing ;; edge cases.
            let aux = if val.starts_with(";;") {
                format!("{default};{}", &aux[1..])
            } else {
                aux
            };
            if val.ends_with(";;") {
                format!("{};{default}", &aux[..aux.len() - 1])
            } else {
                aux
            }
        }
        Err(_) => default.to_string(),
    };

    let path_str = state.gc.intern_string(path.as_bytes());
    set_field(state, pkg_table, field, Val::Str(path_str))
}

/// Searches for a file matching `name` in the given semicolon-separated path.
///
/// Dots in `name` are replaced with directory separators. `?` in each
/// template is replaced with the transformed name.
///
/// Returns `Ok(Some(filename))` on first match, or `Ok(None)` with an
/// accumulated error string describing all attempted paths.
///
/// Matches PUC-Rio's `findfile`/`pushnexttemplate` from `loadlib.c`.
fn search_path(path: &str, name: &str) -> (Option<String>, String) {
    // Replace dots with directory separators in the module name.
    let name = name.replace('.', LUA_DIRSEP);
    let mut errors = String::new();

    // If the name is an absolute path, try it directly first.
    // This handles os.tmpname() paths on modern Linux where tmpnam()
    // returns /tmp/lua_XXX which can't be found through template expansion.
    if name.starts_with(LUA_DIRSEP) && std::fs::metadata(&name).is_ok() {
        return (Some(name), errors);
    }

    for template in path.split(LUA_PATHSEP) {
        let template = template.trim();
        if template.is_empty() {
            continue;
        }
        let filename = template.replace(LUA_PATH_MARK, &name);
        if std::fs::metadata(&filename).is_ok() {
            return (Some(filename), errors);
        }
        errors.push_str("\n\tno file '");
        errors.push_str(&filename);
        errors.push('\'');
    }

    (None, errors)
}

// ---------------------------------------------------------------------------
// Loaders
// ---------------------------------------------------------------------------

/// Loader 1: preload loader.
///
/// Looks up `package.preload[name]`. If found, pushes the function.
/// Otherwise pushes an error string.
///
/// Matches PUC-Rio's `loader_preload` from `loadlib.c`.
fn loader_preload(state: &mut LuaState) -> LuaResult<u32> {
    let name = check_string(state, "loader_preload", 0)?;
    let pkg = get_package_table(state)?;

    // Get package.preload table.
    let preload_val = get_field(state, pkg, "preload");
    let Val::Table(preload) = preload_val else {
        return Err(simple_error(
            "'package.preload' must be a table".to_string(),
        ));
    };

    // Look up preload[name].
    let key_ref = state.gc.intern_string(name.as_bytes());
    let val = {
        let table = state
            .gc
            .tables
            .get(preload)
            .ok_or_else(|| simple_error("package.preload table not found".into()))?;
        table.get_str(key_ref, &state.gc.string_arena)
    };

    if val.is_nil() {
        let msg = state
            .gc
            .intern_string(format!("\n\tno field package.preload['{name}']").as_bytes());
        state.push(Val::Str(msg));
    } else {
        state.push(val);
    }
    Ok(1)
}

/// Loader 2: Lua file loader.
///
/// Searches `package.path` for a Lua file, compiles and returns it.
///
/// Matches PUC-Rio's `loader_Lua` from `loadlib.c`.
fn loader_lua(state: &mut LuaState) -> LuaResult<u32> {
    let name = check_string(state, "loader_lua", 0)?;
    let pkg = get_package_table(state)?;

    // Get package.path.
    let path_val = get_field(state, pkg, "path");
    let path = match path_val {
        Val::Str(r) => state
            .gc
            .string_arena
            .get(r)
            .map(|s| String::from_utf8_lossy(s.data()).to_string())
            .unwrap_or_default(),
        _ => {
            return Err(simple_error("'package.path' must be a string".to_string()));
        }
    };

    let (found, errors) = search_path(&path, &name);

    let Some(filename) = found else {
        // Not found: push error string.
        let msg = state.gc.intern_string(errors.as_bytes());
        state.push(Val::Str(msg));
        return Ok(1);
    };

    // Read and compile the file (as bytes to support binary string literals).
    let source = match std::fs::read(&filename) {
        Ok(s) => s,
        Err(e) => {
            return Err(simple_error(format!(
                "error loading module '{name}' from file '{filename}':\n\t{e}"
            )));
        }
    };

    let chunk_name = format!("@{filename}");
    match crate::compile_or_undump(&source, &chunk_name) {
        Ok(proto) => {
            let mut proto =
                crate::vm::proto::ProtoRef::try_unwrap(proto).unwrap_or_else(|rc| (*rc).clone());
            crate::patch_string_constants(&mut proto, &mut state.gc);
            let proto = crate::vm::proto::ProtoRef::new(proto);

            let lua_cl = crate::vm::closure::LuaClosure::new(proto, state.global);
            let closure_ref = state.gc.alloc_closure(Closure::Lua(lua_cl));
            state.push(Val::Function(closure_ref));
            Ok(1)
        }
        Err(e) => Err(simple_error(format!(
            "error loading module '{name}' from file '{filename}':\n\t{e}"
        ))),
    }
}

/// Loader 3: native module loader.
///
/// When the `dynmod` feature is enabled, searches `package.cpath` for a
/// shared library and loads it as a rilua-native module. Without the feature,
/// returns the same "no file" error listing as PUC-Rio when files are not
/// found.
///
/// Reference: `loader_C` in `loadlib.c`.
#[cfg(not(feature = "dynmod"))]
fn loader_c(state: &mut LuaState) -> LuaResult<u32> {
    let name = check_string(state, "loader_c", 0)?;
    let pkg = get_package_table(state)?;

    let cpath_val = get_field(state, pkg, "cpath");
    let cpath = match cpath_val {
        Val::Str(r) => state
            .gc
            .string_arena
            .get(r)
            .map(|s| String::from_utf8_lossy(s.data()).to_string())
            .unwrap_or_default(),
        _ => {
            return Err(simple_error("'package.cpath' must be a string".to_string()));
        }
    };

    let (found, errors) = search_path(&cpath, &name);
    if found.is_some() {
        let msg = format!("\n\tnative modules not enabled (cannot load '{name}')");
        let msg_ref = state.gc.intern_string(msg.as_bytes());
        state.push(Val::Str(msg_ref));
    } else {
        let msg = state.gc.intern_string(errors.as_bytes());
        state.push(Val::Str(msg));
    }
    Ok(1)
}

#[cfg(feature = "dynmod")]
fn loader_c(state: &mut LuaState) -> LuaResult<u32> {
    let name = check_string(state, "loader_c", 0)?;
    let pkg = get_package_table(state)?;

    let cpath_val = get_field(state, pkg, "cpath");
    let cpath = match cpath_val {
        Val::Str(r) => state
            .gc
            .string_arena
            .get(r)
            .map(|s| String::from_utf8_lossy(s.data()).to_string())
            .unwrap_or_default(),
        _ => {
            return Err(simple_error("'package.cpath' must be a string".to_string()));
        }
    };

    let (found, errors) = search_path(&cpath, &name);
    let Some(filename) = found else {
        let msg = state.gc.intern_string(errors.as_bytes());
        state.push(Val::Str(msg));
        return Ok(1);
    };

    let sym_name = mkfuncname(&name);
    ll_loadfunc(state, &filename, &sym_name)
}

/// Loader 4: native root loader.
///
/// For `"foo.bar"`, loads `foo.so` and looks up `rilua_open_foo_bar`.
/// Extracts root module name (before first `.`) and searches
/// `package.cpath`. Matches PUC-Rio's `loader_Croot` in `loadlib.c`.
#[cfg(not(feature = "dynmod"))]
fn loader_croot(state: &mut LuaState) -> LuaResult<u32> {
    let name = check_string(state, "loader_croot", 0)?;

    let Some(dot_pos) = name.find('.') else {
        return Ok(0);
    };
    let root = &name[..dot_pos];

    let pkg = get_package_table(state)?;
    let cpath_val = get_field(state, pkg, "cpath");
    let cpath = match cpath_val {
        Val::Str(r) => state
            .gc
            .string_arena
            .get(r)
            .map(|s| String::from_utf8_lossy(s.data()).to_string())
            .unwrap_or_default(),
        _ => {
            return Err(simple_error("'package.cpath' must be a string".to_string()));
        }
    };

    let (found, errors) = search_path(&cpath, root);
    if found.is_some() {
        let msg = format!("\n\tno module '{name}' in native root file");
        let msg_ref = state.gc.intern_string(msg.as_bytes());
        state.push(Val::Str(msg_ref));
    } else {
        let msg = state.gc.intern_string(errors.as_bytes());
        state.push(Val::Str(msg));
    }
    Ok(1)
}

#[cfg(feature = "dynmod")]
fn loader_croot(state: &mut LuaState) -> LuaResult<u32> {
    let name = check_string(state, "loader_croot", 0)?;

    let Some(dot_pos) = name.find('.') else {
        return Ok(0);
    };
    let root = &name[..dot_pos];

    let pkg = get_package_table(state)?;
    let cpath_val = get_field(state, pkg, "cpath");
    let cpath = match cpath_val {
        Val::Str(r) => state
            .gc
            .string_arena
            .get(r)
            .map(|s| String::from_utf8_lossy(s.data()).to_string())
            .unwrap_or_default(),
        _ => {
            return Err(simple_error("'package.cpath' must be a string".to_string()));
        }
    };

    let (found, errors) = search_path(&cpath, root);
    let Some(filename) = found else {
        let msg = state.gc.intern_string(errors.as_bytes());
        state.push(Val::Str(msg));
        return Ok(1);
    };

    // For "foo.bar", look up "rilua_open_foo_bar".
    let sym_name = mkfuncname(&name);
    ll_loadfunc(state, &filename, &sym_name)
}

// ---------------------------------------------------------------------------
// require
// ---------------------------------------------------------------------------

/// Implements Lua's `require(modname)`.
///
/// Checks `package.loaded`, iterates loaders, caches results.
///
/// Reference: `ll_require` in `loadlib.c`.
pub fn ll_require(state: &mut LuaState) -> LuaResult<u32> {
    let name = check_string(state, "require", 0)?;
    let pkg = get_package_table(state)?;

    // 1. Get package.loaded table.
    let loaded_val = get_field(state, pkg, "loaded");
    let Val::Table(loaded) = loaded_val else {
        return Err(simple_error("'package.loaded' must be a table".to_string()));
    };

    // 2. Check if already loaded.
    let name_key = state.gc.intern_string(name.as_bytes());
    let cached = {
        let loaded_t = state
            .gc
            .tables
            .get(loaded)
            .ok_or_else(|| simple_error("package.loaded table not found".into()))?;
        loaded_t.get_str(name_key, &state.gc.string_arena)
    };

    if cached.is_truthy() {
        // Check for sentinel (circular require).
        if cached == Val::LightUserdata(SENTINEL) {
            return Err(simple_error(format!(
                "loop or previous error loading module '{name}'"
            )));
        }
        state.push(cached);
        return Ok(1);
    }

    // 3. Get loaders table.
    let loaders_val = get_field(state, pkg, "loaders");
    let Val::Table(loaders) = loaders_val else {
        return Err(simple_error(
            "'package.loaders' must be a table".to_string(),
        ));
    };

    // 4. Set sentinel in package.loaded[name].
    {
        let name_key = state.gc.intern_string(name.as_bytes());
        let loaded_t = state
            .gc
            .tables
            .get_mut(loaded)
            .ok_or_else(|| simple_error("package.loaded table not found".into()))?;
        loaded_t.raw_set(
            Val::Str(name_key),
            Val::LightUserdata(SENTINEL),
            &state.gc.string_arena,
        )?;
    }

    // 5. Iterate loaders.
    let mut error_msg = String::new();
    let found_loader;
    let mut i = 1;

    loop {
        let loader_val = {
            let loaders_t = state
                .gc
                .tables
                .get(loaders)
                .ok_or_else(|| simple_error("package.loaders table not found".into()))?;
            loaders_t.get(Val::Num(f64::from(i)), &state.gc.string_arena)
        };

        if loader_val.is_nil() {
            // No more loaders.
            // Clear sentinel before raising error.
            let name_key = state.gc.intern_string(name.as_bytes());
            let loaded_t = state
                .gc
                .tables
                .get_mut(loaded)
                .ok_or_else(|| simple_error("package.loaded table not found".into()))?;
            loaded_t.raw_set(Val::Str(name_key), Val::Nil, &state.gc.string_arena)?;
            return Err(simple_error(format!(
                "module '{name}' not found:{error_msg}"
            )));
        }

        // Call the loader with the module name.
        let call_base = state.top;
        state.ensure_stack(call_base + 3);
        state.stack_set(call_base, loader_val);
        let name_str = state.gc.intern_string(name.as_bytes());
        state.stack_set(call_base + 1, Val::Str(name_str));
        state.top = call_base + 2;

        state.call_function(call_base, 1)?;

        let result = state.stack_get(call_base);
        state.top = call_base;

        if let Val::Function(_) = result {
            found_loader = result;
            break;
        } else if let Val::Str(r) = result {
            // Accumulate error message.
            if let Some(s) = state.gc.string_arena.get(r) {
                error_msg.push_str(&String::from_utf8_lossy(s.data()));
            }
        }
        // Otherwise skip (PUC-Rio: lua_pop(L, 1)).

        i += 1;
    }

    // 6. Call the found loader with the module name.
    let call_base = state.top;
    state.ensure_stack(call_base + 3);
    state.stack_set(call_base, found_loader);
    let name_str = state.gc.intern_string(name.as_bytes());
    state.stack_set(call_base + 1, Val::Str(name_str));
    state.top = call_base + 2;

    state.call_function(call_base, 1)?;

    let module_result = state.stack_get(call_base);
    state.top = call_base;

    // 7. If non-nil return, set package.loaded[name] = result.
    if !module_result.is_nil() {
        let name_key = state.gc.intern_string(name.as_bytes());
        let loaded_t = state
            .gc
            .tables
            .get_mut(loaded)
            .ok_or_else(|| simple_error("package.loaded table not found".into()))?;
        loaded_t.raw_set(Val::Str(name_key), module_result, &state.gc.string_arena)?;
    }

    // 8. Read package.loaded[name] -- if still sentinel, set to true.
    let name_key = state.gc.intern_string(name.as_bytes());
    let final_val = {
        let loaded_t = state
            .gc
            .tables
            .get(loaded)
            .ok_or_else(|| simple_error("package.loaded table not found".into()))?;
        loaded_t.get_str(name_key, &state.gc.string_arena)
    };

    if final_val == Val::LightUserdata(SENTINEL) {
        let name_key = state.gc.intern_string(name.as_bytes());
        let loaded_t = state
            .gc
            .tables
            .get_mut(loaded)
            .ok_or_else(|| simple_error("package.loaded table not found".into()))?;
        loaded_t.raw_set(Val::Str(name_key), Val::Bool(true), &state.gc.string_arena)?;
        state.push(Val::Bool(true));
    } else {
        state.push(final_val);
    }

    Ok(1)
}

// ---------------------------------------------------------------------------
// module
// ---------------------------------------------------------------------------

/// Implements Lua's `module(name, ...)`.
///
/// Creates/finds a table for the module, initializes `_M`, `_NAME`, `_PACKAGE`
/// fields, sets it as the calling function's environment, and calls any
/// option functions (e.g., `package.seeall`).
///
/// Reference: `ll_module` in `loadlib.c`.
pub fn ll_module(state: &mut LuaState) -> LuaResult<u32> {
    if nargs(state) < 1 {
        return Err(bad_argument("module", 1, "string expected"));
    }
    let name = check_string(state, "module", 0)?;
    let pkg = get_package_table(state)?;

    // Get package.loaded table.
    let loaded_val = get_field(state, pkg, "loaded");
    let Val::Table(loaded) = loaded_val else {
        return Err(simple_error("'package.loaded' must be a table".to_string()));
    };

    // Check if module is already in package.loaded.
    let name_key = state.gc.intern_string(name.as_bytes());
    let existing = {
        let loaded_t = state
            .gc
            .tables
            .get(loaded)
            .ok_or_else(|| simple_error("package.loaded table not found".into()))?;
        loaded_t.get_str(name_key, &state.gc.string_arena)
    };

    let module_table = if let Val::Table(t) = existing {
        t
    } else {
        // Create the module table in the global namespace.
        let t = find_or_create_table(state, state.global, &name)?;
        // Store in package.loaded.
        let name_key = state.gc.intern_string(name.as_bytes());
        let loaded_t = state
            .gc
            .tables
            .get_mut(loaded)
            .ok_or_else(|| simple_error("package.loaded table not found".into()))?;
        loaded_t.raw_set(Val::Str(name_key), Val::Table(t), &state.gc.string_arena)?;
        t
    };

    // Check if the table already has _NAME (already initialized).
    let has_name = {
        let name_key = state.gc.intern_string(b"_NAME");
        let mt = state
            .gc
            .tables
            .get(module_table)
            .ok_or_else(|| simple_error("module table not found".into()))?;
        !mt.get_str(name_key, &state.gc.string_arena).is_nil()
    };

    if !has_name {
        // Initialize _M, _NAME, _PACKAGE.
        set_field(state, module_table, "_M", Val::Table(module_table))?;
        let name_str = state.gc.intern_string(name.as_bytes());
        set_field(state, module_table, "_NAME", Val::Str(name_str))?;

        // _PACKAGE: everything up to and including the last dot,
        // or empty string if no dot.
        let package_name = if let Some(dot_pos) = name.rfind('.') {
            &name[..=dot_pos]
        } else {
            ""
        };
        let pkg_str = state.gc.intern_string(package_name.as_bytes());
        set_field(state, module_table, "_PACKAGE", Val::Str(pkg_str))?;
    }

    // Set as calling function's environment.
    // Walk call stack back to find the Lua closure that called module().
    if state.ci > 0 {
        let caller_ci = state.ci - 1;
        let func_idx = state.call_stack[caller_ci].func;
        let func_val = state.stack_get(func_idx);
        if let Val::Function(closure_ref) = func_val
            && let Some(Closure::Lua(lua_cl)) = state.gc.closures.get_mut(closure_ref)
        {
            lua_cl.env = module_table;
        }
    }

    // Call option functions (extra args after name) with module table.
    let n = nargs(state);
    for i in 1..n {
        let opt_fn = arg(state, i);
        if let Val::Function(_) = opt_fn {
            let call_base = state.top;
            state.ensure_stack(call_base + 3);
            state.stack_set(call_base, opt_fn);
            state.stack_set(call_base + 1, Val::Table(module_table));
            state.top = call_base + 2;

            state.call_function(call_base, 0)?;
            state.top = call_base;
        }
    }

    Ok(0)
}

// ---------------------------------------------------------------------------
// package.seeall
// ---------------------------------------------------------------------------

/// Implements `package.seeall(module)`.
///
/// Sets `module`'s metatable `__index` to `_G`, so global lookups work
/// inside the module.
///
/// Reference: `ll_seeall` in `loadlib.c`.
pub fn ll_seeall(state: &mut LuaState) -> LuaResult<u32> {
    if nargs(state) < 1 {
        return Err(bad_argument("package.seeall", 1, "table expected"));
    }
    let table_val = arg(state, 0);
    let Val::Table(table_ref) = table_val else {
        return Err(bad_argument("package.seeall", 1, "table expected"));
    };

    // Get or create metatable for the module table.
    let mt = {
        let t = state
            .gc
            .tables
            .get(table_ref)
            .ok_or_else(|| simple_error("package.seeall: table not found".into()))?;
        t.metatable()
    };

    let mt_ref = if let Some(mt) = mt {
        mt
    } else {
        let mt = state.gc.alloc_table(Table::new());
        let t = state
            .gc
            .tables
            .get_mut(table_ref)
            .ok_or_else(|| simple_error("package.seeall: table not found".into()))?;
        t.set_metatable(Some(mt));
        mt
    };

    // Set __index = _G.
    let index_key = state.gc.intern_string(b"__index");
    let global = state.global;
    let mt_table = state
        .gc
        .tables
        .get_mut(mt_ref)
        .ok_or_else(|| simple_error("package.seeall: metatable not found".into()))?;
    mt_table.raw_set(
        Val::Str(index_key),
        Val::Table(global),
        &state.gc.string_arena,
    )?;

    Ok(0)
}

// ---------------------------------------------------------------------------
// package.loadlib
// ---------------------------------------------------------------------------

/// Implements `package.loadlib(path, init)`.
///
/// Without the `dynmod` feature, returns `(nil, message, "absent")`.
/// With `dynmod`, loads a rilua-native shared library and returns the
/// requested entry point as a Lua function.
///
/// Reference: `ll_loadlib` in `loadlib.c`.
#[cfg(not(feature = "dynmod"))]
pub fn ll_loadlib(state: &mut LuaState) -> LuaResult<u32> {
    state.push(Val::Nil);
    let msg = state
        .gc
        .intern_string(b"dynamic libraries not enabled; check your Lua installation");
    state.push(Val::Str(msg));
    let absent = state.gc.intern_string(b"absent");
    state.push(Val::Str(absent));
    Ok(3)
}

#[cfg(feature = "dynmod")]
pub fn ll_loadlib(state: &mut LuaState) -> LuaResult<u32> {
    let path = check_string(state, "loadlib", 0)?;
    let funcname = check_string(state, "loadlib", 1)?;

    // Special case: if funcname is "*", just check if library can be loaded.
    if funcname == "*" {
        match crate::platform::dynlib::DynLib::open(&path) {
            Ok(lib) => {
                // Store the handle so it stays loaded.
                store_lib_handle(state, lib)?;
                state.push(Val::Bool(true));
                return Ok(1);
            }
            Err(msg) => {
                return push_loaderror(state, &msg, "open");
            }
        }
    }

    ll_loadfunc(state, &path, &funcname)
}

// ---------------------------------------------------------------------------
// package.searchpath (not in 5.1.1, but we expose search_path for internal use)
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Dynamic module helpers (feature-gated)
// ---------------------------------------------------------------------------

#[cfg(feature = "dynmod")]
use crate::platform::dynlib::DynLib;

/// Registry key for the `_LOADLIB` metatable.
#[cfg(feature = "dynmod")]
const LOADLIB_MT_KEY: &str = "_LOADLIB";

/// Builds the entry point symbol name from a module name.
///
/// Dots are replaced with underscores. If the name contains a `-` (the
/// ignore mark), everything before and including the `-` is stripped.
///
/// Examples:
/// - `"hello"` -> `"rilua_open_hello"`
/// - `"foo.bar"` -> `"rilua_open_foo_bar"`
/// - `"foo-bar.baz"` -> `"rilua_open_bar_baz"` (strips up to `-`)
#[cfg(feature = "dynmod")]
fn mkfuncname(modname: &str) -> String {
    let name = if let Some(pos) = modname.find(LUA_IGMARK) {
        &modname[pos + LUA_IGMARK.len()..]
    } else {
        modname
    };
    format!("rilua_open_{}", name.replace('.', "_"))
}

/// Pushes `(nil, msg, errtype)` onto the stack -- the standard loadlib error
/// return convention. Returns `Ok(3)` for use in `-> LuaResult<u32>` functions.
#[cfg(feature = "dynmod")]
#[allow(clippy::unnecessary_wraps)]
fn push_loaderror(state: &mut LuaState, msg: &str, errtype: &str) -> LuaResult<u32> {
    state.push(Val::Nil);
    let msg_ref = state.gc.intern_string(msg.as_bytes());
    state.push(Val::Str(msg_ref));
    let err_ref = state.gc.intern_string(errtype.as_bytes());
    state.push(Val::Str(err_ref));
    Ok(3)
}

/// Stores a `DynLib` handle as userdata in the registry, keyed by
/// `"LOADLIB: <path>"`. The `_LOADLIB` metatable's `__gc` ensures
/// `dlclose`/`FreeLibrary` runs on GC finalization.
///
/// If a handle for this path already exists, the new one is dropped
/// (deduplicated).
#[cfg(feature = "dynmod")]
fn store_lib_handle(state: &mut LuaState, lib: DynLib) -> LuaResult<()> {
    let reg_key = format!("LOADLIB: {}", lib.path());
    let key_ref = state.gc.intern_string(reg_key.as_bytes());

    // Check if already loaded.
    let registry = state.registry;
    let existing = {
        let reg = state
            .gc
            .tables
            .get(registry)
            .ok_or_else(|| simple_error("registry not found".into()))?;
        reg.get_str(key_ref, &state.gc.string_arena)
    };
    if !existing.is_nil() {
        // Already loaded -- drop the new handle (DynLib::drop calls dlclose).
        return Ok(());
    }

    // Get or create the _LOADLIB metatable.
    let mt = super::new_metatable(state, LOADLIB_MT_KEY)?;

    // Create userdata wrapping the DynLib.
    let ud = crate::vm::value::Userdata::with_metatable(Box::new(lib), mt);
    let ud_ref = state.gc.alloc_userdata(ud);

    // Store in registry.
    let reg = state
        .gc
        .tables
        .get_mut(registry)
        .ok_or_else(|| simple_error("registry not found".into()))?;
    reg.raw_set(
        Val::Str(key_ref),
        Val::Userdata(ud_ref),
        &state.gc.string_arena,
    )?;

    Ok(())
}

/// Shared logic for `ll_loadlib` and `loader_c`/`loader_croot`.
///
/// Loads the shared library at `path`, validates the module info, looks up
/// the `sym_name` symbol, wraps it as a `RustClosure`, and pushes it onto
/// the stack. On error, pushes `(nil, msg, errtype)`.
#[cfg(feature = "dynmod")]
fn ll_loadfunc(state: &mut LuaState, path: &str, sym_name: &str) -> LuaResult<u32> {
    // 1. Load the library.
    let lib = match DynLib::open(path) {
        Ok(lib) => lib,
        Err(msg) => return push_loaderror(state, &msg, "open"),
    };

    // 2. Validate module info.
    if let Err(msg) = crate::dynmod::validate_module_info(&lib) {
        return push_loaderror(state, &msg, "open");
    }

    // 3. Look up the entry point.
    let entry = match crate::dynmod::load_entry_point(&lib, sym_name) {
        Ok(entry) => entry,
        Err(msg) => {
            // Store the lib handle even on init failure (PUC-Rio behavior).
            store_lib_handle(state, lib)?;
            return push_loaderror(state, &msg, "init");
        }
    };

    // 4. Store the library handle to prevent dlclose.
    store_lib_handle(state, lib)?;

    // 5. Wrap the entry point in a RustClosure and push it.
    make_module_wrapper(state, entry)
}

/// Creates a `RustClosure` that wraps a [`RiluaModuleEntry`] function pointer.
///
/// The raw function pointer is stored as a `LightUserdata` upvalue. The
/// wrapper reads it back, casts to `RiluaModuleEntry`, and calls it with
/// `catch_unwind`.
#[cfg(feature = "dynmod")]
#[allow(unsafe_code, clippy::unnecessary_wraps)]
fn make_module_wrapper(
    state: &mut LuaState,
    entry: crate::dynmod::RiluaModuleEntry,
) -> LuaResult<u32> {
    // Store the function pointer as a lightuserdata upvalue.
    let ptr_val = Val::LightUserdata(entry as usize);

    let wrapper: crate::vm::closure::RustFn = |state: &mut LuaState| {
        // Read the function pointer from upvalue[0].
        let ci = &state.call_stack[state.ci];
        let func_val = state.stack_get(ci.func);
        let Val::Function(closure_ref) = func_val else {
            return Err(simple_error("dynmod wrapper: not a function".into()));
        };
        let cl = state
            .gc
            .closures
            .get(closure_ref)
            .ok_or_else(|| simple_error("dynmod wrapper: closure not found".into()))?;
        let uv = match cl {
            Closure::Rust(rc) => rc.upvalues.first().copied().unwrap_or(Val::Nil),
            Closure::Lua(_) => {
                return Err(simple_error("dynmod wrapper: expected Rust closure".into()));
            }
        };
        let Val::LightUserdata(ptr) = uv else {
            return Err(simple_error("dynmod wrapper: bad upvalue".into()));
        };

        let entry: crate::dynmod::RiluaModuleEntry = unsafe { std::mem::transmute(ptr) };

        match crate::dynmod::call_entry_point(entry, state) {
            Ok(n) => Ok(n as u32),
            Err(msg) => Err(simple_error(msg)),
        }
    };

    let closure = Closure::Rust(RustClosure {
        func: wrapper,
        upvalues: vec![ptr_val],
        name: "dynmod_entry".to_string(),
        env: None,
        native: 0,
    });
    let closure_ref = state.gc.alloc_closure(closure);
    state.push(Val::Function(closure_ref));
    Ok(1)
}

/// `__gc` metamethod for `_LOADLIB` userdata.
///
/// The `DynLib` stored inside the userdata calls `dlclose`/`FreeLibrary`
/// via its `Drop` impl. This function is a no-op since Rust drops the
/// `Box<dyn Any>` (and thus the `DynLib`) when the userdata is collected.
/// However, we register it to ensure the `_LOADLIB` metatable has `__gc`,
/// which is the signal to the GC that finalization is needed.
#[cfg(feature = "dynmod")]
#[allow(clippy::unnecessary_wraps)]
fn loadlib_gc(_state: &mut LuaState) -> LuaResult<u32> {
    // DynLib::drop handles dlclose automatically. No extra work needed.
    Ok(0)
}

/// Creates the `_LOADLIB` metatable in the registry with a `__gc` method.
#[cfg(feature = "dynmod")]
fn create_loadlib_metatable(state: &mut LuaState) -> LuaResult<()> {
    let mt = super::new_metatable(state, LOADLIB_MT_KEY)?;
    super::register_table_fn(state, mt, "__gc", loadlib_gc)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

/// Helper: create a RustClosure with upvalues and register it in a table.
fn register_table_fn_with_upvalues(
    state: &mut LuaState,
    table_ref: GcRef<Table>,
    name: &str,
    func: crate::vm::closure::RustFn,
    upvalues: Vec<Val>,
) -> LuaResult<()> {
    let closure = Closure::Rust(RustClosure {
        func,
        upvalues,
        name: name.to_string(),
        env: None,
        native: 0,
    });
    let closure_ref = state.gc.alloc_closure(closure);

    let key_ref = state.gc.intern_string(name.as_bytes());
    let key = Val::Str(key_ref);
    let val = Val::Function(closure_ref);

    let table = state
        .gc
        .tables
        .get_mut(table_ref)
        .ok_or_else(|| simple_error("table not found for function registration".into()))?;
    table.raw_set(key, val, &state.gc.string_arena)?;

    Ok(())
}

/// Helper: create a RustClosure with upvalues and register it as a global.
fn register_global_fn_with_upvalues(
    state: &mut LuaState,
    name: &str,
    func: crate::vm::closure::RustFn,
    upvalues: Vec<Val>,
) -> LuaResult<()> {
    let global = state.global;
    register_table_fn_with_upvalues(state, global, name, func, upvalues)
}

/// Opens the package library.
///
/// Creates the `package` table with all fields and functions, registers
/// `require` and `module` as globals, and pre-populates `package.loaded`
/// with already-opened libraries.
///
/// Must be called **after** all other standard libraries are opened,
/// because it populates `package.loaded` with existing globals.
///
/// Reference: `luaopen_package` in `loadlib.c`.
pub fn open_package_lib(state: &mut LuaState) -> LuaResult<()> {
    let pkg_table = state.gc.alloc_table(Table::new());
    let pkg_val = Val::Table(pkg_table);

    // Create _LOADLIB metatable (dynmod only, must be before loadlib registration).
    #[cfg(feature = "dynmod")]
    create_loadlib_metatable(state)?;

    // Register simple functions (no upvalue needed).
    super::register_table_fn(state, pkg_table, "loadlib", ll_loadlib)?;
    super::register_table_fn(state, pkg_table, "seeall", ll_seeall)?;

    // Create loaders table with 4 loader closures, each with pkg_table upvalue.
    let loaders_table = state.gc.alloc_table(Table::new());
    let loader_fns: &[(&str, crate::vm::closure::RustFn)] = &[
        ("loader_preload", loader_preload),
        ("loader_lua", loader_lua),
        ("loader_c", loader_c),
        ("loader_croot", loader_croot),
    ];

    for (i, &(name, func)) in loader_fns.iter().enumerate() {
        let closure = Closure::Rust(RustClosure {
            func,
            upvalues: vec![pkg_val],
            name: name.to_string(),
            env: None,
            native: 0,
        });
        let closure_ref = state.gc.alloc_closure(closure);

        let loaders_t = state
            .gc
            .tables
            .get_mut(loaders_table)
            .ok_or_else(|| simple_error("loaders table not found".into()))?;
        #[allow(clippy::cast_precision_loss)]
        loaders_t.raw_set(
            Val::Num((i + 1) as f64),
            Val::Function(closure_ref),
            &state.gc.string_arena,
        )?;
    }

    set_field(state, pkg_table, "loaders", Val::Table(loaders_table))?;

    // Set paths.
    set_path(state, pkg_table, "path", "LUA_PATH", LUA_PATH_DEFAULT)?;
    set_path(state, pkg_table, "cpath", "LUA_CPATH", LUA_CPATH_DEFAULT)?;

    // package.config: 5 lines.
    let config_str =
        format!("{LUA_DIRSEP}\n{LUA_PATHSEP}\n{LUA_PATH_MARK}\n{LUA_EXECDIR}\n{LUA_IGMARK}\n");
    let config_ref = state.gc.intern_string(config_str.as_bytes());
    set_field(state, pkg_table, "config", Val::Str(config_ref))?;

    // Create package.loaded table and store in registry as "_LOADED".
    let loaded_table = state.gc.alloc_table(Table::new());
    set_field(state, pkg_table, "loaded", Val::Table(loaded_table))?;
    // Also put in registry.
    {
        let key_ref = state.gc.intern_string(LOADED_KEY.as_bytes());
        let registry = state.registry;
        let reg_t = state
            .gc
            .tables
            .get_mut(registry)
            .ok_or_else(|| simple_error("registry not found".into()))?;
        reg_t.raw_set(
            Val::Str(key_ref),
            Val::Table(loaded_table),
            &state.gc.string_arena,
        )?;
    }

    // Create package.preload table.
    let preload_table = state.gc.alloc_table(Table::new());
    set_field(state, pkg_table, "preload", Val::Table(preload_table))?;

    // Register `require` and `module` as globals with pkg_table upvalue.
    register_global_fn_with_upvalues(state, "require", ll_require, vec![pkg_val])?;
    register_global_fn_with_upvalues(state, "module", ll_module, vec![pkg_val])?;

    // Register package as global.
    super::register_global_val(state, "package", pkg_val)?;

    // Pre-populate package.loaded with already-opened libraries.
    // Look up known library globals and store them.
    let lib_names = [
        "string",
        "table",
        "math",
        "os",
        "io",
        "coroutine",
        "debug",
        "package",
    ];
    for lib_name in &lib_names {
        let key_ref = state.gc.intern_string(lib_name.as_bytes());
        let global_t = state
            .gc
            .tables
            .get(state.global)
            .ok_or_else(|| simple_error("global table not found".into()))?;
        let val = global_t.get_str(key_ref, &state.gc.string_arena);
        if !val.is_nil() {
            let loaded_t = state
                .gc
                .tables
                .get_mut(loaded_table)
                .ok_or_else(|| simple_error("loaded table not found".into()))?;
            loaded_t.raw_set(Val::Str(key_ref), val, &state.gc.string_arena)?;
        }
    }

    Ok(())
}
