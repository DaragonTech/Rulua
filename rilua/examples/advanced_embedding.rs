//! Advanced embedding example for rilua.
//!
//! Demonstrates:
//! - Creating userdata with custom metatables (Vec2 type)
//! - Registering Rust functions that interact with userdata
//! - Using `IntoLua`/`FromLua` traits for type conversion
//! - Calling Lua functions from Rust and reading results
//! - Coroutine interaction from Rust
//! - Error handling patterns
//!
//! Usage:
//!     cargo run --example advanced_embedding

#![allow(clippy::expect_used)]

use rilua::vm::state::LuaState;
use rilua::vm::value::Userdata;
use rilua::{
    FromLua, Function, Lua, LuaApi, LuaApiMut, LuaError, LuaResult, RuntimeError, StdLib, Table,
    Thread, Val, runtime_error,
};

// ---------------------------------------------------------------------------
// Vec2 userdata type
// ---------------------------------------------------------------------------

/// A 2D vector stored as Lua userdata.
#[derive(Debug, Clone, Copy)]
struct Vec2 {
    x: f64,
    y: f64,
}

/// Helper: extract a `Vec2` from a stack argument.
fn get_vec2(state: &LuaState, idx: usize) -> LuaResult<Vec2> {
    let val = state.stack_get(state.base + idx);
    match val {
        Val::Userdata(r) => {
            let ud = state
                .gc
                .userdata
                .get(r)
                .ok_or_else(|| runtime_error("userdata has been collected"))?;
            ud.downcast_ref::<Vec2>()
                .copied()
                .ok_or_else(|| runtime_error("expected Vec2 userdata"))
        }
        _ => Err(runtime_error(format!(
            "Vec2 expected, got {}",
            val.type_name()
        ))),
    }
}

/// Allocates a new `Vec2` userdata with the "Vec2" metatable.
///
/// This demonstrates how RustFn implementations create userdata at the
/// `LuaState` level. The metatable is looked up from the registry where
/// it was stored during setup.
fn push_vec2(state: &mut LuaState, v: Vec2) -> Val {
    let mt = rilua::stdlib::get_registry_metatable(state, "Vec2");
    let ud = match mt {
        Some(mt_ref) => Userdata::with_metatable(Box::new(v), mt_ref),
        None => Userdata::new(Box::new(v)),
    };
    let r = state.gc.alloc_userdata(ud);
    Val::Userdata(r)
}

// -- Native functions exposed to Lua --

/// `vec2_new(x, y) -> Vec2`
fn vec2_new(state: &mut LuaState) -> LuaResult<u32> {
    let x = match state.stack_get(state.base) {
        Val::Num(n) => n,
        other => {
            return Err(runtime_error(format!(
                "vec2_new: number expected for x, got {}",
                other.type_name()
            )));
        }
    };
    let y = match state.stack_get(state.base + 1) {
        Val::Num(n) => n,
        other => {
            return Err(runtime_error(format!(
                "vec2_new: number expected for y, got {}",
                other.type_name()
            )));
        }
    };

    let val = push_vec2(state, Vec2 { x, y });
    state.push(val);
    Ok(1)
}

/// `vec2_tostring(v) -> string`
fn vec2_tostring(state: &mut LuaState) -> LuaResult<u32> {
    let v = get_vec2(state, 0)?;
    let s = format!("Vec2({}, {})", v.x, v.y);
    let str_ref = state.gc.intern_string(s.as_bytes());
    state.push(Val::Str(str_ref));
    Ok(1)
}

/// `__add` metamethod: Vec2 + Vec2
fn vec2_add(state: &mut LuaState) -> LuaResult<u32> {
    let a = get_vec2(state, 0)?;
    let b = get_vec2(state, 1)?;
    let val = push_vec2(
        state,
        Vec2 {
            x: a.x + b.x,
            y: a.y + b.y,
        },
    );
    state.push(val);
    Ok(1)
}

/// `__mul` metamethod: Vec2 * number
fn vec2_mul(state: &mut LuaState) -> LuaResult<u32> {
    let a = get_vec2(state, 0)?;
    let Val::Num(s) = state.stack_get(state.base + 1) else {
        return Err(runtime_error(
            "Vec2 __mul: number expected as second operand",
        ));
    };
    let val = push_vec2(
        state,
        Vec2 {
            x: a.x * s,
            y: a.y * s,
        },
    );
    state.push(val);
    Ok(1)
}

/// `__len` metamethod: #Vec2 returns the magnitude
fn vec2_len(state: &mut LuaState) -> LuaResult<u32> {
    let v = get_vec2(state, 0)?;
    let mag = v.x.hypot(v.y);
    state.push(Val::Num(mag));
    Ok(1)
}

/// `__eq` metamethod: Vec2 == Vec2
fn vec2_eq(state: &mut LuaState) -> LuaResult<u32> {
    let a = get_vec2(state, 0)?;
    let b = get_vec2(state, 1)?;
    state.push(Val::Bool(a.x == b.x && a.y == b.y));
    Ok(1)
}

// ---------------------------------------------------------------------------
// A simple Rust function: rust_add(a, b) -> number
// ---------------------------------------------------------------------------

fn rust_add_fn(state: &mut LuaState) -> LuaResult<u32> {
    let a = match state.stack_get(state.base) {
        Val::Num(n) => n,
        other => {
            return Err(runtime_error(format!(
                "rust_add: number expected, got {}",
                other.type_name()
            )));
        }
    };
    let b = match state.stack_get(state.base + 1) {
        Val::Num(n) => n,
        other => {
            return Err(runtime_error(format!(
                "rust_add: number expected, got {}",
                other.type_name()
            )));
        }
    };
    state.push(Val::Num(a + b));
    Ok(1)
}

// ---------------------------------------------------------------------------
// Setup: create Vec2 metatable and register globals
// ---------------------------------------------------------------------------

fn setup(lua: &mut Lua) -> LuaResult<()> {
    // Create the Vec2 metatable (cached in the registry by name).
    // All Vec2 userdata share this single metatable.
    let mt = lua.create_userdata_metatable("Vec2")?;

    // Set metamethods on the metatable using the convenience method.
    lua.table_set_function(&mt, "__add", vec2_add)?;
    lua.table_set_function(&mt, "__mul", vec2_mul)?;
    lua.table_set_function(&mt, "__len", vec2_len)?;
    lua.table_set_function(&mt, "__eq", vec2_eq)?;
    lua.table_set_function(&mt, "__tostring", vec2_tostring)?;

    // Register global functions.
    lua.register_function("vec2_new", vec2_new)?;
    lua.register_function("vec2_tostring", vec2_tostring)?;
    lua.register_function("rust_add", rust_add_fn)?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Demonstrate calling Lua from Rust
// ---------------------------------------------------------------------------

fn demo_call_lua_function(lua: &mut Lua) -> LuaResult<()> {
    println!("\n=== Calling Lua functions from Rust ===");

    // Call make_config() defined in advanced.lua.
    let make_config: Function = lua.global("make_config")?;
    let results = lua.call_function(&make_config, &[])?;

    if let Some(Val::Table(_)) = results.first() {
        let config: Table = Table::from_lua(results[0], lua)?;

        // Pre-create string keys (create_string needs &mut, raw_get needs &).
        let key_name = lua.create_string(b"name");
        let key_version = lua.create_string(b"version");
        let key_features = lua.create_string(b"features");

        // Read string field using table_raw_get.
        let name_val = lua.table_raw_get(&config, key_name)?;
        let name = String::from_lua(name_val, lua)?;
        println!("config.name = {name}");

        // Read number field.
        let version_val = lua.table_raw_get(&config, key_version)?;
        let version = f64::from_lua(version_val, lua)?;
        println!("config.version = {version}");

        // Read array field (integer keys don't need interning).
        let features_val = lua.table_raw_get(&config, key_features)?;
        if let Val::Table(_) = features_val {
            let features = Table::from_lua(features_val, lua)?;
            let len = lua.table_raw_len(&features);
            print!("config.features = [");
            for i in 1..=len {
                let v = lua.table_raw_get(&features, Val::Num(i as f64))?;
                let s = String::from_lua(v, lua)?;
                if i > 1 {
                    print!(", ");
                }
                print!("{s}");
            }
            println!("]");
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Demonstrate coroutine interaction from Rust
// ---------------------------------------------------------------------------

fn demo_coroutine(lua: &mut Lua) -> LuaResult<()> {
    println!("\n=== Coroutine interaction from Rust ===");

    // Call make_counter(1, 5) to get a coroutine thread.
    let make_counter: Function = lua.global("make_counter")?;
    let results = lua.call_function(&make_counter, &[Val::Num(1.0), Val::Num(5.0)])?;

    let thread_val = results
        .first()
        .copied()
        .ok_or_else(|| runtime_error("make_counter returned no value"))?;
    let thread = Thread::from_lua(thread_val, lua)?;

    // Store the thread as a global so we can resume it via Lua code.
    lua.set_global("_counter_co", thread)?;

    println!("Resuming coroutine from Rust:");
    loop {
        // Use coroutine.resume via a loaded chunk.
        let resume_fn: Function = lua.load("return coroutine.resume(_counter_co)")?;
        let results = lua.call_function(&resume_fn, &[])?;

        let ok = results.first().copied().unwrap_or(Val::Nil);
        let value = results.get(1).copied().unwrap_or(Val::Nil);

        match ok {
            Val::Bool(true) => match value {
                Val::Num(n) => println!("  yielded: {n}"),
                Val::Str(_) => {
                    let s = String::from_lua(value, lua)?;
                    println!("  returned: {s}");
                }
                _ => println!("  returned: {value}"),
            },
            Val::Bool(false) => {
                println!("  coroutine finished");
                break;
            }
            _ => break,
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Demonstrate error handling from Rust
// ---------------------------------------------------------------------------

fn demo_error_handling(lua: &mut Lua) -> LuaResult<()> {
    println!("\n=== Error handling from Rust ===");

    // Attempt to execute code that raises an error.
    let result = lua.exec("error('boom from Lua')");
    match result {
        Ok(()) => println!("  (no error)"),
        Err(e) => println!("  caught error: {e}"),
    }

    // Load a function and call it with traced errors (includes stack trace).
    let bad_fn = lua.load("return 1 + nil")?;
    let result = lua.call_function_traced(&bad_fn, &[]);
    match result {
        Ok(vals) => println!("  result: {vals:?}"),
        Err(e) => println!("  traced error: {e}"),
    }

    // Demonstrate creating a custom Rust error.
    let custom_err: LuaError = runtime_error("custom error from Rust");
    println!("  custom error: {custom_err}");

    // Demonstrate RuntimeError with fields.
    let detailed_err = LuaError::Runtime(RuntimeError {
        message: "detailed error".into(),
        level: 1,
        traceback: vec![],
    });
    println!("  detailed error: {detailed_err}");

    Ok(())
}

// ---------------------------------------------------------------------------
// Demonstrate IntoLua / FromLua type conversions
// ---------------------------------------------------------------------------

fn demo_type_conversions(lua: &mut Lua) -> LuaResult<()> {
    println!("\n=== Type conversions (IntoLua / FromLua) ===");

    // Rust -> Lua -> Rust round trips via set_global / global.
    lua.set_global("my_int", 42i32)?;
    lua.set_global("my_str", "hello from Rust")?;
    lua.set_global("my_bool", true)?;
    lua.set_global("my_float", 9.81f64)?;
    lua.set_global("my_nil", Option::<i32>::None)?;

    let int_val: i32 = lua.global("my_int")?;
    let str_val: String = lua.global("my_str")?;
    let bool_val: bool = lua.global("my_bool")?;
    let float_val: f64 = lua.global("my_float")?;
    let opt_val: Option<i32> = lua.global("my_nil")?;

    println!("  i32:       {int_val}");
    println!("  String:    {str_val}");
    println!("  bool:      {bool_val}");
    println!("  f64:       {float_val}");
    println!("  Option:    {opt_val:?}");

    // Multi-value return: call a Lua function that returns two values.
    lua.exec("function swap(a, b) return b, a end")?;
    let swap_fn: Function = lua.global("swap")?;
    let results = lua.call_function(&swap_fn, &[Val::Num(1.0), Val::Num(2.0)])?;
    println!(
        "  swap(1, 2) = ({}, {})",
        results.first().copied().unwrap_or(Val::Nil),
        results.get(1).copied().unwrap_or(Val::Nil),
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn main() {
    let mut lua = Lua::new_with(StdLib::BASE | StdLib::STRING | StdLib::COROUTINE)
        .expect("failed to create Lua state");

    // Set up Vec2 userdata type and register native functions.
    if let Err(e) = setup(&mut lua) {
        eprintln!("setup error: {e}");
        std::process::exit(1);
    }

    // Run the companion Lua script (defines functions + exercises Vec2).
    if let Err(e) = lua.exec_file("examples/advanced.lua") {
        eprintln!("script error: {e}");
        std::process::exit(1);
    }

    // Demonstrate features from the Rust side.
    if let Err(e) = demo_type_conversions(&mut lua) {
        eprintln!("type conversion error: {e}");
        std::process::exit(1);
    }

    if let Err(e) = demo_call_lua_function(&mut lua) {
        eprintln!("call lua function error: {e}");
        std::process::exit(1);
    }

    if let Err(e) = demo_coroutine(&mut lua) {
        eprintln!("coroutine error: {e}");
        std::process::exit(1);
    }

    if let Err(e) = demo_error_handling(&mut lua) {
        eprintln!("error handling error: {e}");
        std::process::exit(1);
    }

    println!("\nAll demos completed.");
}
