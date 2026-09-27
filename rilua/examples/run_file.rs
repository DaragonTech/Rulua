//! Runs a Lua file passed as a command-line argument.
//!
//! Usage:
//!     cargo run --example run_file -- examples/hello.lua

use std::process;

fn main() {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: run_file <script.lua>");
        process::exit(1);
    };

    let Ok(mut lua) = rilua::Lua::new() else {
        eprintln!("failed to create Lua state");
        process::exit(1);
    };

    if let Err(e) = lua.exec_file(&path) {
        eprintln!("{e}");
        process::exit(1);
    }
}
