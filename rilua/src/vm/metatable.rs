//! Metatable and metamethod support.
//!
//! Lua 5.1.1's extensibility mechanism: metatables control operator
//! overloading, table access delegation, function call interception,
//! and garbage collection behavior.
//!
//! ## Fast Negative Cache
//!
//! Tables store a `flags` byte where each bit caches the absence of
//! a specific metamethod. The `gettm` function sets a bit when a
//! metamethod is NOT found. Any `rawset` with a key starting with
//! `__` resets all flags to 0 (invalidating the cache).
//!
//! Reference: `ltm.h`, `ltm.c` in PUC-Rio Lua 5.1.1.

use super::gc::arena::{Arena, GcRef};
use super::string::LuaString;
use super::table::Table;
use super::value::{Userdata, Val};

// ---------------------------------------------------------------------------
// TMS enum (tag method selector)
// ---------------------------------------------------------------------------

/// Tag method selector. Enumerates all 17 metamethod events.
///
/// Order matches PUC-Rio's `ltm.h` exactly -- do not reorder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum TMS {
    /// `__index` -- table read delegation
    Index = 0,
    /// `__newindex` -- table write delegation
    NewIndex = 1,
    /// `__gc` -- finalizer
    Gc = 2,
    /// `__mode` -- weak table mode
    Mode = 3,
    /// `__eq` -- equality comparison (last event with fast-access cache)
    Eq = 4,
    /// `__add` -- addition
    Add = 5,
    /// `__sub` -- subtraction
    Sub = 6,
    /// `__mul` -- multiplication
    Mul = 7,
    /// `__div` -- division
    Div = 8,
    /// `__mod` -- modulo
    Mod = 9,
    /// `__pow` -- exponentiation
    Pow = 10,
    /// `__unm` -- unary minus
    Unm = 11,
    /// `__len` -- length operator
    Len = 12,
    /// `__lt` -- less-than comparison
    Lt = 13,
    /// `__le` -- less-or-equal comparison
    Le = 14,
    /// `__concat` -- concatenation
    Concat = 15,
    /// `__call` -- function call
    Call = 16,
}

/// Total number of metamethod events.
pub const TM_N: usize = 17;

/// Maximum depth for chained metamethod lookups (__index chains, etc.).
///
/// Prevents infinite loops when metatables form cycles.
pub const MAXTAGLOOP: usize = 100;

/// Metamethod names in TMS order.
pub const TM_NAMES: [&str; TM_N] = [
    "__index",
    "__newindex",
    "__gc",
    "__mode",
    "__eq",
    "__add",
    "__sub",
    "__mul",
    "__div",
    "__mod",
    "__pow",
    "__unm",
    "__len",
    "__lt",
    "__le",
    "__concat",
    "__call",
];

// ---------------------------------------------------------------------------
// Fast negative cache
// ---------------------------------------------------------------------------

/// Check whether the flags byte indicates a metamethod is absent.
///
/// Only events 0..=4 (Index through Eq) are cached in the flags byte.
/// Returns `true` if the event is cached as absent (bit is set).
#[inline]
pub fn fast_tm_absent(flags: u8, event: TMS) -> bool {
    let bit = event as u8;
    if bit > TMS::Eq as u8 {
        return false; // not cached
    }
    flags & (1u8 << bit) != 0
}

/// Invalidate all cached flags (reset to 0).
///
/// Called when a key starting with `__` is rawset into a table.
#[inline]
pub fn invalidate_flags(flags: &mut u8) {
    *flags = 0;
}

// ---------------------------------------------------------------------------
// Metamethod lookup
// ---------------------------------------------------------------------------

/// Look up a metamethod in a metatable, updating the flags cache.
///
/// If the metamethod is not found and the event is cacheable (Index..Eq),
/// sets the corresponding bit in `flags`. Returns the metamethod value
/// or `None` if not present.
///
/// Matches PUC-Rio's `luaT_gettm`.
pub fn gettm(
    tables: &Arena<Table>,
    string_arena: &Arena<LuaString>,
    mt: GcRef<Table>,
    _event: TMS,
    tm_name: GcRef<LuaString>,
) -> Option<Val> {
    let table = tables.get(mt)?;
    let val = table.get_str(tm_name, string_arena);

    if val.is_nil() {
        // Cache absence for fast events (Index through Eq).
        // Flag updates must be done by the caller via Table::set_tm_flag()
        // since we only have an immutable reference here.
        None
    } else {
        Some(val)
    }
}

/// Fast metamethod lookup using the flags cache.
///
/// Checks the table's flags byte first. If the flag bit is set for this
/// event, the metamethod is known absent and we skip the hash lookup.
/// Only effective for events Index..Eq (bits 0-4).
///
/// Matches PUC-Rio's `fasttm` macro.
pub fn fasttm(
    tables: &Arena<Table>,
    string_arena: &Arena<LuaString>,
    mt: GcRef<Table>,
    event: TMS,
    tm_names: &[Option<GcRef<LuaString>>; TM_N],
) -> Option<Val> {
    let table = tables.get(mt)?;
    if fast_tm_absent(table.flags(), event) {
        return None;
    }
    let tm_name = tm_names[event as usize]?;
    let val = table.get_str(tm_name, string_arena);
    if val.is_nil() { None } else { Some(val) }
}

/// Look up `__eq` metamethod shared by two metatables.
///
/// For equality comparison in Lua 5.1.1, both operands must share the
/// same `__eq` metamethod (either from the same metatable, or from
/// different metatables with raw-equal TM values).
///
/// Returns `None` if either operand lacks `__eq` or the TM values differ.
///
/// Matches PUC-Rio's `get_compTM`.
pub fn get_comp_tm(
    tables: &Arena<Table>,
    string_arena: &Arena<LuaString>,
    mt1: Option<GcRef<Table>>,
    mt2: Option<GcRef<Table>>,
    event: TMS,
    tm_names: &[Option<GcRef<LuaString>>; TM_N],
) -> Option<Val> {
    let mt1 = mt1?;
    let tm1 = fasttm(tables, string_arena, mt1, event, tm_names)?;

    // Same metatable => same metamethods (fast path).
    if let Some(mt2) = mt2
        && mt1 == mt2
    {
        return Some(tm1);
    }

    let mt2 = mt2?;
    let tm2 = fasttm(tables, string_arena, mt2, event, tm_names)?;

    // Both must be the same value (raw equality).
    if val_raw_equal(tm1, tm2, tables, string_arena) {
        Some(tm1)
    } else {
        None
    }
}

/// Raw equality of two values (no metamethods).
///
/// Used by `get_comp_tm` and `rawequal`. Matches PUC-Rio's `luaO_rawequalObj`.
pub fn val_raw_equal(
    a: Val,
    b: Val,
    _tables: &Arena<Table>,
    string_arena: &Arena<LuaString>,
) -> bool {
    #![allow(clippy::match_same_arms)]
    match (&a, &b) {
        (Val::Nil, Val::Nil) => true,
        (Val::Bool(x), Val::Bool(y)) => x == y,
        // Lua raw equality: exact f64 comparison (NaN != NaN, +0 == -0).
        #[allow(clippy::float_cmp)]
        (Val::Num(x), Val::Num(y)) => x == y,
        (Val::Str(x), Val::Str(y)) => {
            if x == y {
                return true; // Same GcRef (identity)
            }
            // Interned strings: same content means same ref.
            // But compare content as fallback.
            let sx = string_arena.get(*x);
            let sy = string_arena.get(*y);
            match (sx, sy) {
                (Some(a), Some(b)) => a.data() == b.data(),
                _ => false,
            }
        }
        // Reference types: identity comparison.
        (Val::Table(x), Val::Table(y)) => x == y,
        (Val::Function(x), Val::Function(y)) => x == y,
        (Val::Userdata(x), Val::Userdata(y)) => x == y,
        (Val::Thread(x), Val::Thread(y)) => x == y,
        (Val::LightUserdata(x), Val::LightUserdata(y)) => x == y,
        // Different types are never equal.
        _ => false,
    }
}

/// Look up a metamethod in the metatable for a given value.
///
/// - Tables and userdata have per-instance metatables.
/// - Other types use type metatables from the `type_metatables` array.
///
/// Matches PUC-Rio's `luaT_gettmbyobj`.
pub fn gettmbyobj(
    val: Val,
    event: TMS,
    tables: &Arena<Table>,
    string_arena: &Arena<LuaString>,
    type_metatables: &[Option<GcRef<Table>>; NUM_TYPE_TAGS],
    tm_names: &[Option<GcRef<LuaString>>; TM_N],
    userdata: &Arena<Userdata>,
) -> Option<Val> {
    let tm_name = tm_names[event as usize]?;

    let mt = match val {
        Val::Table(r) => tables.get(r).and_then(Table::metatable)?,
        Val::Userdata(r) => userdata.get(r).and_then(Userdata::metatable)?,
        _ => type_metatables[type_tag(val)]?,
    };

    let table = tables.get(mt)?;
    let result = table.get_str(tm_name, string_arena);
    if result.is_nil() { None } else { Some(result) }
}

// ---------------------------------------------------------------------------
// Type tag for type_metatables indexing
// ---------------------------------------------------------------------------

/// Returns a type index for indexing into `type_metatables`.
///
/// Maps each Val variant to a stable index:
/// Nil=0, Bool=1, Number=2, String=3, Function=4,
/// Userdata=5, Thread=6, Table=7, LightUserdata=8
#[inline]
pub fn type_tag(val: Val) -> usize {
    match val {
        Val::Nil => 0,
        Val::Bool(_) => 1,
        Val::Num(_) => 2,
        Val::Str(_) => 3,
        Val::Function(_) => 4,
        Val::Userdata(_) => 5,
        Val::Thread(_) => 6,
        Val::Table(_) => 7,
        Val::LightUserdata(_) => 8,
    }
}

/// Number of distinct type tags.
pub const NUM_TYPE_TAGS: usize = 9;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tms_values_match_puc_rio() {
        assert_eq!(TMS::Index as u8, 0);
        assert_eq!(TMS::NewIndex as u8, 1);
        assert_eq!(TMS::Gc as u8, 2);
        assert_eq!(TMS::Mode as u8, 3);
        assert_eq!(TMS::Eq as u8, 4);
        assert_eq!(TMS::Add as u8, 5);
        assert_eq!(TMS::Sub as u8, 6);
        assert_eq!(TMS::Mul as u8, 7);
        assert_eq!(TMS::Div as u8, 8);
        assert_eq!(TMS::Mod as u8, 9);
        assert_eq!(TMS::Pow as u8, 10);
        assert_eq!(TMS::Unm as u8, 11);
        assert_eq!(TMS::Len as u8, 12);
        assert_eq!(TMS::Lt as u8, 13);
        assert_eq!(TMS::Le as u8, 14);
        assert_eq!(TMS::Concat as u8, 15);
        assert_eq!(TMS::Call as u8, 16);
    }

    #[test]
    fn tm_n_count() {
        assert_eq!(TM_N, 17);
    }

    #[test]
    fn fast_tm_absent_cached() {
        let mut flags: u8 = 0;
        // Not cached yet.
        assert!(!fast_tm_absent(flags, TMS::Index));

        // Set bit for Index.
        flags |= 1 << (TMS::Index as u8);
        assert!(fast_tm_absent(flags, TMS::Index));
        assert!(!fast_tm_absent(flags, TMS::NewIndex));
    }

    #[test]
    fn fast_tm_absent_uncacheable() {
        let flags: u8 = 0xFF;
        // Events beyond Eq are not cached, so always return false.
        assert!(!fast_tm_absent(flags, TMS::Add));
        assert!(!fast_tm_absent(flags, TMS::Call));
    }

    #[test]
    fn invalidate_clears_flags() {
        let mut flags: u8 = 0xFF;
        invalidate_flags(&mut flags);
        assert_eq!(flags, 0);
    }

    #[test]
    fn type_tag_values() {
        assert_eq!(type_tag(Val::Nil), 0);
        assert_eq!(type_tag(Val::Bool(true)), 1);
        assert_eq!(type_tag(Val::Num(1.0)), 2);
        assert_eq!(type_tag(Val::LightUserdata(0)), 8);
    }

    #[test]
    fn tm_names_correct() {
        assert_eq!(TM_NAMES[TMS::Index as usize], "__index");
        assert_eq!(TM_NAMES[TMS::Add as usize], "__add");
        assert_eq!(TM_NAMES[TMS::Call as usize], "__call");
        assert_eq!(TM_NAMES.len(), TM_N);
    }

    #[test]
    fn val_raw_equal_basics() {
        let tables: Arena<Table> = Arena::new();
        let strings: Arena<LuaString> = Arena::new();

        assert!(val_raw_equal(Val::Nil, Val::Nil, &tables, &strings));
        assert!(!val_raw_equal(
            Val::Nil,
            Val::Bool(false),
            &tables,
            &strings
        ));
        assert!(val_raw_equal(
            Val::Bool(true),
            Val::Bool(true),
            &tables,
            &strings
        ));
        assert!(!val_raw_equal(
            Val::Bool(true),
            Val::Bool(false),
            &tables,
            &strings
        ));
        assert!(val_raw_equal(
            Val::Num(1.0),
            Val::Num(1.0),
            &tables,
            &strings
        ));
        assert!(!val_raw_equal(
            Val::Num(1.0),
            Val::Num(2.0),
            &tables,
            &strings
        ));
        assert!(val_raw_equal(
            Val::LightUserdata(42),
            Val::LightUserdata(42),
            &tables,
            &strings,
        ));
        assert!(!val_raw_equal(
            Val::LightUserdata(1),
            Val::LightUserdata(2),
            &tables,
            &strings,
        ));
    }
}
