//! Type conversion traits for moving values between Rust and Lua.
//!
//! Provides `IntoLua` / `FromLua` for single-value conversions and
//! `IntoLuaMulti` / `FromLuaMulti` for multi-value (vararg/return)
//! conversions. Standard implementations cover Rust primitives,
//! strings, `Option<T>`, and tuple arities up to 8.

use crate::api::{LuaApi, LuaApiMut};
use crate::error::{LuaError, LuaResult, RuntimeError};
use crate::handles::{AnyUserData, Function, Table, Thread};
use crate::vm::value::Val;

// ---------------------------------------------------------------------------
// Single-value traits
// ---------------------------------------------------------------------------

/// Converts a Rust value into a Lua `Val`.
///
/// Takes `&mut L` where `L: LuaApiMut` because operations like string interning require
/// mutable access to the GC.
pub trait IntoLua {
    /// Performs the conversion.
    fn into_lua<L: LuaApiMut>(self, lua: &mut L) -> LuaResult<Val>;
}

/// Extracts a Rust value from a Lua `Val`.
///
/// Takes `&L` where `L: LuaApi` (immutable) because reading does not mutate state.
pub trait FromLua: Sized {
    /// Performs the conversion.
    fn from_lua<L: LuaApi>(val: Val, lua: &L) -> LuaResult<Self>;
}

// ---------------------------------------------------------------------------
// Multi-value traits
// ---------------------------------------------------------------------------

/// Converts a Rust value into multiple Lua values.
pub trait IntoLuaMulti {
    /// Performs the conversion.
    fn into_lua_multi<L: LuaApiMut>(self, lua: &mut L) -> LuaResult<Vec<Val>>;
}

/// Extracts a Rust value from multiple Lua values.
pub trait FromLuaMulti: Sized {
    /// Performs the conversion.
    fn from_lua_multi<L: LuaApi>(values: &[Val], lua: &L) -> LuaResult<Self>;
}

// ---------------------------------------------------------------------------
// Val passthrough
// ---------------------------------------------------------------------------

impl IntoLua for Val {
    fn into_lua<L: LuaApiMut>(self, _lua: &mut L) -> LuaResult<Val> {
        Ok(self)
    }
}

impl FromLua for Val {
    fn from_lua<L: LuaApi>(val: Val, _lua: &L) -> LuaResult<Self> {
        Ok(val)
    }
}

// ---------------------------------------------------------------------------
// () <-> Nil
// ---------------------------------------------------------------------------

impl IntoLua for () {
    fn into_lua<L: LuaApiMut>(self, _lua: &mut L) -> LuaResult<Val> {
        Ok(Val::Nil)
    }
}

impl FromLua for () {
    fn from_lua<L: LuaApi>(_val: Val, _lua: &L) -> LuaResult<Self> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// bool <-> Bool
// ---------------------------------------------------------------------------

impl IntoLua for bool {
    fn into_lua<L: LuaApiMut>(self, _lua: &mut L) -> LuaResult<Val> {
        Ok(Val::Bool(self))
    }
}

impl FromLua for bool {
    fn from_lua<L: LuaApi>(val: Val, _lua: &L) -> LuaResult<Self> {
        match val {
            Val::Bool(b) => Ok(b),
            Val::Nil => Ok(false),
            _ => Ok(true),
        }
    }
}

// ---------------------------------------------------------------------------
// f64 / f32 <-> Num
// ---------------------------------------------------------------------------

impl IntoLua for f64 {
    fn into_lua<L: LuaApiMut>(self, _lua: &mut L) -> LuaResult<Val> {
        Ok(Val::Num(self))
    }
}

impl FromLua for f64 {
    fn from_lua<L: LuaApi>(val: Val, _lua: &L) -> LuaResult<Self> {
        match val {
            Val::Num(n) => Ok(n),
            _ => Err(conversion_error("number", val.type_name())),
        }
    }
}

impl IntoLua for f32 {
    fn into_lua<L: LuaApiMut>(self, _lua: &mut L) -> LuaResult<Val> {
        Ok(Val::Num(f64::from(self)))
    }
}

impl FromLua for f32 {
    fn from_lua<L: LuaApi>(val: Val, _lua: &L) -> LuaResult<Self> {
        match val {
            Val::Num(n) => Ok(n as Self),
            _ => Err(conversion_error("number", val.type_name())),
        }
    }
}

// ---------------------------------------------------------------------------
// Integer types <-> Num (with range checks on FromLua)
// ---------------------------------------------------------------------------

macro_rules! impl_integer_into_lua {
    ($($ty:ty),*) => {
        $(
            impl IntoLua for $ty {
                #[allow(clippy::cast_lossless, trivial_numeric_casts)]
                fn into_lua<L: LuaApi>(self, _lua: &mut L) -> LuaResult<Val> {
                    Ok(Val::Num(self as f64))
                }
            }
        )*
    };
}

impl_integer_into_lua!(i8, i16, i32, i64, u8, u16, u32, u64, isize, usize);

macro_rules! impl_integer_from_lua {
    ($($ty:ty),*) => {
        $(
            impl FromLua for $ty {
                #[allow(clippy::cast_lossless, trivial_numeric_casts)]
                fn from_lua<L: LuaApi>(val: Val, _lua: &L) -> LuaResult<Self> {
                    match val {
                        Val::Num(n) => {
                            if n.fract() != 0.0 {
                                return Err(conversion_error(
                                    concat!("integer (", stringify!($ty), ")"),
                                    "float",
                                ));
                            }
                            let min = <$ty>::MIN as f64;
                            let max = <$ty>::MAX as f64;
                            if n < min || n > max {
                                return Err(conversion_error(
                                    concat!("integer (", stringify!($ty), ")"),
                                    "number out of range",
                                ));
                            }
                            Ok(n as $ty)
                        }
                        _ => Err(conversion_error(
                            concat!("integer (", stringify!($ty), ")"),
                            val.type_name(),
                        )),
                    }
                }
            }
        )*
    };
}

impl_integer_from_lua!(i8, i16, i32, i64, u8, u16, u32, u64, isize, usize);

// ---------------------------------------------------------------------------
// String <-> Str
// ---------------------------------------------------------------------------

impl IntoLua for String {
    fn into_lua<L: LuaApiMut>(self, lua: &mut L) -> LuaResult<Val> {
        let r = lua.state_mut().gc.intern_string(self.as_bytes());
        Ok(Val::Str(r))
    }
}

impl IntoLua for &str {
    fn into_lua<L: LuaApiMut>(self, lua: &mut L) -> LuaResult<Val> {
        let r = lua.state_mut().gc.intern_string(self.as_bytes());
        Ok(Val::Str(r))
    }
}

impl IntoLua for &[u8] {
    fn into_lua<L: LuaApiMut>(self, lua: &mut L) -> LuaResult<Val> {
        let r = lua.state_mut().gc.intern_string(self);
        Ok(Val::Str(r))
    }
}

impl FromLua for String {
    fn from_lua<L: LuaApi>(val: Val, lua: &L) -> LuaResult<Self> {
        match val {
            Val::Str(r) => {
                let s = lua
                    .state()
                    .gc
                    .string_arena
                    .get(r)
                    .ok_or_else(|| conversion_error("string", "collected string"))?;
                Ok(Self::from_utf8_lossy(s.data()).into_owned())
            }
            _ => Err(conversion_error("string", val.type_name())),
        }
    }
}

impl FromLua for Vec<u8> {
    fn from_lua<L: LuaApi>(val: Val, lua: &L) -> LuaResult<Self> {
        match val {
            Val::Str(r) => {
                let s = lua
                    .state()
                    .gc
                    .string_arena
                    .get(r)
                    .ok_or_else(|| conversion_error("string", "collected string"))?;
                Ok(s.data().to_vec())
            }
            _ => Err(conversion_error("string", val.type_name())),
        }
    }
}

// ---------------------------------------------------------------------------
// Option<T> <-> nil / value
// ---------------------------------------------------------------------------

impl<T: IntoLua> IntoLua for Option<T> {
    fn into_lua<L: LuaApiMut>(self, lua: &mut L) -> LuaResult<Val> {
        match self {
            Some(v) => v.into_lua(lua),
            None => Ok(Val::Nil),
        }
    }
}

impl<T: FromLua> FromLua for Option<T> {
    fn from_lua<L: LuaApi>(val: Val, lua: &L) -> LuaResult<Self> {
        match val {
            Val::Nil => Ok(None),
            other => Ok(Some(T::from_lua(other, lua)?)),
        }
    }
}

// ---------------------------------------------------------------------------
// Handle types <-> Val
// ---------------------------------------------------------------------------

impl IntoLua for Table {
    fn into_lua<L: LuaApiMut>(self, _lua: &mut L) -> LuaResult<Val> {
        Ok(Val::Table(self.gc_ref()))
    }
}

impl FromLua for Table {
    fn from_lua<L: LuaApi>(val: Val, _lua: &L) -> LuaResult<Self> {
        match val {
            Val::Table(r) => Ok(Self(r)),
            _ => Err(conversion_error("table", val.type_name())),
        }
    }
}

impl IntoLua for Function {
    fn into_lua<L: LuaApiMut>(self, _lua: &mut L) -> LuaResult<Val> {
        Ok(Val::Function(self.gc_ref()))
    }
}

impl FromLua for Function {
    fn from_lua<L: LuaApi>(val: Val, _lua: &L) -> LuaResult<Self> {
        match val {
            Val::Function(r) => Ok(Self(r)),
            _ => Err(conversion_error("function", val.type_name())),
        }
    }
}

impl IntoLua for Thread {
    fn into_lua<L: LuaApiMut>(self, _lua: &mut L) -> LuaResult<Val> {
        Ok(Val::Thread(self.gc_ref()))
    }
}

impl FromLua for Thread {
    fn from_lua<L: LuaApi>(val: Val, _lua: &L) -> LuaResult<Self> {
        match val {
            Val::Thread(r) => Ok(Self(r)),
            _ => Err(conversion_error("thread", val.type_name())),
        }
    }
}

impl IntoLua for AnyUserData {
    fn into_lua<L: LuaApiMut>(self, _lua: &mut L) -> LuaResult<Val> {
        Ok(Val::Userdata(self.gc_ref()))
    }
}

impl FromLua for AnyUserData {
    fn from_lua<L: LuaApi>(val: Val, _lua: &L) -> LuaResult<Self> {
        match val {
            Val::Userdata(r) => Ok(Self(r)),
            _ => Err(conversion_error("userdata", val.type_name())),
        }
    }
}

// ---------------------------------------------------------------------------
// Vec<Val> passthrough for Multi traits
// ---------------------------------------------------------------------------

impl IntoLuaMulti for Vec<Val> {
    fn into_lua_multi<L: LuaApiMut>(self, _lua: &mut L) -> LuaResult<Vec<Val>> {
        Ok(self)
    }
}

impl FromLuaMulti for Vec<Val> {
    fn from_lua_multi<L: LuaApi>(values: &[Val], _lua: &L) -> LuaResult<Self> {
        Ok(values.to_vec())
    }
}

// ---------------------------------------------------------------------------
// () -> empty multi
// ---------------------------------------------------------------------------

impl IntoLuaMulti for () {
    fn into_lua_multi<L: LuaApiMut>(self, _lua: &mut L) -> LuaResult<Vec<Val>> {
        Ok(vec![])
    }
}

impl FromLuaMulti for () {
    fn from_lua_multi<L: LuaApi>(_values: &[Val], _lua: &L) -> LuaResult<Self> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tuple impls for IntoLuaMulti / FromLuaMulti (arity 1..=8)
// ---------------------------------------------------------------------------

macro_rules! impl_tuple_multi {
    ($($idx:tt : $T:ident),+) => {
        impl<$($T: IntoLua),+> IntoLuaMulti for ($($T,)+) {
            fn into_lua_multi<L: LuaApiMut>(self, lua: &mut L) -> LuaResult<Vec<Val>> {
                Ok(vec![
                    $(self.$idx.into_lua(lua)?,)+
                ])
            }
        }

        impl<$($T: FromLua),+> FromLuaMulti for ($($T,)+) {
            fn from_lua_multi<L: LuaApi>(values: &[Val], lua: &L) -> LuaResult<Self> {
                Ok((
                    $(
                        $T::from_lua(
                            values.get($idx).copied().unwrap_or(Val::Nil),
                            lua,
                        )?,
                    )+
                ))
            }
        }
    };
}

impl_tuple_multi!(0: A);
impl_tuple_multi!(0: A, 1: B);
impl_tuple_multi!(0: A, 1: B, 2: C);
impl_tuple_multi!(0: A, 1: B, 2: C, 3: D);
impl_tuple_multi!(0: A, 1: B, 2: C, 3: D, 4: E);
impl_tuple_multi!(0: A, 1: B, 2: C, 3: D, 4: E, 5: F);
impl_tuple_multi!(0: A, 1: B, 2: C, 3: D, 4: E, 5: F, 6: G);
impl_tuple_multi!(0: A, 1: B, 2: C, 3: D, 4: E, 5: F, 6: G, 7: H);

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn conversion_error(expected: &str, got: &str) -> LuaError {
    LuaError::Runtime(RuntimeError {
        message: format!("{expected} expected, got {got}"),
        level: 0,
        traceback: vec![],
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Lua;

    fn make_lua() -> Lua {
        Lua::new_empty()
    }

    // --- Val passthrough ---

    #[test]
    fn val_round_trip() {
        let mut lua = make_lua();
        let v = Val::Num(7.5);
        let converted = v.into_lua(&mut lua);
        assert!(converted.is_ok());
        assert_eq!(converted.ok(), Some(Val::Num(7.5)));

        let back = Val::from_lua(Val::Num(7.5), &lua);
        assert!(back.is_ok());
        assert_eq!(back.ok(), Some(Val::Num(7.5)));
    }

    // --- () <-> Nil ---

    #[test]
    fn unit_to_nil() {
        let mut lua = make_lua();
        let v = ().into_lua(&mut lua);
        assert_eq!(v.ok(), Some(Val::Nil));
    }

    #[test]
    fn unit_from_anything() {
        let lua = make_lua();
        let v = <()>::from_lua(Val::Num(42.0), &lua);
        assert!(v.is_ok());
    }

    // --- bool ---

    #[test]
    fn bool_round_trip() {
        let mut lua = make_lua();
        let v = true.into_lua(&mut lua);
        assert_eq!(v.ok(), Some(Val::Bool(true)));

        let back = bool::from_lua(Val::Bool(true), &lua);
        assert_eq!(back.ok(), Some(true));
    }

    #[test]
    fn bool_from_nil_is_false() {
        let lua = make_lua();
        let v = bool::from_lua(Val::Nil, &lua);
        assert_eq!(v.ok(), Some(false));
    }

    #[test]
    fn bool_from_number_is_true() {
        let lua = make_lua();
        let v = bool::from_lua(Val::Num(0.0), &lua);
        assert_eq!(v.ok(), Some(true));
    }

    // --- f64 / f32 ---

    #[test]
    fn f64_round_trip() {
        let mut lua = make_lua();
        let v = 9.75f64.into_lua(&mut lua);
        assert_eq!(v.ok(), Some(Val::Num(9.75)));

        let back = f64::from_lua(Val::Num(9.75), &lua);
        assert_eq!(back.ok(), Some(9.75));
    }

    #[test]
    fn f64_from_string_fails() {
        let mut lua = make_lua();
        let r = lua.state_mut().gc.intern_string(b"hello");
        let v = f64::from_lua(Val::Str(r), &lua);
        assert!(v.is_err());
    }

    // --- Integer types ---

    #[test]
    fn i32_round_trip() {
        let mut lua = make_lua();
        let v = 42i32.into_lua(&mut lua);
        assert_eq!(v.ok(), Some(Val::Num(42.0)));

        let back = i32::from_lua(Val::Num(42.0), &lua);
        assert_eq!(back.ok(), Some(42));
    }

    #[test]
    fn i32_from_float_fails() {
        let lua = make_lua();
        let v = i32::from_lua(Val::Num(5.75), &lua);
        assert!(v.is_err());
    }

    #[test]
    fn u8_overflow_fails() {
        let lua = make_lua();
        let v = u8::from_lua(Val::Num(256.0), &lua);
        assert!(v.is_err());
    }

    #[test]
    fn u8_negative_fails() {
        let lua = make_lua();
        let v = u8::from_lua(Val::Num(-1.0), &lua);
        assert!(v.is_err());
    }

    // --- String ---

    #[test]
    fn string_round_trip() {
        let mut lua = make_lua();
        let val = "hello".into_lua(&mut lua).ok().unwrap_or(Val::Nil);
        assert!(matches!(val, Val::Str(_)));

        let back = String::from_lua(val, &lua);
        assert_eq!(back.ok(), Some("hello".to_string()));
    }

    #[test]
    fn string_owned_round_trip() {
        let mut lua = make_lua();
        let v = String::from("world").into_lua(&mut lua);
        assert!(v.is_ok());
    }

    #[test]
    fn bytes_round_trip() {
        let mut lua = make_lua();
        let val = b"binary\x00data"
            .as_slice()
            .into_lua(&mut lua)
            .ok()
            .unwrap_or(Val::Nil);
        assert!(matches!(val, Val::Str(_)));

        let back = Vec::<u8>::from_lua(val, &lua);
        assert_eq!(back.ok(), Some(b"binary\x00data".to_vec()));
    }

    // --- Option ---

    #[test]
    fn option_some() {
        let mut lua = make_lua();
        let v = Some(42.0f64).into_lua(&mut lua);
        assert_eq!(v.ok(), Some(Val::Num(42.0)));

        let back = Option::<f64>::from_lua(Val::Num(42.0), &lua);
        assert_eq!(back.ok(), Some(Some(42.0)));
    }

    #[test]
    fn option_none() {
        let mut lua = make_lua();
        let v = Option::<f64>::None.into_lua(&mut lua);
        assert_eq!(v.ok(), Some(Val::Nil));

        let back = Option::<f64>::from_lua(Val::Nil, &lua);
        assert_eq!(back.ok(), Some(None));
    }

    // --- AnyUserData ---

    #[test]
    fn anyuserdata_into_from_lua() {
        let mut lua = make_lua();
        let ud = crate::vm::value::Userdata::new(Box::new(123i64));
        let r = lua.state_mut().gc.alloc_userdata(ud);
        let handle = AnyUserData(r);

        let val = handle.into_lua(&mut lua).ok().unwrap_or(Val::Nil);
        assert!(matches!(val, Val::Userdata(_)));

        let back = AnyUserData::from_lua(val, &lua);
        assert!(back.is_ok());
        assert_eq!(back.ok().map(AnyUserData::gc_ref), Some(r));
    }

    #[test]
    fn anyuserdata_from_wrong_type() {
        let lua = make_lua();
        let result = AnyUserData::from_lua(Val::Num(42.0), &lua);
        assert!(result.is_err());
    }

    // --- Handle types ---

    #[test]
    fn table_handle_conversion() {
        let mut lua = make_lua();
        let t = lua.create_table();
        let val = t.into_lua(&mut lua).ok().unwrap_or(Val::Nil);
        assert!(matches!(val, Val::Table(_)));

        let back = Table::from_lua(val, &lua);
        assert!(back.is_ok());
        assert_eq!(back.ok().map(Table::gc_ref), Some(t.gc_ref()));
    }

    // --- Multi ---

    #[test]
    fn vec_val_multi_round_trip() {
        let mut lua = make_lua();
        let vals = vec![Val::Num(1.0), Val::Num(2.0)];
        let multi = vals.into_lua_multi(&mut lua);
        assert_eq!(multi.ok().map(|v| v.len()), Some(2));
    }

    #[test]
    fn unit_multi() {
        let mut lua = make_lua();
        let multi = ().into_lua_multi(&mut lua);
        assert_eq!(multi.ok().map(|v| v.len()), Some(0));
    }

    #[test]
    fn tuple_from_multi() {
        let lua = make_lua();
        let vals = [Val::Num(1.0), Val::Num(2.0)];
        let result = <(f64, f64)>::from_lua_multi(&vals, &lua);
        assert_eq!(result.ok(), Some((1.0, 2.0)));
    }

    #[test]
    fn tuple_from_multi_missing_values() {
        let lua = make_lua();
        let vals = [Val::Num(1.0)];
        let result = <(f64, Option<f64>)>::from_lua_multi(&vals, &lua);
        assert_eq!(result.ok(), Some((1.0, None)));
    }
}
