//! Closures and upvalues.
//!
//! A closure pairs a function prototype (or Rust function pointer)
//! with captured upvalues. Lua closures reference a `Proto` via `Rc`
//! and an array of upvalues. Upvalues start "open" (pointing into the
//! stack) and are "closed" (copied to the heap) when the creating
//! scope exits.
//!
//! ## Closure Types
//!
//! - `LuaClosure`: `ProtoRef` + shared `GcRef<Upvalue>` array + env table
//! - `RustClosure`: function pointer + inline `Vec<Val>` upvalues + name
//!
//! ## Upvalue States
//!
//! - `Open { stack_index }`: points to a live stack slot
//! - `Closed { value }`: owns the captured value
//!
//! Reference: `lfunc.h`, `lfunc.c`, `lobject.h` in PUC-Rio Lua 5.1.1.

use super::gc::arena::GcRef;
use super::gc::trace::Trace;
use super::proto::ProtoRef;
use super::table::Table;
use super::value::Val;

use crate::error::LuaResult;

// ---------------------------------------------------------------------------
// Forward declaration for LuaState (used in RustFn type)
// ---------------------------------------------------------------------------

use super::state::LuaState;

// ---------------------------------------------------------------------------
// Upvalue
// ---------------------------------------------------------------------------

/// Internal state of an upvalue: open (on stack) or closed (owned).
#[derive(Debug, Clone)]
pub enum UpvalueState {
    /// Points to a live stack slot. The value is read/written through
    /// the stack at the given index.
    Open { stack_index: usize },
    /// Owns the captured value. Transitioned from Open when the
    /// declaring function's scope exits.
    Closed { value: Val },
}

/// A captured variable shared between closures.
///
/// While the declaring function is active, the upvalue is "open" and
/// points to a stack slot. When the function returns, the upvalue is
/// "closed" and the value is copied into the upvalue's own storage.
///
/// Multiple closures can share the same `GcRef<Upvalue>`, so
/// mutations through one closure are visible to all.
#[derive(Debug, Clone)]
pub struct Upvalue {
    /// Whether this upvalue points to the stack or owns its value.
    pub state: UpvalueState,
}

impl Upvalue {
    /// Creates a new open upvalue pointing to the given stack index.
    #[must_use]
    pub fn new_open(stack_index: usize) -> Self {
        Self {
            state: UpvalueState::Open { stack_index },
        }
    }

    /// Creates a new closed upvalue with the given value.
    #[must_use]
    pub fn new_closed(value: Val) -> Self {
        Self {
            state: UpvalueState::Closed { value },
        }
    }

    /// Returns `true` if this upvalue is open (points to stack).
    #[must_use]
    pub fn is_open(&self) -> bool {
        matches!(self.state, UpvalueState::Open { .. })
    }

    /// Returns the stack index if this upvalue is open.
    #[must_use]
    pub fn stack_index(&self) -> Option<usize> {
        match self.state {
            UpvalueState::Open { stack_index } => Some(stack_index),
            UpvalueState::Closed { .. } => None,
        }
    }

    /// Reads the upvalue's current value.
    ///
    /// If open, reads from the stack. If closed, returns the owned value.
    #[must_use]
    pub fn get(&self, stack: &[Val]) -> Val {
        match self.state {
            UpvalueState::Open { stack_index } => {
                if stack_index < stack.len() {
                    stack[stack_index]
                } else {
                    Val::Nil
                }
            }
            UpvalueState::Closed { value } => value,
        }
    }

    /// Writes a value to the upvalue.
    ///
    /// If open, writes to the stack. If closed, overwrites the owned value.
    pub fn set(&mut self, stack: &mut [Val], val: Val) {
        match &mut self.state {
            UpvalueState::Open { stack_index } => {
                if (*stack_index) < stack.len() {
                    stack[*stack_index] = val;
                }
            }
            UpvalueState::Closed { value } => {
                *value = val;
            }
        }
    }

    /// Closes this upvalue: copies the current value from the stack
    /// and transitions to the Closed state.
    ///
    /// No-op if already closed.
    pub fn close(&mut self, stack: &[Val]) {
        if let UpvalueState::Open { stack_index } = self.state {
            let value = if stack_index < stack.len() {
                stack[stack_index]
            } else {
                Val::Nil
            };
            self.state = UpvalueState::Closed { value };
        }
    }
}

impl Trace for Upvalue {
    fn trace(&self) {
        // Phase 6: mark the closed value if it contains GC references.
    }
}

// ---------------------------------------------------------------------------
// RustFn type alias
// ---------------------------------------------------------------------------

/// Type alias for Rust functions callable from Lua.
///
/// The function receives a mutable reference to the `LuaState` and
/// returns the number of return values pushed onto the stack.
/// Matches PUC-Rio's `lua_CFunction`.
pub type RustFn = fn(&mut LuaState) -> LuaResult<u32>;

// ---------------------------------------------------------------------------
// LuaClosure
// ---------------------------------------------------------------------------

/// A Lua closure: compiled bytecode + captured upvalues.
///
/// The prototype is shared via `ProtoRef` (immutable, no cycles).
/// Upvalues are GC-managed and may be shared with other closures.
#[derive(Debug)]
pub struct LuaClosure {
    /// Compiled function prototype (shared, immutable).
    pub proto: ProtoRef,
    /// Captured upvalues (one per `proto.num_upvalues`).
    pub upvalues: Vec<GcRef<Upvalue>>,
    /// Environment table (used for global variable lookups).
    pub env: GcRef<Table>,
}

impl LuaClosure {
    /// Creates a new Lua closure from a prototype and environment.
    #[must_use]
    pub fn new(proto: ProtoRef, env: GcRef<Table>) -> Self {
        let num_upvalues = proto.num_upvalues as usize;
        Self {
            proto,
            upvalues: Vec::with_capacity(num_upvalues),
            env,
        }
    }
}

// ---------------------------------------------------------------------------
// RustClosure
// ---------------------------------------------------------------------------

/// A Rust closure: native function + inline upvalues.
///
/// Unlike Lua closures, Rust closures store upvalues as inline `Val`
/// values (not shared `GcRef<Upvalue>`). This matches PUC-Rio's
/// `CClosure` where upvalues are `TValue` arrays.
pub struct RustClosure {
    /// The native function pointer.
    pub func: RustFn,
    /// Inline upvalue storage (not shared between closures).
    pub upvalues: Vec<Val>,
    /// Function name for debug/error messages.
    pub name: String,
    /// Per-closure environment table (fenv). `None` means use global.
    /// Matches PUC-Rio's `CClosure.env`.
    pub env: Option<GcRef<Table>>,
    /// Opaque host pointer for foreign (C ABI) functions. `0` for native
    /// Rust functions. Used by the `lua51` C API layer to store the
    /// `lua_CFunction` that its trampoline `func` dispatches to.
    pub native: usize,
}

impl std::fmt::Debug for RustClosure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RustClosure")
            .field("name", &self.name)
            .field("upvalues", &self.upvalues.len())
            .finish_non_exhaustive()
    }
}

impl RustClosure {
    /// Creates a new Rust closure with no upvalues.
    #[must_use]
    pub fn new(func: RustFn, name: &str) -> Self {
        Self {
            func,
            upvalues: Vec::new(),
            name: name.to_string(),
            env: None,
            native: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// Closure enum
// ---------------------------------------------------------------------------

/// A Lua or Rust closure with captured upvalues.
///
/// The VM dispatch loop and call machinery use this enum to handle
/// both closure types uniformly.
#[derive(Debug)]
pub enum Closure {
    /// A Lua closure (compiled bytecode + shared upvalues).
    Lua(LuaClosure),
    /// A Rust closure (native function + inline upvalues).
    Rust(RustClosure),
}

impl Closure {
    /// Returns `true` if this is a Rust (C) closure.
    #[must_use]
    pub fn is_rust(&self) -> bool {
        matches!(self, Self::Rust(_))
    }

    /// Returns `true` if this is a Lua closure.
    #[must_use]
    pub fn is_lua(&self) -> bool {
        matches!(self, Self::Lua(_))
    }

    /// Returns a reference to the inner Lua closure, if applicable.
    #[must_use]
    pub fn as_lua(&self) -> Option<&LuaClosure> {
        match self {
            Self::Lua(cl) => Some(cl),
            Self::Rust(_) => None,
        }
    }

    /// Returns a mutable reference to the inner Lua closure, if applicable.
    pub fn as_lua_mut(&mut self) -> Option<&mut LuaClosure> {
        match self {
            Self::Lua(cl) => Some(cl),
            Self::Rust(_) => None,
        }
    }

    /// Returns a reference to the inner Rust closure, if applicable.
    #[must_use]
    pub fn as_rust(&self) -> Option<&RustClosure> {
        match self {
            Self::Rust(cl) => Some(cl),
            Self::Lua(_) => None,
        }
    }

    /// Returns a mutable reference to the inner Rust closure.
    pub fn as_rust_mut(&mut self) -> Option<&mut RustClosure> {
        match self {
            Self::Rust(cl) => Some(cl),
            Self::Lua(_) => None,
        }
    }
}

impl Trace for Closure {
    fn trace(&self) {
        // Phase 6: mark env, upvalues, and (for Rust closures) inline values.
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vm::gc::Color;
    use crate::vm::gc::arena::Arena;
    use crate::vm::proto::Proto;
    use crate::vm::table::Table;

    // Helper: create a dummy RustFn (must match RustFn signature).
    #[allow(clippy::unnecessary_wraps)]
    fn dummy_rust_fn(_state: &mut LuaState) -> LuaResult<u32> {
        Ok(0)
    }

    // ----- Upvalue tests -----

    #[test]
    fn upvalue_new_open() {
        let uv = Upvalue::new_open(5);
        assert!(uv.is_open());
        assert_eq!(uv.stack_index(), Some(5));
    }

    #[test]
    fn upvalue_new_closed() {
        let uv = Upvalue::new_closed(Val::Num(42.0));
        assert!(!uv.is_open());
        assert_eq!(uv.stack_index(), None);
    }

    #[test]
    fn upvalue_get_open() {
        let stack = vec![Val::Nil, Val::Num(10.0), Val::Num(20.0)];
        let uv = Upvalue::new_open(1);
        assert_eq!(uv.get(&stack), Val::Num(10.0));
    }

    #[test]
    fn upvalue_get_open_out_of_bounds() {
        let stack = vec![Val::Nil];
        let uv = Upvalue::new_open(100);
        assert!(uv.get(&stack).is_nil());
    }

    #[test]
    fn upvalue_get_closed() {
        let stack = vec![Val::Nil];
        let uv = Upvalue::new_closed(Val::Bool(true));
        assert_eq!(uv.get(&stack), Val::Bool(true));
    }

    #[test]
    fn upvalue_set_open() {
        let mut stack = vec![Val::Nil, Val::Nil, Val::Nil];
        let mut uv = Upvalue::new_open(1);
        uv.set(&mut stack, Val::Num(99.0));
        assert_eq!(stack[1], Val::Num(99.0));
    }

    #[test]
    fn upvalue_set_closed() {
        let mut stack = vec![Val::Nil];
        let mut uv = Upvalue::new_closed(Val::Num(1.0));
        uv.set(&mut stack, Val::Num(2.0));
        assert_eq!(uv.get(&stack), Val::Num(2.0));
    }

    #[test]
    fn upvalue_close() {
        let stack = vec![Val::Nil, Val::Num(42.0)];
        let mut uv = Upvalue::new_open(1);
        assert!(uv.is_open());

        uv.close(&stack);
        assert!(!uv.is_open());
        assert_eq!(uv.get(&[]), Val::Num(42.0));
    }

    #[test]
    fn upvalue_close_already_closed() {
        let stack = vec![Val::Nil];
        let mut uv = Upvalue::new_closed(Val::Bool(true));
        uv.close(&stack); // no-op
        assert_eq!(uv.get(&stack), Val::Bool(true));
    }

    #[test]
    fn upvalue_shared_mutation() {
        // Two upvalue refs pointing to the same stack slot.
        let mut stack = vec![Val::Nil, Val::Num(0.0)];

        let mut uv1 = Upvalue::new_open(1);
        let uv2 = Upvalue::new_open(1);

        uv1.set(&mut stack, Val::Num(5.0));
        // uv2 reads the same stack slot.
        assert_eq!(uv2.get(&stack), Val::Num(5.0));
    }

    // ----- LuaClosure tests -----

    #[test]
    fn lua_closure_new() {
        let mut tables: Arena<Table> = Arena::new();
        let env = tables.alloc(Table::new(), Color::White0);
        let mut proto = Proto::new("test");
        proto.num_upvalues = 2;
        let cl = LuaClosure::new(ProtoRef::new(proto), env);
        assert_eq!(cl.proto.source, "test");
        assert!(cl.upvalues.is_empty()); // capacity only, not filled yet
        assert_eq!(cl.upvalues.capacity(), 2);
        assert_eq!(cl.env, env);
    }

    // ----- RustClosure tests -----

    #[test]
    fn rust_closure_new() {
        let cl = RustClosure::new(dummy_rust_fn, "print");
        assert_eq!(cl.name, "print");
        assert!(cl.upvalues.is_empty());
    }

    #[test]
    fn rust_closure_with_upvalues() {
        let mut cl = RustClosure::new(dummy_rust_fn, "counter");
        cl.upvalues.push(Val::Num(0.0));
        assert_eq!(cl.upvalues.len(), 1);
        assert_eq!(cl.upvalues[0], Val::Num(0.0));
    }

    // ----- Closure enum tests -----

    #[test]
    fn closure_is_lua() {
        let mut tables: Arena<Table> = Arena::new();
        let env = tables.alloc(Table::new(), Color::White0);
        let cl = Closure::Lua(LuaClosure::new(ProtoRef::new(Proto::new("test")), env));
        assert!(cl.is_lua());
        assert!(!cl.is_rust());
        assert!(cl.as_lua().is_some());
        assert!(cl.as_rust().is_none());
    }

    #[test]
    fn closure_is_rust() {
        let cl = Closure::Rust(RustClosure::new(dummy_rust_fn, "f"));
        assert!(cl.is_rust());
        assert!(!cl.is_lua());
        assert!(cl.as_rust().is_some());
        assert!(cl.as_lua().is_none());
    }

    #[test]
    fn closure_debug_format() {
        let cl = Closure::Rust(RustClosure::new(dummy_rust_fn, "test_fn"));
        let debug = format!("{cl:?}");
        assert!(debug.contains("Rust"));
        assert!(debug.contains("test_fn"));
    }

    #[test]
    fn upvalue_trace_is_stub() {
        let uv = Upvalue::new_open(0);
        uv.trace(); // should not panic
    }

    #[test]
    fn closure_trace_is_stub() {
        let cl = Closure::Rust(RustClosure::new(dummy_rust_fn, "f"));
        cl.trace(); // should not panic
    }
}
