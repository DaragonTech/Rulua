//! rilua — Lua 5.1.1 implemented in Rust.
//!
//! A from-scratch implementation targeting behavioral equivalence with
//! the PUC-Rio reference interpreter. Designed for embedding in Rust
//! applications, with a focus on the World of Warcraft addon variant.
//!
//! # Architecture
//!
//! Pipeline: Source -> Lexer -> Parser -> AST -> Compiler -> Proto -> VM
//!
//! See `docs/src/architecture.md` for design documentation.
//!
//! # Usage
//!
//! ```rust
//! use rilua::{Lua, LuaApiMut};
//!
//! let mut lua = Lua::new().unwrap();
//! lua.exec("print(1 + 2)").unwrap();
//!
//! lua.set_global("x", 42.0).unwrap();
//! let x: f64 = lua.global("x").unwrap();
//! assert_eq!(x, 42.0);
//! ```

pub mod api;
pub mod compiler;
pub mod conversion;
#[cfg(feature = "dynmod")]
pub mod dynmod;
pub mod error;
pub mod handles;
pub(crate) mod platform;
pub mod stdlib;
pub mod vm;

// Re-exports for public API.
pub use api::{LuaApi, LuaApiMut};
pub use conversion::{FromLua, FromLuaMulti, IntoLua, IntoLuaMulti};
pub use error::{LuaError, LuaResult, RuntimeError, runtime_error};
pub use handles::{AnyUserData, Function, Table, Thread};
pub use stdlib::StdLib;
pub use vm::closure::RustFn;
pub use vm::proto::ProtoRef;
pub use vm::state::ThreadStatus;
pub use vm::value::Val;

use vm::callinfo::LUA_MULTRET;
use vm::closure::{Closure, LuaClosure};
use vm::execute::{CallResult, execute};
use vm::proto::Proto;
use vm::state::{Gc, LuaState};

// ---------------------------------------------------------------------------
// Interrupt flag — cross-platform, WASM-safe
// ---------------------------------------------------------------------------

use std::sync::atomic::{AtomicBool, Ordering};

/// Global interrupt flag set by the embedder's signal handler.
///
/// The VM checks this in the execute loop and raises a runtime error
/// when set. `AtomicBool` is async-signal-safe on all platforms.
static INTERRUPTED: AtomicBool = AtomicBool::new(false);

/// Sets the interrupt flag.
///
/// Call this from a signal handler or other external interrupt source.
/// The VM will check the flag on the next instruction dispatch and
/// raise a runtime error.
pub fn set_interrupted() {
    INTERRUPTED.store(true, Ordering::Relaxed);
}

/// Clears the interrupt flag.
///
/// Call this before starting execution to ensure a stale flag from a
/// previous run does not immediately trigger an error.
pub fn clear_interrupted() {
    INTERRUPTED.store(false, Ordering::Relaxed);
}

/// Checks and auto-clears the interrupt flag.
///
/// Returns `true` if the flag was set, and atomically clears it.
/// The clear-on-read avoids repeated interrupts from a single Ctrl+C.
pub(crate) fn check_interrupted() -> bool {
    // Fast path: a relaxed load (mov on x86, ldr on ARM) costs ~1 cycle.
    // Only pay for the store on the rare true path.
    if INTERRUPTED.load(Ordering::Relaxed) {
        INTERRUPTED.store(false, Ordering::Relaxed);
        return true;
    }
    false
}

// ---------------------------------------------------------------------------
// Lua struct: high-level embedding API
// ---------------------------------------------------------------------------

/// A Lua interpreter instance.
///
/// Owns the full VM state including value stack, call stack, GC heap,
/// and global table. All interaction with Lua values goes through this
/// struct.
pub struct Lua {
    state: LuaState,
}

impl LuaApi for Lua {
    fn state(&self) -> &LuaState {
        &self.state
    }
}

impl LuaApiMut for Lua {
    fn state_mut(&mut self) -> &mut LuaState {
        &mut self.state
    }
}

// SAFETY: With the `send` feature enabled, all fields of `LuaState` are
// `Send`-safe:
// - `ProtoRef` is `Arc<Proto>` (Send + Sync)
// - `UserDataBox` is `Box<dyn Any + Send>` (Send)
// - `GcRef<T>` is two `u32` indices + PhantomData (Send)
// - `Gc` contains `Arena<T>` (Vec of owned data) and `StringTable` (HashMap)
// - `IoFile` has `unsafe impl Send` (FILE* can be transferred between threads)
// - All other fields are primitives, `Vec<Val>`, `Vec<CallInfo>`, etc.
//
// `Lua` requires `&mut self` for all operations, preventing concurrent access.
// This impl enables embedding rilua in frameworks (e.g., bevy_mod_scripting)
// that store script contexts behind a Mutex.
#[cfg(feature = "send")]
#[allow(unsafe_code)]
unsafe impl Send for Lua {}

impl Lua {
    /// Creates a new Lua state with all standard libraries loaded.
    pub fn new() -> LuaResult<Self> {
        let mut lua = Self {
            state: LuaState::new(),
        };
        stdlib::open_libs(&mut lua.state)?;
        Ok(lua)
    }

    /// Creates a new Lua state with no libraries loaded.
    ///
    /// The state has a global table and registry but no standard
    /// functions (`print`, `type`, etc.) are available.
    pub fn new_empty() -> Self {
        Self {
            state: LuaState::new(),
        }
    }

    /// Creates a new Lua state with selected standard libraries.
    ///
    /// ```rust
    /// use rilua::{Lua, StdLib};
    ///
    /// let lua = Lua::new_with(StdLib::BASE | StdLib::STRING).unwrap();
    /// ```
    pub fn new_with(libs: StdLib) -> LuaResult<Self> {
        let mut lua = Self {
            state: LuaState::new(),
        };
        stdlib::open_libs_selective(&mut lua.state, libs)?;
        Ok(lua)
    }

    // -----------------------------------------------------------------------
    // Execution
    // -----------------------------------------------------------------------

    /// Compiles and executes a Lua source string.
    ///
    /// The chunk name is set to `"=(string)"`.
    pub fn exec(&mut self, source: &str) -> LuaResult<()> {
        self.exec_bytes(source.as_bytes(), "=(string)")
    }

    /// Compiles and executes Lua source bytes with a given chunk name.
    ///
    /// Source is `&[u8]` because Lua files may contain arbitrary byte
    /// sequences (e.g. `\0`, `\255` in string literals).
    pub fn exec_bytes(&mut self, source: &[u8], name: &str) -> LuaResult<()> {
        let proto = compile_or_undump(source, name)?;
        self.run_proto(proto)
    }

    /// Reads a file and executes its contents as a Lua chunk.
    ///
    /// The chunk name is set to `@<path>` following PUC-Rio convention.
    pub fn exec_file(&mut self, path: &str) -> LuaResult<()> {
        let source = std::fs::read(path).map_err(|e| {
            LuaError::Runtime(RuntimeError {
                message: format!("cannot open {path}: {e}"),
                level: 0,
                traceback: vec![],
            })
        })?;
        let name = format!("@{path}");
        self.exec_bytes(&source, &name)
    }

    // -----------------------------------------------------------------------
    // Calling loaded functions
    // -----------------------------------------------------------------------

    /// Calls a loaded `Function` handle with arguments and returns results.
    ///
    /// Sets up the stack: push function, push args, precall, execute,
    /// collect results. Used by the REPL to execute loaded chunks and
    /// print results, and by `-l` to call `require`.
    pub fn call_function(&mut self, func: &Function, args: &[Val]) -> LuaResult<Vec<Val>> {
        // Push the function at the current top.
        let func_idx = self.state.top;
        self.state.ensure_stack(func_idx + 1 + args.len());
        self.state.stack_set(func_idx, Val::Function(func.0));
        self.state.top = func_idx + 1;

        // Push arguments.
        for arg in args {
            let top = self.state.top;
            self.state.stack_set(top, *arg);
            self.state.top = top + 1;
        }

        // Save the base index so we know where results land.
        let save_base = self.state.base;
        self.state.base = func_idx + 1;

        // Call with LUA_MULTRET to get all results.
        match self.state.precall(func_idx, LUA_MULTRET)? {
            CallResult::Lua => execute(&mut self.state)?,
            CallResult::Rust => {}
        }

        // Collect results: they're at func_idx..self.state.top.
        let results: Vec<Val> = (func_idx..self.state.top)
            .map(|i| self.state.stack_get(i))
            .collect();

        // Restore state.
        self.state.top = func_idx;
        self.state.base = save_base;

        Ok(results)
    }

    /// Calls a loaded `Function` handle, appending a stack traceback on error.
    ///
    /// Identical to `call_function` but on runtime error, generates a
    /// `debug.traceback`-style stack trace and appends it to the error
    /// message. Used by the CLI to match PUC-Rio's `docall` pattern
    /// where a C traceback function is the `lua_pcall` error handler.
    ///
    /// Because rilua uses `Result`-based errors (no `longjmp`), the call
    /// stack is still intact after an error, so we generate the traceback
    /// after the fact instead of through an error handler function.
    pub fn call_function_traced(&mut self, func: &Function, args: &[Val]) -> LuaResult<Vec<Val>> {
        // Push the function at the current top.
        let func_idx = self.state.top;
        self.state.ensure_stack(func_idx + 1 + args.len());
        self.state.stack_set(func_idx, Val::Function(func.0));
        self.state.top = func_idx + 1;

        // Push arguments.
        for arg in args {
            let top = self.state.top;
            self.state.stack_set(top, *arg);
            self.state.top = top + 1;
        }

        // Save the base index so we know where results land.
        let save_base = self.state.base;
        let save_ci = self.state.ci;
        self.state.base = func_idx + 1;

        // Call with LUA_MULTRET to get all results.
        let result = match self.state.precall(func_idx, LUA_MULTRET) {
            Ok(CallResult::Lua) => execute(&mut self.state),
            Ok(CallResult::Rust) => Ok(()),
            Err(e) => Err(e),
        };

        match result {
            Ok(()) => {
                // Collect results: they're at func_idx..self.state.top.
                let results: Vec<Val> = (func_idx..self.state.top)
                    .map(|i| self.state.stack_get(i))
                    .collect();

                // Restore state.
                self.state.top = func_idx;
                self.state.base = save_base;

                Ok(results)
            }
            Err(e) => {
                // Generate traceback while the call stack is still intact.
                let msg = e.to_string();
                let traceback = stdlib::debug::generate_traceback(&self.state, &msg, 0);

                // Restore state.
                self.state.top = func_idx;
                self.state.base = save_base;
                self.state.ci = save_ci;

                Err(LuaError::Runtime(RuntimeError {
                    message: traceback,
                    level: 0,
                    traceback: vec![],
                }))
            }
        }
    }

    // -----------------------------------------------------------------------
    // File loading
    // -----------------------------------------------------------------------

    /// Reads a file (or stdin if `None`) and compiles it, returning a
    /// function handle.
    ///
    /// The chunk name is set to `@path` for files, or `=stdin` for stdin.
    /// Handles the shebang line (`#!`) that may appear in executable
    /// Lua scripts.
    pub fn load_file(&mut self, path: Option<&str>) -> LuaResult<Function> {
        let (source, name) = if let Some(p) = path {
            let bytes = std::fs::read(p).map_err(|e| {
                LuaError::Runtime(RuntimeError {
                    message: format!("cannot open {p}: {e}"),
                    level: 0,
                    traceback: vec![],
                })
            })?;
            let name = format!("@{p}");
            (bytes, name)
        } else {
            use std::io::Read;
            let mut bytes = Vec::new();
            std::io::stdin().read_to_end(&mut bytes).map_err(|e| {
                LuaError::Runtime(RuntimeError {
                    message: format!("cannot read stdin: {e}"),
                    level: 0,
                    traceback: vec![],
                })
            })?;
            (bytes, "=stdin".to_string())
        };
        self.load_bytes(&source, &name)
    }

    /// Returns the next key-value pair after `key` in the table.
    ///
    /// Pass `Val::Nil` to start iteration. Returns `None` when the
    /// table is exhausted. This mirrors PUC-Rio's `lua_next`.
    pub fn table_next(&self, table: &Table, key: Val) -> LuaResult<Option<(Val, Val)>> {
        let t = self.state.gc.tables.get(table.0).ok_or_else(|| {
            LuaError::Runtime(RuntimeError {
                message: "table has been collected".into(),
                level: 0,
                traceback: vec![],
            })
        })?;
        t.next(key, &self.state.gc.string_arena)
    }

    /// Returns the byte content of a `Val::Str`.
    ///
    /// Returns `None` if the value is not a string or the string has
    /// been collected.
    pub fn val_as_bytes(&self, val: Val) -> Option<&[u8]> {
        if let Val::Str(str_ref) = val {
            self.state
                .gc
                .string_arena
                .get(str_ref)
                .map(vm::string::LuaString::data)
        } else {
            None
        }
    }

    /// Borrows a typed value from a userdata handle.
    ///
    /// Returns `None` if the stored type is not `T` or the userdata has
    /// been collected.
    pub fn borrow_userdata<T: std::any::Any>(&self, ud: &AnyUserData) -> Option<&T> {
        ud.borrow::<T>(&self.state)
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    /// Patches string constants, creates a main closure, and executes it.
    fn run_proto(&mut self, proto: ProtoRef) -> LuaResult<()> {
        let mut proto = ProtoRef::try_unwrap(proto).unwrap_or_else(|rc| (*rc).clone());
        patch_string_constants(&mut proto, &mut self.state.gc);
        let proto = ProtoRef::new(proto);

        let num_upvalues = proto.num_upvalues as usize;
        let mut lua_cl = LuaClosure::new(proto, self.state.global);
        // Binary-loaded protos may have upvalues; create fresh nil slots.
        for _ in 0..num_upvalues {
            let uv = vm::closure::Upvalue::new_closed(Val::Nil);
            let uv_ref = self.state.gc.alloc_upvalue(uv);
            lua_cl.upvalues.push(uv_ref);
        }
        let closure_ref = self.state.gc.alloc_closure(Closure::Lua(lua_cl));

        self.state.stack_set(0, Val::Function(closure_ref));
        self.state.top = 1;
        self.state.base = 1;

        match self.state.precall(0, LUA_MULTRET)? {
            CallResult::Lua => execute(&mut self.state)?,
            CallResult::Rust => {}
        }

        Ok(())
    }

    /// Reads a value from the global table by name.
    ///
    /// Uses `intern_string` (idempotent) to get the key reference, then
    /// does a direct table lookup.
    #[allow(dead_code)]
    fn get_global_val(&mut self, name: &str) -> Val {
        let key_ref = self.state.gc.intern_string(name.as_bytes());
        let Some(global_table) = self.state.gc.tables.get(self.state.global) else {
            return Val::Nil;
        };
        global_table.get_str(key_ref, &self.state.gc.string_arena)
    }

    /// Sets a value in the global table by name.
    #[allow(dead_code)]
    fn set_global_val(&mut self, name: &str, val: Val) -> LuaResult<()> {
        let key_ref = self.state.gc.intern_string(name.as_bytes());
        let key = Val::Str(key_ref);
        let global = self.state.global;
        let table = self.state.gc.tables.get_mut(global).ok_or_else(|| {
            LuaError::Runtime(RuntimeError {
                message: "global table not found".into(),
                level: 0,
                traceback: vec![],
            })
        })?;
        table.raw_set(key, val, &self.state.gc.string_arena)
    }
}

// ---------------------------------------------------------------------------
// Backward-compatible free functions
// ---------------------------------------------------------------------------

/// Executes Lua source bytes as `"=(string)"`.
///
/// Compiles the source, registers the standard library, and runs the
/// resulting chunk. Equivalent to `exec_with_name(source, "=(string)")`.
pub fn exec(source: &[u8]) -> LuaResult<()> {
    exec_with_name(source, "=(string)")
}

/// Executes Lua source bytes with the given chunk name.
///
/// Source is accepted as `&[u8]` because Lua files may contain arbitrary
/// byte sequences (e.g. `\0`, `\255` in string literals).
///
/// Pipeline: compile -> patch string constants -> create state ->
/// register stdlib -> create main closure -> precall -> execute.
pub fn exec_with_name(source: &[u8], name: &str) -> LuaResult<()> {
    let mut lua = Lua::new()?;
    lua.exec_bytes(source, name)
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Resolves string constant placeholders in a Proto using the GC.
///
/// During compilation, string constants are stored as `Val::Nil` with
/// Compiles source code or loads a precompiled binary chunk.
///
/// Detects the `\x1bLua` signature and dispatches to `undump` for binary
/// chunks or `compile` for source text. Both return an unpatched Proto
/// (strings in `string_pool`, `Val::Nil` placeholders).
pub(crate) fn compile_or_undump(source: &[u8], name: &str) -> LuaResult<ProtoRef> {
    // Skip shebang line before checking for binary signature.
    // PUC-Rio's luaL_loadfile reads past the leading '#' line, then
    // checks if the remaining content starts with LUA_SIGNATURE.
    let data = if source.first() == Some(&b'#') {
        // Find end of first line.
        match source.iter().position(|&b| b == b'\n') {
            Some(pos) => &source[pos + 1..],
            None => &[], // Only a shebang line, no content.
        }
    } else {
        source
    };
    if data.starts_with(vm::dump::LUA_SIGNATURE) {
        vm::undump::undump(data, name)
    } else {
        // Pass the original source (with shebang) to the compiler.
        // The lexer handles shebang stripping internally.
        compiler::compile(source, name)
    }
}

/// their raw bytes recorded in `proto.string_pool`. This function
/// interns each string via the GC and replaces the placeholder with
/// the real `Val::Str` value. Recurses into child protos.
pub(crate) fn patch_string_constants(proto: &mut Proto, gc: &mut Gc) {
    for (idx, bytes) in proto.string_pool.drain(..) {
        let str_ref = gc.intern_string(&bytes);
        proto.constants[idx as usize] = Val::Str(str_ref);
    }
    for child in &mut proto.protos {
        patch_string_constants(ProtoRef::make_mut(child), gc);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lua_new_creates_working_state() {
        let lua = Lua::new();
        assert!(lua.is_ok());
    }

    #[test]
    fn lua_new_empty_has_no_libs() {
        let mut lua = Lua::new_empty();
        // `print` should not be defined.
        let val: LuaResult<Val> = lua.global("print");
        assert!(val.is_ok());
        assert_eq!(val.ok(), Some(Val::Nil));
    }

    #[test]
    fn lua_new_with_selective() {
        let lua = Lua::new_with(StdLib::BASE | StdLib::STRING);
        assert!(lua.is_ok());
    }

    #[test]
    fn lua_exec_string() {
        let mut lua = Lua::new().ok().unwrap_or_else(Lua::new_empty);
        let result = lua.exec("local x = 1 + 2");
        assert!(result.is_ok());
    }

    #[test]
    fn lua_exec_syntax_error() {
        let mut lua = Lua::new().ok().unwrap_or_else(Lua::new_empty);
        let result = lua.exec("if then end");
        assert!(result.is_err());
    }

    #[test]
    fn lua_set_and_get_global_f64() {
        let mut lua = Lua::new_empty();
        lua.set_global("x", 42.0f64).ok();
        let val: LuaResult<f64> = lua.global("x");
        assert_eq!(val.ok(), Some(42.0));
    }

    #[test]
    fn lua_set_and_get_global_string() {
        let mut lua = Lua::new_empty();
        lua.set_global("name", "hello").ok();
        let val: LuaResult<String> = lua.global("name");
        assert_eq!(val.ok(), Some("hello".to_string()));
    }

    #[test]
    fn lua_set_and_get_global_bool() {
        let mut lua = Lua::new_empty();
        lua.set_global("flag", true).ok();
        let val: LuaResult<bool> = lua.global("flag");
        assert_eq!(val.ok(), Some(true));
    }

    #[test]
    fn lua_set_and_get_global_nil() {
        let mut lua = Lua::new_empty();
        lua.set_global("x", 42.0f64).ok();
        lua.set_global::<Option<f64>>("x", None).ok();
        let val: LuaResult<Option<f64>> = lua.global("x");
        assert_eq!(val.ok(), Some(None));
    }

    #[test]
    fn lua_create_table() {
        let mut lua = Lua::new_empty();
        let t = lua.create_table();
        t.raw_set(&mut lua.state, Val::Num(1.0), Val::Num(10.0))
            .ok();
        let v = t.raw_get(&lua.state, Val::Num(1.0));
        assert_eq!(v.ok(), Some(Val::Num(10.0)));
    }

    #[test]
    fn lua_register_function() {
        let mut lua = Lua::new_empty();
        let result = lua.register_function("myfn", |state| {
            state.push(Val::Num(99.0));
            Ok(1)
        });
        assert!(result.is_ok());
        let val: LuaResult<Val> = lua.global("myfn");
        assert!(matches!(val.ok(), Some(Val::Function(_))));
    }

    #[test]
    fn lua_gc_methods() {
        let mut lua = Lua::new_empty();
        let count = lua.gc_count();
        assert!(count > 0);

        lua.gc_stop();
        assert_eq!(lua.state.gc.gc_state.gc_threshold, usize::MAX);

        lua.gc_restart();
        assert_eq!(
            lua.state.gc.gc_state.gc_threshold,
            lua.state.gc.gc_state.total_bytes
        );

        let old_pause = lua.gc_set_pause(300);
        assert_eq!(old_pause, 200); // default
        assert_eq!(lua.state.gc.gc_state.gc_pause, 300);

        let old_mul = lua.gc_set_step_multiplier(400);
        assert_eq!(old_mul, 200); // default
        assert_eq!(lua.state.gc.gc_state.gc_stepmul, 400);
    }

    #[test]
    fn lua_gc_collect() {
        let mut lua = Lua::new_empty();
        let result = lua.gc_collect();
        assert!(result.is_ok());
    }

    #[test]
    fn lua_load_and_check() {
        let mut lua = Lua::new_empty();
        let func = lua.load("return 42");
        assert!(func.is_ok());
        assert!(matches!(func.ok(), Some(Function(_))));
    }

    #[test]
    fn lua_call_function_returns_results() {
        let mut lua = Lua::new().ok().unwrap_or_else(Lua::new_empty);
        let func = lua.load("return 1, 2, 3").ok();
        assert!(func.is_some());
        let func = func.unwrap_or_else(|| unreachable!());
        let results = lua.call_function(&func, &[]);
        assert!(results.is_ok());
        let results = results.unwrap_or_default();
        assert_eq!(results.len(), 3);
        assert_eq!(results[0], Val::Num(1.0));
        assert_eq!(results[1], Val::Num(2.0));
        assert_eq!(results[2], Val::Num(3.0));
    }

    #[test]
    fn lua_call_function_with_args() {
        let mut lua = Lua::new().ok().unwrap_or_else(Lua::new_empty);
        let func = lua.load("return select('#', ...)").ok();
        assert!(func.is_some());
        let func = func.unwrap_or_else(|| unreachable!());
        let results = lua.call_function(&func, &[Val::Num(10.0), Val::Num(20.0)]);
        assert!(results.is_ok());
        let results = results.unwrap_or_default();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], Val::Num(2.0));
    }

    #[test]
    fn lua_call_function_no_results() {
        let mut lua = Lua::new().ok().unwrap_or_else(Lua::new_empty);
        let func = lua.load("local x = 1").ok();
        assert!(func.is_some());
        let func = func.unwrap_or_else(|| unreachable!());
        let results = lua.call_function(&func, &[]);
        assert!(results.is_ok());
        let results = results.unwrap_or_default();
        assert!(results.is_empty());
    }

    #[test]
    fn lua_create_string() {
        let mut lua = Lua::new_empty();
        let val = lua.create_string(b"hello");
        assert!(matches!(val, Val::Str(_)));
        // Create same string again: should return same reference.
        let val2 = lua.create_string(b"hello");
        assert_eq!(val, val2);
    }

    #[test]
    fn lua_table_raw_set_via_api() {
        let mut lua = Lua::new_empty();
        let t = lua.create_table();
        let result = lua.table_raw_set(&t, Val::Num(1.0), Val::Num(42.0));
        assert!(result.is_ok());
        let v = t.raw_get(&lua.state, Val::Num(1.0));
        assert_eq!(v.ok(), Some(Val::Num(42.0)));
    }

    #[test]
    fn lua_load_file_nonexistent() {
        let mut lua = Lua::new_empty();
        let result = lua.load_file(Some("/nonexistent/path/to/file.lua"));
        assert!(result.is_err());
    }

    // -- Userdata API --

    #[test]
    fn lua_create_userdata() {
        let mut lua = Lua::new_empty();
        let ud = lua.create_userdata(42i64);
        let val = ud.borrow::<i64>(&lua.state);
        assert_eq!(val, Some(&42i64));
    }

    #[test]
    fn lua_create_userdata_type_mismatch() {
        let mut lua = Lua::new_empty();
        let ud = lua.create_userdata(42i64);
        assert!(ud.borrow::<String>(&lua.state).is_none());
    }

    #[test]
    fn lua_create_typed_userdata_with_metatable() {
        let mut lua = Lua::new_empty();
        let ud = lua.create_typed_userdata(100u32, "MyType");
        assert!(ud.is_ok());
        let ud = ud.unwrap_or_else(|_| unreachable!());
        // Metatable should be set.
        let mt = ud.metatable(&lua.state);
        assert!(mt.is_some());
    }

    #[test]
    fn lua_userdata_metatable_caching() {
        let mut lua = Lua::new_empty();
        let mt1 = lua.create_userdata_metatable("CachedType");
        assert!(mt1.is_ok());
        let mt1 = mt1.unwrap_or_else(|_| unreachable!());

        let mt2 = lua.create_userdata_metatable("CachedType");
        assert!(mt2.is_ok());
        let mt2 = mt2.unwrap_or_else(|_| unreachable!());

        // Same name returns same metatable.
        assert_eq!(mt1.gc_ref(), mt2.gc_ref());
    }

    #[test]
    fn lua_create_userdata_set_global() {
        let mut lua = Lua::new().ok().unwrap_or_else(Lua::new_empty);
        let ud = lua.create_userdata(99i64);

        // Set as global via IntoLua.
        let val: Val = ud.into_lua(&mut lua).unwrap_or(Val::Nil);
        lua.set_global_val("myud", val).ok();

        // Retrieve it.
        let got = lua.get_global_val("myud");
        assert!(matches!(got, Val::Userdata(_)));
    }

    #[test]
    fn backward_compat_exec() {
        let result = exec(b"local x = 1 + 2");
        assert!(result.is_ok());
    }

    #[test]
    fn backward_compat_exec_with_name() {
        let result = exec_with_name(b"local x = 1 + 2", "=test");
        assert!(result.is_ok());
    }

    // -- Interrupt flag --

    #[test]
    fn interrupt_flag_set_and_check() {
        // Ensure clean state.
        clear_interrupted();

        set_interrupted();
        assert!(check_interrupted(), "flag should be true after set");
        assert!(!check_interrupted(), "flag should auto-clear after check");
    }

    #[test]
    fn interrupt_flag_clear() {
        clear_interrupted();

        set_interrupted();
        clear_interrupted();
        assert!(!check_interrupted(), "flag should be false after clear");
    }

    /// Compile-time assertion that `Lua` is `Send` when the `send`
    /// feature is enabled.
    #[cfg(feature = "send")]
    #[test]
    fn lua_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<Lua>();
    }
}
