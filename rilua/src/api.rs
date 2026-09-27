//! Public API traits for Lua operations.
//!
//! Provides `LuaApi` for immutable operations and `LuaApiMut` for mutable operations.

use std::any::Any;

use crate::conversion::{FromLua, IntoLua};
use crate::error::{LuaError, LuaResult, RuntimeError};
use crate::handles::{AnyUserData, Function, Table};
use crate::vm::closure::{Closure, LuaClosure, RustClosure, RustFn};
use crate::vm::proto::ProtoRef;
use crate::vm::state::LuaState;
use crate::vm::value::Val;

/// Read-only API trait for Lua operations.
///
/// Provides immutable access to Lua state for operations that don't modify state.
pub trait LuaApi {
    /// Returns an immutable reference to the underlying `LuaState`.
    fn state(&self) -> &LuaState;

    /// Returns the total memory in use by Lua (in bytes).
    fn gc_count(&self) -> usize {
        self.state().gc.gc_state.total_bytes
    }

    /// Raw get on a table handle via the public API.
    fn table_raw_get(&self, table: &Table, key: Val) -> LuaResult<Val> {
        table.raw_get(self.state(), key)
    }

    /// Returns the raw length of a table (no `__len` metamethod).
    fn table_raw_len(&self, table: &Table) -> i64 {
        table.raw_len(self.state())
    }
}

/// Mutable API trait for Lua operations.
///
/// Extends `LuaApi` with operations that require mutable access to state.
pub trait LuaApiMut: LuaApi {
    /// Returns a mutable reference to the underlying `LuaState`.
    fn state_mut(&mut self) -> &mut LuaState;

    // -----------------------------------------------------------------------
    // Globals
    // -----------------------------------------------------------------------

    /// Gets a global variable, converting it to the requested Rust type.
    fn global<V: FromLua>(&mut self, name: &str) -> LuaResult<V>
    where
        Self: Sized,
    {
        let val = self.get_global_val(name);
        V::from_lua(val, self)
    }

    /// Sets a global variable from a Rust value.
    fn set_global<V: IntoLua>(&mut self, name: &str, value: V) -> LuaResult<()>
    where
        Self: Sized,
    {
        let val = value.into_lua(self)?;
        self.set_global_val(name, val)
    }

    /// Reads a value from the global table by name.
    fn get_global_val(&mut self, name: &str) -> Val {
        let state = self.state_mut();
        let key_ref = state.gc.intern_string(name.as_bytes());
        let Some(global_table) = state.gc.tables.get(state.global) else {
            return Val::Nil;
        };
        global_table.get_str(key_ref, &state.gc.string_arena)
    }

    /// Sets a value in the global table by name.
    fn set_global_val(&mut self, name: &str, val: Val) -> LuaResult<()> {
        let state = self.state_mut();
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
        table.raw_set(key, val, &state.gc.string_arena)
    }

    // -----------------------------------------------------------------------
    // Table creation
    // -----------------------------------------------------------------------

    /// Allocates a new empty table and returns a handle.
    fn create_table(&mut self) -> Table {
        let state = self.state_mut();
        let r = state.gc.alloc_table(crate::vm::table::Table::new());
        Table(r)
    }

    // -----------------------------------------------------------------------
    // Userdata creation
    // -----------------------------------------------------------------------

    /// Creates a new userdata containing `data` with no metatable.
    ///
    /// With the `send` feature enabled, `T` must also implement `Send`.
    #[cfg(not(feature = "send"))]
    fn create_userdata<T: Any>(&mut self, data: T) -> AnyUserData {
        let state = self.state_mut();
        let ud = crate::vm::value::Userdata::new(Box::new(data));
        let r = state.gc.alloc_userdata(ud);
        AnyUserData(r)
    }

    /// Creates a new userdata containing `data` with no metatable.
    ///
    /// With the `send` feature, `T` must implement `Send` so the `Lua`
    /// instance remains thread-safe.
    #[cfg(feature = "send")]
    fn create_userdata<T: Any + Send>(&mut self, data: T) -> AnyUserData {
        let state = self.state_mut();
        let ud = crate::vm::value::Userdata::new(Box::new(data));
        let r = state.gc.alloc_userdata(ud);
        AnyUserData(r)
    }

    /// Creates a new userdata with a named, registry-cached metatable.
    #[cfg(not(feature = "send"))]
    fn create_typed_userdata<T: Any>(
        &mut self,
        data: T,
        type_name: &str,
    ) -> LuaResult<AnyUserData> {
        let state = self.state_mut();
        let mt = crate::stdlib::new_metatable(state, type_name)?;
        let ud = crate::vm::value::Userdata::with_metatable(Box::new(data), mt);
        let r = state.gc.alloc_userdata(ud);
        Ok(AnyUserData(r))
    }

    /// Creates a new userdata with a named, registry-cached metatable
    /// (thread-safe variant).
    #[cfg(feature = "send")]
    fn create_typed_userdata<T: Any + Send>(
        &mut self,
        data: T,
        type_name: &str,
    ) -> LuaResult<AnyUserData> {
        let state = self.state_mut();
        let mt = crate::stdlib::new_metatable(state, type_name)?;
        let ud = crate::vm::value::Userdata::with_metatable(Box::new(data), mt);
        let r = state.gc.alloc_userdata(ud);
        Ok(AnyUserData(r))
    }

    /// Creates or retrieves a named metatable for a userdata type.
    fn create_userdata_metatable(&mut self, type_name: &str) -> LuaResult<Table> {
        let state = self.state_mut();
        let mt = crate::stdlib::new_metatable(state, type_name)?;
        Ok(Table(mt))
    }

    // -----------------------------------------------------------------------
    // Function registration
    // -----------------------------------------------------------------------

    /// Registers a Rust function as a global Lua function.
    fn register_function(&mut self, name: &str, func: RustFn) -> LuaResult<()> {
        let state = self.state_mut();
        let closure = Closure::Rust(RustClosure::new(func, name));
        let closure_ref = state.gc.alloc_closure(closure);
        self.set_global_val(name, Val::Function(closure_ref))
    }

    // -----------------------------------------------------------------------
    // GC control
    // -----------------------------------------------------------------------

    /// Runs a full garbage collection cycle.
    fn gc_collect(&mut self) -> LuaResult<()> {
        self.state_mut().full_gc()
    }

    /// Stops the garbage collector.
    fn gc_stop(&mut self) {
        self.state_mut().gc.gc_state.gc_threshold = usize::MAX;
    }

    /// Restarts the garbage collector.
    fn gc_restart(&mut self) {
        let state = self.state_mut();
        state.gc.gc_state.gc_threshold = state.gc.gc_state.total_bytes;
    }

    /// Performs an incremental GC step.
    fn gc_step(&mut self, step_size: i64) -> LuaResult<bool> {
        self.state_mut().gc_step(step_size)
    }

    /// Sets the GC pause parameter (percentage). Returns the previous value.
    fn gc_set_pause(&mut self, pause: u32) -> u32 {
        let state = self.state_mut();
        let old = state.gc.gc_state.gc_pause;
        state.gc.gc_state.gc_pause = pause;
        old
    }

    /// Sets the GC step multiplier. Returns the previous value.
    fn gc_set_step_multiplier(&mut self, stepmul: u32) -> u32 {
        let state = self.state_mut();
        let old = state.gc.gc_state.gc_stepmul;
        state.gc.gc_state.gc_stepmul = stepmul;
        old
    }

    // -----------------------------------------------------------------------
    // String creation
    // -----------------------------------------------------------------------

    /// Interns a byte string via the GC string table, returning `Val::Str`.
    fn create_string(&mut self, s: &[u8]) -> Val {
        let state = self.state_mut();
        let str_ref = state.gc.intern_string(s);
        Val::Str(str_ref)
    }

    // -----------------------------------------------------------------------
    // Table operations
    // -----------------------------------------------------------------------

    /// Raw set on a table handle via the public API.
    fn table_raw_set(&mut self, table: &Table, key: Val, value: Val) -> LuaResult<()> {
        table.raw_set(self.state_mut(), key, value)
    }

    /// Sets a named Rust function on a table.
    fn table_set_function(&mut self, table: &Table, name: &str, func: RustFn) -> LuaResult<()> {
        let state = self.state_mut();
        let key = Val::Str(state.gc.intern_string(name.as_bytes()));
        let closure = Closure::Rust(RustClosure::new(func, name));
        let closure_ref = state.gc.alloc_closure(closure);
        table.raw_set(state, key, Val::Function(closure_ref))
    }

    // -----------------------------------------------------------------------
    // Compilation and loading
    // -----------------------------------------------------------------------

    /// Compiles Lua source bytes (or loads a binary chunk) and returns a function handle.
    fn load_bytes(&mut self, source: &[u8], name: &str) -> LuaResult<Function> {
        let proto = crate::compile_or_undump(source, name)?;
        let mut proto = ProtoRef::try_unwrap(proto).unwrap_or_else(|rc| (*rc).clone());
        let state = self.state_mut();
        crate::patch_string_constants(&mut proto, &mut state.gc);
        let proto = ProtoRef::new(proto);

        let num_upvalues = proto.num_upvalues as usize;
        let mut lua_cl = LuaClosure::new(proto, state.global);
        for _ in 0..num_upvalues {
            let uv = crate::vm::closure::Upvalue::new_closed(Val::Nil);
            let uv_ref = state.gc.alloc_upvalue(uv);
            lua_cl.upvalues.push(uv_ref);
        }
        let closure_ref = state.gc.alloc_closure(Closure::Lua(lua_cl));
        Ok(Function(closure_ref))
    }

    /// Compiles a Lua source string and returns a function handle.
    fn load(&mut self, source: &str) -> LuaResult<Function> {
        self.load_bytes(source.as_bytes(), "=(string)")
    }
}
