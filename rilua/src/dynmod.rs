//! Dynamic module loading support for rilua-native modules.
//!
//! This module defines the ABI contract between rilua and dynamically loaded
//! native modules. Modules are Rust `cdylib` crates compiled against the same
//! rilua version and `rustc` version as the host.
//!
//! # Module ABI
//!
//! A rilua-native module exports two symbols:
//!
//! 1. `RILUA_MODULE_INFO` — a [`RiluaModuleInfo`] struct for version/ABI
//!    validation.
//! 2. `rilua_open_<modname>` — a [`RiluaModuleEntry`] function that receives
//!    `*mut LuaState` and returns the number of values pushed (or negative on
//!    error).
//!
//! Module authors use [`export_module_info!`](crate::export_module_info) to generate the info symbol.
//!
//! # Safety
//!
//! All unsafe code in this module is feature-gated behind `dynmod`. The host
//! validates `RILUA_MODULE_INFO` before calling any module code, and wraps
//! entry point calls in `catch_unwind` to convert panics to Lua errors.

use crate::platform::dynlib::DynLib;
use crate::vm::state::LuaState;
use crate::vm::value::Val;

// ---------------------------------------------------------------------------
// Version number
// ---------------------------------------------------------------------------

/// Computes the rilua version number as `MAJOR * 10000 + MINOR * 100 + PATCH`.
///
/// Parsed from `CARGO_PKG_VERSION` at compile time.
pub const RILUA_VERSION_NUM: u32 = {
    // env!("CARGO_PKG_VERSION") is "0.1.5" etc.
    // We parse it manually in a const context.
    let bytes = env!("CARGO_PKG_VERSION").as_bytes();
    let mut major: u32 = 0;
    let mut minor: u32 = 0;
    let mut patch: u32 = 0;
    let mut part = 0u8; // 0=major, 1=minor, 2=patch
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'.' {
            part += 1;
        } else {
            let digit = (b - b'0') as u32;
            match part {
                0 => major = major * 10 + digit,
                1 => minor = minor * 10 + digit,
                _ => patch = patch * 10 + digit,
            }
        }
        i += 1;
    }
    major * 10000 + minor * 100 + patch
};

// ---------------------------------------------------------------------------
// Module info struct
// ---------------------------------------------------------------------------

/// ABI descriptor exported by every rilua-native module.
///
/// The host validates these fields before calling any module code:
/// - `magic` must be `b"rilua\0"`.
/// - `rilua_version` must match the host's [`RILUA_VERSION_NUM`].
/// - `state_size` and `val_size` must match `size_of::<LuaState>()` and
///   `size_of::<Val>()` respectively.
#[repr(C)]
pub struct RiluaModuleInfo {
    /// Magic bytes: `b"rilua\0"`.
    pub magic: [u8; 6],
    /// Version number: `MAJOR*10000 + MINOR*100 + PATCH`.
    pub rilua_version: u32,
    /// `std::mem::size_of::<LuaState>()` at module compile time.
    pub state_size: u32,
    /// `std::mem::size_of::<Val>()` at module compile time.
    pub val_size: u32,
}

/// Expected magic bytes in [`RiluaModuleInfo`].
pub const RILUA_MODULE_MAGIC: [u8; 6] = *b"rilua\0";

// ---------------------------------------------------------------------------
// Entry point type
// ---------------------------------------------------------------------------

/// Signature of a rilua-native module entry point.
///
/// The function receives a raw pointer to `LuaState`, pushes values onto the
/// stack, and returns the count of values pushed (typically 1: the module
/// table). Returns negative on error.
pub type RiluaModuleEntry = unsafe extern "C" fn(state: *mut LuaState) -> i32;

// ---------------------------------------------------------------------------
// Export macro
// ---------------------------------------------------------------------------

/// Generates the `RILUA_MODULE_INFO` symbol for a rilua-native module.
///
/// Place this at the top level of your cdylib crate:
///
/// ```ignore
/// rilua::export_module_info!();
/// ```
#[macro_export]
macro_rules! export_module_info {
    () => {
        #[unsafe(no_mangle)]
        pub static RILUA_MODULE_INFO: $crate::dynmod::RiluaModuleInfo =
            $crate::dynmod::RiluaModuleInfo {
                magic: *b"rilua\0",
                rilua_version: $crate::dynmod::RILUA_VERSION_NUM,
                state_size: std::mem::size_of::<$crate::vm::state::LuaState>() as u32,
                val_size: std::mem::size_of::<$crate::vm::value::Val>() as u32,
            };
    };
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// Validates a loaded library's `RILUA_MODULE_INFO` against the host.
///
/// Returns `Ok(())` if the module is ABI-compatible, or `Err(message)`
/// describing the mismatch.
#[allow(unsafe_code)]
pub(crate) fn validate_module_info(lib: &DynLib) -> Result<(), String> {
    let ptr = lib.symbol("RILUA_MODULE_INFO")?;
    let info = unsafe { &*(ptr as *const RiluaModuleInfo) };

    if info.magic != RILUA_MODULE_MAGIC {
        return Err(format!(
            "'{}' is not a rilua module (bad magic)",
            lib.path()
        ));
    }

    if info.rilua_version != RILUA_VERSION_NUM {
        return Err(format!(
            "module '{}' was compiled for rilua version {} but host is version {}",
            lib.path(),
            info.rilua_version,
            RILUA_VERSION_NUM
        ));
    }

    let host_state_size = std::mem::size_of::<LuaState>() as u32;
    if info.state_size != host_state_size {
        return Err(format!(
            "module '{}' ABI mismatch: LuaState size {} vs host {}",
            lib.path(),
            info.state_size,
            host_state_size
        ));
    }

    let host_val_size = std::mem::size_of::<Val>() as u32;
    if info.val_size != host_val_size {
        return Err(format!(
            "module '{}' ABI mismatch: Val size {} vs host {}",
            lib.path(),
            info.val_size,
            host_val_size
        ));
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Module loading helper
// ---------------------------------------------------------------------------

/// Loads a module entry point from a validated library.
///
/// The caller must have already called [`validate_module_info`] on this
/// library. Returns the entry point function pointer.
#[allow(unsafe_code)]
pub(crate) fn load_entry_point(
    lib: &DynLib,
    symbol_name: &str,
) -> Result<RiluaModuleEntry, String> {
    let ptr = lib.symbol(symbol_name)?;
    if ptr.is_null() {
        return Err(format!(
            "symbol '{}' is null in '{}'",
            symbol_name,
            lib.path()
        ));
    }
    let func: RiluaModuleEntry = unsafe { std::mem::transmute(ptr) };
    Ok(func)
}

/// Calls a module entry point with panic catching.
///
/// Wraps the call in `catch_unwind` to convert panics into Lua errors.
/// Returns the number of values pushed on success.
#[allow(unsafe_code)]
pub(crate) fn call_entry_point(
    entry: RiluaModuleEntry,
    state: &mut LuaState,
) -> Result<i32, String> {
    let state_ptr: *mut LuaState = state;
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe { entry(state_ptr) })) {
        Ok(n) => {
            if n < 0 {
                Err(format!("module entry point returned error code {n}"))
            } else {
                Ok(n)
            }
        }
        Err(_) => Err("module entry point panicked".to_string()),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_num_parses_correctly() {
        // For version "0.1.5", should be 0*10000 + 1*100 + 5 = 105.
        let version = env!("CARGO_PKG_VERSION");
        let parts: Vec<u32> = version.split('.').map(|s| s.parse().unwrap_or(0)).collect();
        let expected = parts[0] * 10000 + parts[1] * 100 + parts[2];
        assert_eq!(RILUA_VERSION_NUM, expected);
    }

    #[test]
    fn module_magic_is_correct() {
        assert_eq!(&RILUA_MODULE_MAGIC, b"rilua\0");
    }
}
