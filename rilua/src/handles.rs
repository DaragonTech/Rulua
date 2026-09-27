//! Handle types for Lua objects.
//!
//! Lightweight `Copy` newtypes over `GcRef<T>` that provide a typed,
//! public API for interacting with GC-managed Lua objects. All mutating
//! operations require `&mut Lua`, ensuring the borrow checker prevents
//! aliased mutation.

use std::any::Any;

use crate::error::{LuaError, LuaResult, RuntimeError};
use crate::vm::closure::Closure;
use crate::vm::gc::arena::GcRef;
use crate::vm::state::{LuaState, LuaThread, ThreadStatus};
use crate::vm::value::{Userdata, Val};

/// Handle to a Lua table in the GC arena.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Table(pub(crate) GcRef<crate::vm::table::Table>);

impl Table {
    /// Creates a `Table` handle from a raw `GcRef<Table>`.
    ///
    /// Used when converting `Val::Table` to a typed handle.
    pub fn from_gc_ref(r: GcRef<crate::vm::table::Table>) -> Self {
        Self(r)
    }
}

/// Handle to a Lua function (closure) in the GC arena.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Function(pub(crate) GcRef<Closure>);

/// Handle to a Lua coroutine thread in the GC arena.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Thread(pub(crate) GcRef<LuaThread>);

/// Handle to a Lua userdata in the GC arena.
///
/// Wraps a type-erased `Box<dyn Any>` value with an optional metatable.
/// Use [`borrow`](Self::borrow) / [`borrow_mut`](Self::borrow_mut) to
/// recover the concrete Rust type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnyUserData(pub(crate) GcRef<Userdata>);

impl AnyUserData {
    /// Creates an `AnyUserData` handle from a raw `GcRef<Userdata>`.
    ///
    /// Used when converting `Val::Userdata` to a typed handle.
    pub fn from_gc_ref(r: GcRef<Userdata>) -> Self {
        Self(r)
    }
}

impl Table {
    /// Gets a value by key without metamethod dispatch.
    pub fn raw_get(&self, state: &LuaState, key: Val) -> LuaResult<Val> {
        let table = state.gc.tables.get(self.0).ok_or_else(|| {
            LuaError::Runtime(RuntimeError {
                message: "table has been collected".into(),
                level: 0,
                traceback: vec![],
            })
        })?;
        Ok(table.get(key, &state.gc.string_arena))
    }

    /// Sets a value by key without metamethod dispatch.
    pub fn raw_set(&self, state: &mut LuaState, key: Val, value: Val) -> LuaResult<()> {
        let table = state.gc.tables.get_mut(self.0).ok_or_else(|| {
            LuaError::Runtime(RuntimeError {
                message: "table has been collected".into(),
                level: 0,
                traceback: vec![],
            })
        })?;
        table.raw_set(key, value, &state.gc.string_arena)
    }

    /// Returns the raw length of the table (no `__len` metamethod).
    pub fn raw_len(&self, state: &LuaState) -> i64 {
        state
            .gc
            .tables
            .get(self.0)
            .map_or(0, |t| t.len(&state.gc.string_arena) as i64)
    }

    /// Sets or clears the metatable for this table.
    pub fn set_metatable(&self, state: &mut LuaState, mt: Option<Self>) -> LuaResult<()> {
        let table = state.gc.tables.get_mut(self.0).ok_or_else(|| {
            LuaError::Runtime(RuntimeError {
                message: "table has been collected".into(),
                level: 0,
                traceback: vec![],
            })
        })?;
        table.set_metatable(mt.map(|t| t.0));
        Ok(())
    }

    /// Returns the underlying `GcRef` for internal use.
    pub fn gc_ref(self) -> GcRef<crate::vm::table::Table> {
        self.0
    }
}

impl Function {
    /// Creates a `Function` handle from a raw `GcRef<Closure>`.
    ///
    /// Used when converting `Val::Function` to a typed handle.
    pub fn from_gc_ref(r: GcRef<Closure>) -> Self {
        Self(r)
    }

    /// Returns the underlying `GcRef` for internal use.
    pub fn gc_ref(self) -> GcRef<Closure> {
        self.0
    }
}

impl Thread {
    /// Returns the status of this coroutine thread.
    pub fn status(&self, state: &LuaState) -> ThreadStatus {
        state
            .gc
            .threads
            .get(self.0)
            .map_or(ThreadStatus::Dead, |t| t.status)
    }

    /// Returns the underlying `GcRef` for internal use.
    pub fn gc_ref(self) -> GcRef<LuaThread> {
        self.0
    }
}

impl AnyUserData {
    /// Borrows the inner data as `&T`. Returns `None` if the stored
    /// type is not `T` or if the userdata has been collected.
    pub fn borrow<'a, T: Any>(&self, state: &'a LuaState) -> Option<&'a T> {
        state.gc.userdata.get(self.0)?.downcast_ref::<T>()
    }

    /// Borrows the inner data as `&mut T`. Returns `None` if the stored
    /// type is not `T` or if the userdata has been collected.
    pub fn borrow_mut<'a, T: Any>(&self, state: &'a mut LuaState) -> Option<&'a mut T> {
        state.gc.userdata.get_mut(self.0)?.downcast_mut::<T>()
    }

    /// Sets the metatable for this userdata.
    pub fn set_metatable(&self, state: &mut LuaState, mt: Option<Table>) -> LuaResult<()> {
        let ud = state.gc.userdata.get_mut(self.0).ok_or_else(|| {
            LuaError::Runtime(RuntimeError {
                message: "userdata has been collected".into(),
                level: 0,
                traceback: vec![],
            })
        })?;
        ud.set_metatable(mt.map(|t| t.0));
        Ok(())
    }

    /// Gets the metatable for this userdata, if set.
    pub fn metatable(&self, state: &LuaState) -> Option<Table> {
        let ud = state.gc.userdata.get(self.0)?;
        ud.metatable().map(Table)
    }

    /// Returns the underlying `GcRef` for internal use.
    pub fn gc_ref(self) -> GcRef<Userdata> {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vm::value::Userdata;

    #[test]
    fn table_raw_get_set() {
        let mut state = LuaState::new();
        let t = state.gc.alloc_table(crate::vm::table::Table::new());
        let handle = Table(t);

        let key = Val::Num(1.0);
        let value = Val::Num(42.0);
        handle.raw_set(&mut state, key, value).ok();
        let got = handle.raw_get(&state, key).ok();
        assert_eq!(got, Some(Val::Num(42.0)));
    }

    #[test]
    fn table_raw_len_empty() {
        let mut state = LuaState::new();
        let t = state.gc.alloc_table(crate::vm::table::Table::new());
        let handle = Table(t);
        assert_eq!(handle.raw_len(&state), 0);
    }

    #[test]
    fn function_gc_ref_round_trip() {
        let mut state = LuaState::new();
        let cl = Closure::Rust(crate::vm::closure::RustClosure::new(|_| Ok(0), "test"));
        let r = state.gc.alloc_closure(cl);
        let handle = Function(r);
        assert_eq!(handle.gc_ref(), r);
    }

    #[test]
    fn table_set_metatable() {
        let mut state = LuaState::new();
        let t = state.gc.alloc_table(crate::vm::table::Table::new());
        let mt = state.gc.alloc_table(crate::vm::table::Table::new());
        let handle = Table(t);
        let mt_handle = Table(mt);
        handle.set_metatable(&mut state, Some(mt_handle)).ok();
        // Verify the metatable was set by checking the table's metatable field.
        let table = state.gc.tables.get(t);
        assert!(table.is_some());
        assert_eq!(table.and_then(crate::vm::table::Table::metatable), Some(mt));
    }

    // -- AnyUserData tests --

    #[test]
    fn create_userdata_and_borrow() {
        let mut state = LuaState::new();
        let ud = Userdata::new(Box::new(42i64));
        let r = state.gc.alloc_userdata(ud);
        let handle = AnyUserData(r);

        let val = handle.borrow::<i64>(&state);
        assert_eq!(val, Some(&42i64));
    }

    #[test]
    fn create_userdata_type_mismatch() {
        let mut state = LuaState::new();
        let ud = Userdata::new(Box::new(42i64));
        let r = state.gc.alloc_userdata(ud);
        let handle = AnyUserData(r);

        // Try to borrow as wrong type.
        let val = handle.borrow::<String>(&state);
        assert!(val.is_none());
    }

    #[test]
    fn userdata_borrow_mut() {
        let mut state = LuaState::new();
        let ud = Userdata::new(Box::new(10i64));
        let r = state.gc.alloc_userdata(ud);
        let handle = AnyUserData(r);

        if let Some(val) = handle.borrow_mut::<i64>(&mut state) {
            *val = 99;
        }
        let val = handle.borrow::<i64>(&state);
        assert_eq!(val, Some(&99i64));
    }

    #[test]
    fn userdata_set_metatable() {
        let mut state = LuaState::new();
        let ud = Userdata::new(Box::new(()));
        let r = state.gc.alloc_userdata(ud);
        let handle = AnyUserData(r);

        // Initially no metatable.
        assert!(handle.metatable(&state).is_none());

        // Set a metatable.
        let mt = state.gc.alloc_table(crate::vm::table::Table::new());
        let mt_handle = Table(mt);
        handle.set_metatable(&mut state, Some(mt_handle)).ok();
        assert_eq!(handle.metatable(&state), Some(mt_handle));

        // Clear metatable.
        handle.set_metatable(&mut state, None).ok();
        assert!(handle.metatable(&state).is_none());
    }

    #[test]
    fn userdata_gc_ref_round_trip() {
        let mut state = LuaState::new();
        let ud = Userdata::new(Box::new("test".to_string()));
        let r = state.gc.alloc_userdata(ud);
        let handle = AnyUserData(r);
        assert_eq!(handle.gc_ref(), r);
    }
}
