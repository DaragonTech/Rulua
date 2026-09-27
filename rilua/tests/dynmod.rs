//! Integration tests for the `dynmod` feature.
//!
//! These tests build the example native module, load it via `package.loadlib`,
//! and verify the full load-call-gc cycle.
//!
//! Requires: `cargo test --features dynmod`

#![cfg(feature = "dynmod")]
#![allow(clippy::expect_used)]

use std::path::PathBuf;
use std::process::Command;

use rilua::LuaApiMut;

/// Builds the example native module and returns the path to the shared library.
fn build_example_module() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let module_dir = manifest_dir.join("examples/native_module");

    let output = Command::new("cargo")
        .arg("build")
        .arg("--manifest-path")
        .arg(module_dir.join("Cargo.toml"))
        .output()
        .expect("failed to build example module");

    assert!(
        output.status.success(),
        "failed to build example module: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Find the built library.
    let lib_name = if cfg!(target_os = "macos") {
        "libhello.dylib"
    } else if cfg!(target_os = "windows") {
        "hello.dll"
    } else {
        "libhello.so"
    };

    module_dir.join("target/debug").join(lib_name)
}

#[test]
fn loadlib_opens_valid_module() {
    let lib_path = build_example_module();
    assert!(lib_path.exists(), "built library not found: {lib_path:?}");

    let mut lua = rilua::Lua::new().expect("failed to create Lua state");

    // Use package.loadlib to load the entry point.
    let code = format!(
        r#"
        local f, err, kind = package.loadlib("{}", "rilua_open_hello")
        if not f then
            error("loadlib failed: " .. tostring(err) .. " (" .. tostring(kind) .. ")")
        end
        local mod = f()
        assert(type(mod) == "table", "expected table, got " .. type(mod))
        assert(mod.VERSION == "0.1.0", "bad VERSION: " .. tostring(mod.VERSION))
        local greeting = mod.greet("Lua")
        assert(greeting == "Hello, Lua!", "bad greeting: " .. tostring(greeting))
        RESULT = greeting
        "#,
        lib_path.display()
    );

    lua.exec(&code).expect("loadlib test failed");

    let result: String = lua.global("RESULT").expect("RESULT not set");
    assert_eq!(result, "Hello, Lua!");
}

#[test]
fn loadlib_greet_default() {
    let lib_path = build_example_module();

    let mut lua = rilua::Lua::new().expect("failed to create Lua state");

    let code = format!(
        r#"
        local f = package.loadlib("{}", "rilua_open_hello")
        local mod = f()
        RESULT = mod.greet()
        "#,
        lib_path.display()
    );

    lua.exec(&code).expect("greet default test failed");

    let result: String = lua.global("RESULT").expect("RESULT not set");
    assert_eq!(result, "Hello, world!");
}

#[test]
fn loadlib_bad_symbol_returns_init_error() {
    let lib_path = build_example_module();

    let mut lua = rilua::Lua::new().expect("failed to create Lua state");

    let code = format!(
        r#"
        local f, err, kind = package.loadlib("{}", "nonexistent_symbol")
        assert(f == nil, "expected nil function")
        assert(kind == "init", "expected 'init' error, got: " .. tostring(kind))
        RESULT = kind
        "#,
        lib_path.display()
    );

    lua.exec(&code).expect("bad symbol test failed");

    let result: String = lua.global("RESULT").expect("RESULT not set");
    assert_eq!(result, "init");
}

#[test]
fn loadlib_nonexistent_file_returns_open_error() {
    let mut lua = rilua::Lua::new().expect("failed to create Lua state");

    let code = r#"
        local f, err, kind = package.loadlib("/nonexistent/path/to/libfoo.so", "bar")
        assert(f == nil, "expected nil function")
        assert(kind == "open", "expected 'open' error, got: " .. tostring(kind))
        RESULT = kind
    "#;

    lua.exec(code).expect("nonexistent file test failed");

    let result: String = lua.global("RESULT").expect("RESULT not set");
    assert_eq!(result, "open");
}

#[test]
fn require_native_module_via_cpath() {
    let lib_path = build_example_module();
    let lib_dir = lib_path.parent().expect("no parent directory");

    let mut lua = rilua::Lua::new().expect("failed to create Lua state");

    // Set package.cpath to point at the built library directory.
    // The module name is "hello", the file is "libhello.so".
    // We need a cpath template that matches: "./lib?.so"
    let cpath = format!("{}/lib?.so", lib_dir.display());
    let setup = format!(
        r#"
        package.cpath = "{cpath}"
        local hello = require("hello")
        assert(type(hello) == "table", "expected table from require")
        assert(hello.VERSION == "0.1.0", "bad VERSION")
        RESULT = hello.greet("require")
        "#
    );

    lua.exec(&setup).expect("require native module failed");

    let result: String = lua.global("RESULT").expect("RESULT not set");
    assert_eq!(result, "Hello, require!");
}

#[test]
fn gc_collects_loaded_library() {
    let lib_path = build_example_module();

    let mut lua = rilua::Lua::new().expect("failed to create Lua state");

    let code = format!(
        r#"
        local f = package.loadlib("{}", "rilua_open_hello")
        local mod = f()
        RESULT = mod.greet("gc")
        "#,
        lib_path.display()
    );

    lua.exec(&code).expect("gc test setup failed");
    let result: String = lua.global("RESULT").expect("RESULT not set");
    assert_eq!(result, "Hello, gc!");

    // Run full GC -- this should finalize the _LOADLIB userdata.
    lua.gc_collect().expect("gc_collect failed");
    // No crash = success (DynLib::drop ran dlclose without issues).
}
