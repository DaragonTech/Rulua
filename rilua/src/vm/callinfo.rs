//! Call stack: CallInfo chain for tracking active function calls.
//!
//! Each active function call (Lua or Rust) occupies one `CallInfo` entry.
//! The call stack is a dynamic `Vec<CallInfo>` array, separate from the
//! value stack. Using `usize` indices (not pointers) means reallocation
//! never invalidates saved state.
//!
//! Reference: `lstate.h` CallInfo struct in PUC-Rio Lua 5.1.1.

/// Sentinel value for `num_results`: return all results.
///
/// Matches PUC-Rio's `LUA_MULTRET`. When a function is called with
/// `num_results == LUA_MULTRET`, all return values are kept and the
/// caller reads `top` to determine how many were returned.
pub const LUA_MULTRET: i32 = -1;

/// Per-call metadata for one active function invocation.
///
/// Tracks the stack frame boundaries, saved program counter, and
/// expected return count. The struct is identical for Lua and Rust
/// functions -- the distinction is made by examining the function
/// value at `stack[func]`.
#[derive(Debug, Clone)]
pub struct CallInfo {
    /// Stack index of the function value itself.
    pub func: usize,

    /// Base of this function's stack frame. Points to the first
    /// local variable slot (register 0). For vararg functions,
    /// base is after the fixed-to-vararg adjustment.
    pub base: usize,

    /// Stack top limit for this function. Set to
    /// `base + proto.max_stack_size` for Lua functions, or
    /// `base + LUA_MINSTACK` for Rust functions.
    pub top: usize,

    /// Saved program counter (index into Proto's code array).
    /// Only meaningful for Lua functions. Saved before calling a
    /// child function, restored on return.
    pub saved_pc: usize,

    /// Number of results expected by the caller. `LUA_MULTRET` (-1)
    /// means "all results".
    pub num_results: i32,

    /// Count of tail calls optimized under this frame. Used by
    /// debug hooks to report elided frames.
    pub tail_calls: i32,

    /// Whether this frame holds a Lua closure (vs Rust or sentinel).
    /// Cached at frame creation to avoid arena lookups in hot paths
    /// like `resolve_stack_level_raw`.
    pub is_lua: bool,
}

impl CallInfo {
    /// Creates a new CallInfo for the initial (bottom) frame.
    ///
    /// The initial frame has func=0, base=1, and no saved PC.
    /// It represents the C-level entry point before any Lua code runs.
    #[must_use]
    pub fn new(func: usize, base: usize, top: usize, num_results: i32) -> Self {
        Self {
            func,
            base,
            top,
            saved_pc: 0,
            num_results,
            tail_calls: 0,
            is_lua: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lua_multret_value() {
        assert_eq!(LUA_MULTRET, -1);
    }

    #[test]
    fn callinfo_new() {
        let ci = CallInfo::new(0, 1, 21, LUA_MULTRET);
        assert_eq!(ci.func, 0);
        assert_eq!(ci.base, 1);
        assert_eq!(ci.top, 21);
        assert_eq!(ci.saved_pc, 0);
        assert_eq!(ci.num_results, LUA_MULTRET);
        assert_eq!(ci.tail_calls, 0);
    }

    #[test]
    fn callinfo_with_fixed_results() {
        let ci = CallInfo::new(5, 6, 26, 3);
        assert_eq!(ci.func, 5);
        assert_eq!(ci.base, 6);
        assert_eq!(ci.top, 26);
        assert_eq!(ci.num_results, 3);
    }

    #[test]
    fn callinfo_clone() {
        let ci = CallInfo::new(0, 1, 21, 1);
        let ci2 = ci.clone();
        assert_eq!(ci.func, ci2.func);
        assert_eq!(ci.base, ci2.base);
        assert_eq!(ci.top, ci2.top);
        assert_eq!(ci.num_results, ci2.num_results);
    }

    #[test]
    fn callinfo_saved_pc_mutation() {
        let mut ci = CallInfo::new(0, 1, 21, 1);
        assert_eq!(ci.saved_pc, 0);
        ci.saved_pc = 42;
        assert_eq!(ci.saved_pc, 42);
    }

    #[test]
    fn callinfo_tail_calls_mutation() {
        let mut ci = CallInfo::new(0, 1, 21, 1);
        assert_eq!(ci.tail_calls, 0);
        ci.tail_calls = 5;
        assert_eq!(ci.tail_calls, 5);
    }

    #[test]
    fn callinfo_debug_format() {
        let ci = CallInfo::new(0, 1, 21, LUA_MULTRET);
        let debug = format!("{ci:?}");
        assert!(debug.contains("CallInfo"));
        assert!(debug.contains("func: 0"));
        assert!(debug.contains("base: 1"));
    }
}
