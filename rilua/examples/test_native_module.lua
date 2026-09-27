-- Test script for the rilua native module example.
--
-- Prerequisites:
--   cargo build --manifest-path examples/native_module/Cargo.toml
--
-- Run with:
--   LUA_CPATH="examples/native_module/target/debug/lib?.so" \
--     cargo run --features dynmod -- examples/test_native_module.lua

local hello = require("hello")
print("Module type:", type(hello))
print("Version:", hello.VERSION)
print(hello.greet("world"))
print(hello.greet("rilua"))
print(hello.greet())
print("Native module loading works!")
