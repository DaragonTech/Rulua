//! Value representation: the `Val` enum for all Lua values.
//!
//! All Lua values are represented by the [`Val`] enum. Value types
//! (nil, boolean, number) are stored inline. Reference types (string,
//! table, function, userdata, thread) store an index into the GC arena.
//!
//! ## Equality
//!
//! Lua equality follows IEEE 754 for numbers (NaN != NaN, -0.0 == +0.0)
//! and identity comparison for reference types. `Val` implements
//! `PartialEq` but not `Eq` because NaN breaks reflexivity.
//!
//! ## Hashing
//!
//! Hashing is consistent with equality: -0.0 and +0.0 produce the same
//! hash. NaN values hash consistently but are rejected as table keys by
//! the table implementation.
//!
//! ## Number formatting
//!
//! Numbers format using `"%.14g"` rules (14 significant digits, trailing
//! zeros stripped) to match PUC-Rio's `luaO_str2d` / `lua_number2str`.

use std::any::Any;
use std::fmt;
use std::hash::{Hash, Hasher};

use super::closure::Closure;
use super::gc::arena::GcRef;
use super::gc::trace::Trace;
use super::state::LuaThread;
use super::string::LuaString;
use super::table::Table;

// ---------------------------------------------------------------------------
// Userdata data box type alias
// ---------------------------------------------------------------------------

/// Type-erased storage for userdata values.
///
/// Without the `send` feature, any `'static` type can be stored.
/// With the `send` feature, only `Send` types can be stored, enabling
/// the `Lua` struct to implement `Send`.
#[cfg(not(feature = "send"))]
pub type UserDataBox = Box<dyn Any>;

/// Type-erased storage for userdata values (thread-safe variant).
#[cfg(feature = "send")]
pub type UserDataBox = Box<dyn Any + Send>;

// ---------------------------------------------------------------------------
// Userdata (defined here -- no separate module)
// ---------------------------------------------------------------------------

/// Full userdata: a GC-managed block of user data with an optional
/// metatable and environment table.
///
/// Stores an arbitrary Rust value via [`UserDataBox`] with optional
/// metatable and environment. The I/O library stores file handles here;
/// `newproxy()` stores `()`.
///
/// Reference: `Udata` in `lobject.h`, PUC-Rio Lua 5.1.1.
pub struct Userdata {
    /// The user-owned data. Type-erased; use `downcast_ref`/`downcast_mut`
    /// to recover the concrete type.
    data: UserDataBox,
    /// Per-instance metatable (same model as Table).
    metatable: Option<GcRef<Table>>,
    /// Per-instance environment table (fenv).
    env: Option<GcRef<Table>>,
    /// Whether `__gc` has already been called on this object.
    /// Prevents double-finalization across GC cycles.
    finalized: bool,
    /// Monotonic allocation sequence number for finalization ordering.
    /// PUC-Rio finalizes userdata newest-first (LIFO). Since our arena
    /// iteration order doesn't match allocation order after slot reuse,
    /// we track the allocation sequence explicitly.
    alloc_seq: u64,
}

impl Userdata {
    /// Creates a new userdata with the given data and no metatable.
    pub fn new(data: UserDataBox) -> Self {
        Self {
            data,
            metatable: None,
            env: None,
            finalized: false,
            alloc_seq: 0,
        }
    }

    /// Creates a new userdata with data and a metatable.
    pub fn with_metatable(data: UserDataBox, mt: GcRef<Table>) -> Self {
        Self {
            data,
            metatable: Some(mt),
            env: None,
            finalized: false,
            alloc_seq: 0,
        }
    }

    /// Returns a stable pointer to the boxed user data.
    ///
    /// The boxed value never moves while the userdata is alive, so this
    /// pointer is suitable for handing out through the C API
    /// (`lua_touserdata`).
    pub fn data_ptr(&self) -> *const u8 {
        std::ptr::from_ref::<dyn Any>(&*self.data).cast::<u8>()
    }

    /// Returns the allocation sequence number.
    pub fn alloc_seq(&self) -> u64 {
        self.alloc_seq
    }

    /// Sets the allocation sequence number.
    pub fn set_alloc_seq(&mut self, seq: u64) {
        self.alloc_seq = seq;
    }

    /// Returns a reference to the inner data, downcasting to `T`.
    ///
    /// Returns `None` if the stored type is not `T`.
    pub fn downcast_ref<T: Any>(&self) -> Option<&T> {
        self.data.downcast_ref::<T>()
    }

    /// Returns a mutable reference to the inner data, downcasting to `T`.
    ///
    /// Returns `None` if the stored type is not `T`.
    pub fn downcast_mut<T: Any>(&mut self) -> Option<&mut T> {
        self.data.downcast_mut::<T>()
    }

    /// Returns the per-instance metatable, if set.
    pub fn metatable(&self) -> Option<GcRef<Table>> {
        self.metatable
    }

    /// Sets the per-instance metatable.
    pub fn set_metatable(&mut self, mt: Option<GcRef<Table>>) {
        self.metatable = mt;
    }

    /// Returns the per-instance environment table, if set.
    pub fn env(&self) -> Option<GcRef<Table>> {
        self.env
    }

    /// Sets the per-instance environment table.
    pub fn set_env(&mut self, env: Option<GcRef<Table>>) {
        self.env = env;
    }

    /// Returns whether `__gc` has already been called on this object.
    pub fn finalized(&self) -> bool {
        self.finalized
    }

    /// Sets the finalized flag (prevents double-finalization).
    pub fn set_finalized(&mut self, finalized: bool) {
        self.finalized = finalized;
    }
}

impl Trace for Userdata {
    fn trace(&self) {
        // Phase 7: mark metatable and env if they contain GC references.
    }
}

// ---------------------------------------------------------------------------
// Val
// ---------------------------------------------------------------------------

/// A Lua value.
///
/// All Lua values fit in this enum. Value types are stored inline (nil,
/// boolean, number). Reference types store a [`GcRef`] into the
/// appropriate GC arena. `Val` is `Copy` (16 bytes on 64-bit: 8 bytes
/// for the largest variant + 8 bytes tag/padding).
#[derive(Clone, Copy)]
pub enum Val {
    /// The nil value.
    Nil,
    /// A boolean value.
    Bool(bool),
    /// A double-precision floating-point number (all Lua 5.1 numbers).
    Num(f64),
    /// An interned string in the GC arena.
    Str(GcRef<LuaString>),
    /// A table in the GC arena.
    Table(GcRef<Table>),
    /// A closure (Lua or Rust) in the GC arena.
    Function(GcRef<Closure>),
    /// Full userdata in the GC arena.
    Userdata(GcRef<Userdata>),
    /// A coroutine thread in the GC arena.
    Thread(GcRef<LuaThread>),
    /// Light userdata: an opaque host value (typically a pointer).
    /// Compared and hashed by value. Not GC-managed.
    LightUserdata(usize),
}

impl Val {
    /// Returns `true` if this value is truthy.
    ///
    /// In Lua 5.1, only `nil` and `false` are falsy. Everything else
    /// (including `0`, `0.0`, and `""`) is truthy.
    #[inline]
    pub fn is_truthy(self) -> bool {
        !matches!(self, Self::Nil | Self::Bool(false))
    }

    /// Returns `true` if this value is `nil`.
    #[inline]
    pub fn is_nil(self) -> bool {
        matches!(self, Self::Nil)
    }

    /// Returns the Lua type name as returned by `type()`.
    pub fn type_name(self) -> &'static str {
        match self {
            Self::Nil => "nil",
            Self::Bool(_) => "boolean",
            Self::Num(_) => "number",
            Self::Str(_) => "string",
            Self::Table(_) => "table",
            Self::Function(_) => "function",
            Self::Userdata(_) | Self::LightUserdata(_) => "userdata",
            Self::Thread(_) => "thread",
        }
    }

    /// Returns the number value if this is `Val::Num`.
    #[inline]
    pub fn as_number(self) -> Option<f64> {
        match self {
            Self::Num(n) => Some(n),
            _ => None,
        }
    }

    /// Returns the boolean value if this is `Val::Bool`.
    #[inline]
    pub fn as_bool(self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(b),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// PartialEq (NOT Eq -- NaN breaks reflexivity)
// ---------------------------------------------------------------------------

impl PartialEq for Val {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Nil, Self::Nil) => true,
            (Self::Bool(a), Self::Bool(b)) => a == b,
            // IEEE 754: NaN != NaN, -0.0 == +0.0 (both handled by f64 PartialEq).
            (Self::Num(a), Self::Num(b)) => a == b,
            // Reference types: identity comparison via GcRef.
            (Self::Str(a), Self::Str(b)) => a == b,
            (Self::Table(a), Self::Table(b)) => a == b,
            (Self::Function(a), Self::Function(b)) => a == b,
            (Self::Userdata(a), Self::Userdata(b)) => a == b,
            (Self::Thread(a), Self::Thread(b)) => a == b,
            (Self::LightUserdata(a), Self::LightUserdata(b)) => a == b,
            // Different types are never equal (no implicit coercion).
            _ => false,
        }
    }
}

// ---------------------------------------------------------------------------
// Hash (consistent with PartialEq)
// ---------------------------------------------------------------------------

impl Hash for Val {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Hash the discriminant first to avoid cross-type collisions.
        std::mem::discriminant(self).hash(state);
        match self {
            Self::Nil => {}
            Self::Bool(b) => b.hash(state),
            Self::Num(n) => {
                // -0.0 == +0.0, so they must hash the same.
                if *n == 0.0 {
                    0.0_f64.to_bits().hash(state);
                } else {
                    n.to_bits().hash(state);
                }
            }
            Self::Str(r) => r.hash(state),
            Self::Table(r) => r.hash(state),
            Self::Function(r) => r.hash(state),
            Self::Userdata(r) => r.hash(state),
            Self::Thread(r) => r.hash(state),
            Self::LightUserdata(p) => p.hash(state),
        }
    }
}

// ---------------------------------------------------------------------------
// Debug
// ---------------------------------------------------------------------------

impl fmt::Debug for Val {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Nil => write!(f, "Nil"),
            Self::Bool(b) => write!(f, "Bool({b})"),
            Self::Num(n) => write!(f, "Num({n})"),
            Self::Str(r) => write!(f, "Str({r:?})"),
            Self::Table(r) => write!(f, "Table({r:?})"),
            Self::Function(r) => write!(f, "Function({r:?})"),
            Self::Userdata(r) => write!(f, "Userdata({r:?})"),
            Self::Thread(r) => write!(f, "Thread({r:?})"),
            Self::LightUserdata(p) => write!(f, "LightUserdata(0x{p:x})"),
        }
    }
}

// ---------------------------------------------------------------------------
// Display (Lua's tostring semantics)
// ---------------------------------------------------------------------------

impl fmt::Display for Val {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Nil => write!(f, "nil"),
            Self::Bool(b) => write!(f, "{b}"),
            Self::Num(n) => fmt_lua_number(f, *n),
            // Reference types display as "type: 0xINDEX".
            // String display will show content once LuaString is implemented.
            Self::Str(r) => write!(f, "string: 0x{:08x}", r.index()),
            Self::Table(r) => write!(f, "table: 0x{:08x}", r.index()),
            Self::Function(r) => write!(f, "function: 0x{:08x}", r.index()),
            Self::Userdata(r) => write!(f, "userdata: 0x{:08x}", r.index()),
            Self::Thread(r) => write!(f, "thread: 0x{:08x}", r.index()),
            Self::LightUserdata(p) => write!(f, "userdata: 0x{p:08x}"),
        }
    }
}

// ---------------------------------------------------------------------------
// Number formatting: match PUC-Rio's "%.14g"
// ---------------------------------------------------------------------------

/// Format a number matching C's `"%.14g"` (PUC-Rio's `lua_number2str`).
///
/// Rules:
/// 1. Use 14 significant digits maximum.
/// 2. If the exponent E satisfies -4 <= E < 14, use fixed-point.
/// 3. Otherwise, use scientific notation (`e+XX` / `e-XX`).
/// 4. Strip trailing zeros and trailing decimal point.
fn fmt_lua_number(f: &mut fmt::Formatter<'_>, n: f64) -> fmt::Result {
    if n.is_nan() {
        // Like C's "%.14g": the sign bit decides "nan" vs "-nan".
        return write!(f, "{}", if n.is_sign_negative() { "-nan" } else { "nan" });
    }
    if n.is_infinite() {
        return if n.is_sign_positive() {
            write!(f, "inf")
        } else {
            write!(f, "-inf")
        };
    }
    if n == 0.0 {
        // PUC-Rio's %.14g prints "-0" for negative zero.
        return if n.is_sign_negative() {
            write!(f, "-0")
        } else {
            write!(f, "0")
        };
    }

    let abs = n.abs();
    // Number of digits before the decimal point (0-based exponent).
    let exp10 = abs.log10().floor() as i32;

    if (-4..14).contains(&exp10) {
        // Fixed-point: compute decimal places for 14 significant digits.
        let dec = i32::max(13 - exp10, 0) as usize;
        // Format with exactly `dec` decimal places, then strip trailing zeros.
        let s = format!("{n:.dec$}");
        write!(f, "{}", strip_trailing_zeros(&s))
    } else {
        // Scientific notation: 13 decimal places = 14 significant digits.
        let s = format!("{n:.13e}");
        fmt_scientific_stripped(f, &s)
    }
}

/// Strip trailing zeros (and trailing decimal point) from a formatted number.
fn strip_trailing_zeros(s: &str) -> &str {
    if !s.contains('.') {
        return s;
    }
    let trimmed = s.trim_end_matches('0');
    trimmed.trim_end_matches('.')
}

/// Write a Rust scientific-notation string in C-style format with
/// stripped trailing zeros. Converts `1.23e5` to `1.23e+05`.
fn fmt_scientific_stripped(f: &mut fmt::Formatter<'_>, s: &str) -> fmt::Result {
    let Some(e_pos) = s.find('e') else {
        return write!(f, "{s}");
    };
    let mantissa = &s[..e_pos];
    let exp_str = &s[e_pos + 1..];

    // Strip trailing zeros from mantissa.
    let mantissa = strip_trailing_zeros(mantissa);

    // Parse exponent and format C-style (e+XX or e-XX, minimum 2 digits).
    let Ok(exp) = exp_str.parse::<i32>() else {
        return write!(f, "{s}");
    };

    if exp >= 0 {
        write!(f, "{mantissa}e+{exp:02}")
    } else {
        write!(f, "{mantissa}e-{:02}", exp.unsigned_abs())
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
    use crate::vm::string::lua_hash;
    use std::collections::hash_map::DefaultHasher;

    fn hash_val(v: &Val) -> u64 {
        let mut hasher = DefaultHasher::new();
        v.hash(&mut hasher);
        hasher.finish()
    }

    // -- Truthiness --

    #[test]
    fn nil_is_falsy() {
        assert!(!Val::Nil.is_truthy());
    }

    #[test]
    fn false_is_falsy() {
        assert!(!Val::Bool(false).is_truthy());
    }

    #[test]
    fn true_is_truthy() {
        assert!(Val::Bool(true).is_truthy());
    }

    #[test]
    fn zero_is_truthy() {
        assert!(Val::Num(0.0).is_truthy());
    }

    #[test]
    fn number_is_truthy() {
        assert!(Val::Num(42.0).is_truthy());
    }

    #[test]
    fn string_ref_is_truthy() {
        let mut arena: Arena<LuaString> = Arena::new();
        let r = arena.alloc(LuaString::new(b"test", lua_hash(b"test")), Color::White0);
        assert!(Val::Str(r).is_truthy());
    }

    #[test]
    fn table_ref_is_truthy() {
        let mut arena: Arena<Table> = Arena::new();
        let r = arena.alloc(Table::new(), Color::White0);
        assert!(Val::Table(r).is_truthy());
    }

    // -- Equality --

    #[test]
    fn nil_equals_nil() {
        assert_eq!(Val::Nil, Val::Nil);
    }

    #[test]
    fn bool_equality() {
        assert_eq!(Val::Bool(true), Val::Bool(true));
        assert_eq!(Val::Bool(false), Val::Bool(false));
        assert_ne!(Val::Bool(true), Val::Bool(false));
    }

    #[test]
    fn number_equality() {
        assert_eq!(Val::Num(1.0), Val::Num(1.0));
        assert_ne!(Val::Num(1.0), Val::Num(2.0));
    }

    #[test]
    fn nan_not_equal_to_nan() {
        let nan = Val::Num(f64::NAN);
        assert_ne!(nan, nan);
    }

    #[test]
    fn negative_zero_equals_positive_zero() {
        assert_eq!(Val::Num(-0.0), Val::Num(0.0));
    }

    #[test]
    fn different_types_not_equal() {
        assert_ne!(Val::Nil, Val::Bool(false));
        assert_ne!(Val::Num(0.0), Val::Bool(false));
        assert_ne!(Val::Num(1.0), Val::Bool(true));
    }

    #[test]
    fn string_identity_equality() {
        let mut arena: Arena<LuaString> = Arena::new();
        let r1 = arena.alloc(LuaString::new(b"test", lua_hash(b"test")), Color::White0);
        let r2 = arena.alloc(LuaString::new(b"test", lua_hash(b"test")), Color::White0);
        assert_eq!(Val::Str(r1), Val::Str(r1));
        assert_ne!(Val::Str(r1), Val::Str(r2));
    }

    #[test]
    fn table_identity_equality() {
        let mut arena: Arena<Table> = Arena::new();
        let r1 = arena.alloc(Table::new(), Color::White0);
        let r2 = arena.alloc(Table::new(), Color::White0);
        assert_eq!(Val::Table(r1), Val::Table(r1));
        assert_ne!(Val::Table(r1), Val::Table(r2));
    }

    // -- Hashing --

    #[test]
    fn equal_values_have_equal_hashes() {
        assert_eq!(hash_val(&Val::Nil), hash_val(&Val::Nil));
        assert_eq!(hash_val(&Val::Bool(true)), hash_val(&Val::Bool(true)));
        assert_eq!(hash_val(&Val::Num(42.0)), hash_val(&Val::Num(42.0)));
    }

    #[test]
    fn negative_zero_same_hash_as_positive_zero() {
        assert_eq!(hash_val(&Val::Num(-0.0)), hash_val(&Val::Num(0.0)));
    }

    #[test]
    fn different_types_different_hashes() {
        // These could theoretically collide, but in practice the
        // discriminant hashing prevents it.
        assert_ne!(hash_val(&Val::Nil), hash_val(&Val::Bool(false)));
    }

    // -- Type name --

    #[test]
    fn type_names() {
        assert_eq!(Val::Nil.type_name(), "nil");
        assert_eq!(Val::Bool(true).type_name(), "boolean");
        assert_eq!(Val::Num(1.0).type_name(), "number");
        assert_eq!(Val::LightUserdata(0).type_name(), "userdata");
    }

    #[test]
    fn type_name_for_refs() {
        use crate::vm::closure::{Closure, RustClosure};
        use crate::vm::state::LuaState;

        #[allow(clippy::unnecessary_wraps)]
        fn dummy(_: &mut LuaState) -> crate::error::LuaResult<u32> {
            Ok(0)
        }

        let mut strings: Arena<LuaString> = Arena::new();
        let mut tables: Arena<Table> = Arena::new();
        let mut closures: Arena<Closure> = Arena::new();
        let s = strings.alloc(LuaString::new(b"test", lua_hash(b"test")), Color::White0);
        let t = tables.alloc(Table::new(), Color::White0);
        let c = closures.alloc(
            Closure::Rust(RustClosure::new(dummy, "test")),
            Color::White0,
        );
        assert_eq!(Val::Str(s).type_name(), "string");
        assert_eq!(Val::Table(t).type_name(), "table");
        assert_eq!(Val::Function(c).type_name(), "function");
    }

    // -- Display / number formatting --

    #[test]
    fn display_nil() {
        assert_eq!(format!("{}", Val::Nil), "nil");
    }

    #[test]
    fn display_bool() {
        assert_eq!(format!("{}", Val::Bool(true)), "true");
        assert_eq!(format!("{}", Val::Bool(false)), "false");
    }

    #[test]
    fn display_integers() {
        assert_eq!(format!("{}", Val::Num(0.0)), "0");
        assert_eq!(format!("{}", Val::Num(1.0)), "1");
        assert_eq!(format!("{}", Val::Num(-1.0)), "-1");
        assert_eq!(format!("{}", Val::Num(42.0)), "42");
        assert_eq!(format!("{}", Val::Num(100.0)), "100");
    }

    #[test]
    fn display_fractions() {
        assert_eq!(format!("{}", Val::Num(1.5)), "1.5");
        assert_eq!(format!("{}", Val::Num(0.1)), "0.1");
    }

    #[test]
    fn display_large_number_scientific() {
        // %.14g switches to scientific at exponent >= 14.
        assert_eq!(format!("{}", Val::Num(1e15)), "1e+15");
        assert_eq!(format!("{}", Val::Num(1e20)), "1e+20");
    }

    #[test]
    fn display_small_number_scientific() {
        // %.14g switches to scientific at exponent < -4.
        assert_eq!(format!("{}", Val::Num(1e-5)), "1e-05");
    }

    #[test]
    fn display_small_number_fixed() {
        // Exponent -4 stays fixed.
        assert_eq!(format!("{}", Val::Num(0.0001)), "0.0001");
    }

    #[test]
    fn display_infinity() {
        assert_eq!(format!("{}", Val::Num(f64::INFINITY)), "inf");
        assert_eq!(format!("{}", Val::Num(f64::NEG_INFINITY)), "-inf");
    }

    #[test]
    fn display_nan() {
        assert_eq!(format!("{}", Val::Num(f64::NAN)), "nan");
        assert_eq!(format!("{}", Val::Num(-f64::NAN)), "-nan");
    }

    #[test]
    fn display_pi() {
        // PUC-Rio: "3.1415926535898" (14 significant digits).
        assert_eq!(
            format!("{}", Val::Num(std::f64::consts::PI)),
            "3.1415926535898"
        );
    }

    #[test]
    fn display_one_third() {
        // PUC-Rio: "0.33333333333333" (14 significant digits).
        assert_eq!(format!("{}", Val::Num(1.0 / 3.0)), "0.33333333333333");
    }

    // -- Accessors --

    #[test]
    fn as_number() {
        assert_eq!(Val::Num(42.0).as_number(), Some(42.0));
        assert_eq!(Val::Nil.as_number(), None);
        assert_eq!(Val::Bool(true).as_number(), None);
    }

    #[test]
    fn as_bool() {
        assert_eq!(Val::Bool(true).as_bool(), Some(true));
        assert_eq!(Val::Bool(false).as_bool(), Some(false));
        assert_eq!(Val::Nil.as_bool(), None);
    }

    #[test]
    fn is_nil() {
        assert!(Val::Nil.is_nil());
        assert!(!Val::Bool(false).is_nil());
        assert!(!Val::Num(0.0).is_nil());
    }

    // -- Userdata --

    #[test]
    fn userdata_new_unit() {
        let ud = Userdata::new(Box::new(()));
        assert!(ud.downcast_ref::<()>().is_some());
        assert!(ud.metatable().is_none());
        assert!(ud.env().is_none());
    }

    #[test]
    fn userdata_new_i32() {
        let ud = Userdata::new(Box::new(42_i32));
        assert_eq!(ud.downcast_ref::<i32>(), Some(&42));
        assert!(ud.downcast_ref::<String>().is_none());
    }

    #[test]
    fn userdata_new_string() {
        let ud = Userdata::new(Box::new(String::from("hello")));
        assert_eq!(
            ud.downcast_ref::<String>().map(String::as_str),
            Some("hello")
        );
        assert!(ud.downcast_ref::<i32>().is_none());
    }

    #[test]
    fn userdata_downcast_mut() {
        let mut ud = Userdata::new(Box::new(10_i32));
        if let Some(v) = ud.downcast_mut::<i32>() {
            *v = 20;
        }
        assert_eq!(ud.downcast_ref::<i32>(), Some(&20));
    }

    #[test]
    fn userdata_metatable() {
        let mut table_arena: Arena<Table> = Arena::new();
        let mt = table_arena.alloc(Table::new(), Color::White0);
        let ud = Userdata::with_metatable(Box::new(()), mt);
        assert_eq!(ud.metatable(), Some(mt));
    }

    #[test]
    fn userdata_set_metatable() {
        let mut table_arena: Arena<Table> = Arena::new();
        let mt = table_arena.alloc(Table::new(), Color::White0);
        let mut ud = Userdata::new(Box::new(()));
        assert!(ud.metatable().is_none());
        ud.set_metatable(Some(mt));
        assert_eq!(ud.metatable(), Some(mt));
        ud.set_metatable(None);
        assert!(ud.metatable().is_none());
    }

    #[test]
    fn userdata_env() {
        let mut table_arena: Arena<Table> = Arena::new();
        let env = table_arena.alloc(Table::new(), Color::White0);
        let mut ud = Userdata::new(Box::new(()));
        assert!(ud.env().is_none());
        ud.set_env(Some(env));
        assert_eq!(ud.env(), Some(env));
        ud.set_env(None);
        assert!(ud.env().is_none());
    }

    #[test]
    fn userdata_type_name() {
        let mut arena: Arena<Userdata> = Arena::new();
        let r = arena.alloc(Userdata::new(Box::new(())), Color::White0);
        assert_eq!(Val::Userdata(r).type_name(), "userdata");
    }

    #[test]
    fn userdata_is_truthy() {
        let mut arena: Arena<Userdata> = Arena::new();
        let r = arena.alloc(Userdata::new(Box::new(())), Color::White0);
        assert!(Val::Userdata(r).is_truthy());
    }

    #[test]
    fn userdata_identity_equality() {
        let mut arena: Arena<Userdata> = Arena::new();
        let r1 = arena.alloc(Userdata::new(Box::new(1_i32)), Color::White0);
        let r2 = arena.alloc(Userdata::new(Box::new(1_i32)), Color::White0);
        assert_eq!(Val::Userdata(r1), Val::Userdata(r1));
        assert_ne!(Val::Userdata(r1), Val::Userdata(r2));
    }
}
