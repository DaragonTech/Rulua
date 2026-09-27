//! rilua — Lua 5.1.1 standalone interpreter.
//!
//! Reproduces the PUC-Rio `lua.c` command-line interface:
//! argument parsing, `LUA_INIT`, `-e`/`-l` processing, script
//! execution with `arg` table, and interactive REPL with multiline
//! detection.

use std::env;
use std::io::{self, BufRead, IsTerminal, Write};
use std::process;

use rilua::{Function, Lua, LuaApiMut, LuaError, StdLib, Val};

// ---------------------------------------------------------------------------
// SIGINT handling — raw FFI, no libc crate
// ---------------------------------------------------------------------------

#[cfg(unix)]
mod sigint {
    use rilua::{clear_interrupted, set_interrupted};

    /// POSIX-mandated value for SIGINT (constant across all Unix platforms).
    const SIGINT: i32 = 2;

    // `Option<extern "C" fn(i32)>` uses null-pointer optimization:
    // `None` is address 0, which is `SIG_DFL` in POSIX.
    #[expect(unsafe_code, reason = "raw POSIX signal FFI")]
    unsafe extern "C" {
        fn signal(signum: i32, handler: Option<extern "C" fn(i32)>) -> Option<extern "C" fn(i32)>;
    }

    /// Signal handler: reset to SIG_DFL first (second Ctrl+C kills), then
    /// set the interrupt flag for the VM to pick up.
    extern "C" fn handler(_sig: i32) {
        // SAFETY: signal() with SIG_DFL (None) is async-signal-safe.
        #[expect(unsafe_code, reason = "reset signal handler in signal context")]
        unsafe {
            signal(SIGINT, None);
        }
        set_interrupted();
    }

    /// Installs a SIGINT handler, runs `f`, then restores `SIG_DFL`.
    pub(crate) fn with_sigint<F, T>(f: F) -> T
    where
        F: FnOnce() -> T,
    {
        clear_interrupted();
        // SAFETY: signal() is async-signal-safe when setting a simple handler.
        #[expect(unsafe_code, reason = "POSIX signal handling requires unsafe")]
        unsafe {
            signal(SIGINT, Some(handler));
        }
        let result = f();
        #[expect(unsafe_code, reason = "POSIX signal handling requires unsafe")]
        unsafe {
            signal(SIGINT, None);
        }
        result
    }
}

#[cfg(windows)]
mod sigint {
    use rilua::{clear_interrupted, set_interrupted};

    type BOOL = i32;
    type DWORD = u32;

    const TRUE: BOOL = 1;
    const FALSE: BOOL = 0;
    const CTRL_C_EVENT: DWORD = 0;

    #[expect(unsafe_code, reason = "Win32 console control handler FFI")]
    unsafe extern "system" {
        fn SetConsoleCtrlHandler(
            handler: Option<extern "system" fn(DWORD) -> BOOL>,
            add: BOOL,
        ) -> BOOL;
    }

    extern "system" fn handler(ctrl_type: DWORD) -> BOOL {
        if ctrl_type == CTRL_C_EVENT {
            set_interrupted();
            TRUE
        } else {
            FALSE
        }
    }

    /// Installs a console Ctrl+C handler, runs `f`, then removes it.
    pub(crate) fn with_sigint<F, T>(f: F) -> T
    where
        F: FnOnce() -> T,
    {
        clear_interrupted();
        #[expect(unsafe_code, reason = "Win32 SetConsoleCtrlHandler requires unsafe")]
        unsafe {
            SetConsoleCtrlHandler(Some(handler), TRUE);
        }
        let result = f();
        #[expect(unsafe_code, reason = "Win32 SetConsoleCtrlHandler requires unsafe")]
        unsafe {
            SetConsoleCtrlHandler(Some(handler), FALSE);
        }
        result
    }
}

#[cfg(not(any(unix, windows)))]
mod sigint {
    /// No-op on platforms without signal support (e.g. WASM).
    pub(crate) fn with_sigint<F, T>(f: F) -> T
    where
        F: FnOnce() -> T,
    {
        f()
    }
}

use sigint::with_sigint;

// ---------------------------------------------------------------------------
// Version string (matches PUC-Rio LUA_RELEASE + LUA_COPYRIGHT)
// ---------------------------------------------------------------------------

const LUA_VERSION: &str = "Lua 5.1.1  Copyright (C) 1994-2006 Lua.org, PUC-Rio";

// ---------------------------------------------------------------------------
// Error reporting
// ---------------------------------------------------------------------------

/// Prints an error message to stderr with optional progname prefix.
///
/// Matches PUC-Rio's `l_message()`: if `progname` is `Some`, prefixes
/// with `"progname: "`.
fn l_message(progname: Option<&str>, msg: &str) {
    if let Some(name) = progname {
        eprint!("{name}: ");
    }
    eprintln!("{msg}");
}

/// Reports a `LuaError` to stderr. Returns `true` if an error was reported.
fn report(progname: Option<&str>, err: &LuaError) -> bool {
    let msg = err.to_string();
    if msg.is_empty() {
        return false;
    }
    l_message(progname, &msg);
    true
}

// ---------------------------------------------------------------------------
// Incomplete chunk detection
// ---------------------------------------------------------------------------

/// A syntax error indicates an incomplete chunk when the message ends
/// with `<eof>`. This matches PUC-Rio's check for `'<eof>'` at the end
/// of the error string.
fn is_incomplete(err: &LuaError) -> bool {
    if let LuaError::Syntax(e) = err {
        e.message.ends_with("'<eof>'")
    } else {
        false
    }
}

// ---------------------------------------------------------------------------
// Argument parsing (matches PUC-Rio collectargs)
// ---------------------------------------------------------------------------

/// Flags parsed from command-line arguments.
struct Flags {
    has_i: bool,
    has_v: bool,
    has_e: bool,
    /// Index of the script argument in argv, or 0 if no script.
    script: usize,
}

/// Parses command-line arguments matching PUC-Rio's `collectargs()`.
///
/// Returns `Ok(Flags)` on success or `Err(())` on invalid arguments.
fn collect_args(argv: &[String]) -> Result<Flags, ()> {
    let mut has_i = false;
    let mut has_v = false;
    let mut has_e = false;
    let mut i = 1;

    while i < argv.len() {
        let arg = &argv[i];
        if !arg.starts_with('-') {
            // Not an option: this is the script.
            return Ok(Flags {
                has_i,
                has_v,
                has_e,
                script: i,
            });
        }

        let bytes = arg.as_bytes();
        if bytes.len() < 2 {
            // Bare "-": execute stdin.
            return Ok(Flags {
                has_i,
                has_v,
                has_e,
                script: i,
            });
        }

        match bytes[1] {
            b'-' => {
                // "--" must be exactly 2 chars.
                if bytes.len() != 2 {
                    return Err(());
                }
                // Next arg (if any) is the script.
                let script = if i + 1 < argv.len() { i + 1 } else { 0 };
                return Ok(Flags {
                    has_i,
                    has_v,
                    has_e,
                    script,
                });
            }
            b'i' => {
                // Must be exactly "-i".
                if bytes.len() != 2 {
                    return Err(());
                }
                has_i = true;
                // -i implies -v (PUC-Rio fallthrough).
                has_v = true;
            }
            b'v' => {
                // Must be exactly "-v".
                if bytes.len() != 2 {
                    return Err(());
                }
                has_v = true;
            }
            b'e' => {
                has_e = true;
                // -e accepts suffix form (-efoo) or next-arg form (-e foo).
                if bytes.len() == 2 {
                    i += 1;
                    if i >= argv.len() {
                        return Err(());
                    }
                }
            }
            b'l' => {
                // -l accepts suffix form (-lname) or next-arg form (-l name).
                if bytes.len() == 2 {
                    i += 1;
                    if i >= argv.len() {
                        return Err(());
                    }
                }
            }
            _ => return Err(()),
        }

        i += 1;
    }

    // No script found.
    Ok(Flags {
        has_i,
        has_v,
        has_e,
        script: 0,
    })
}

// ---------------------------------------------------------------------------
// Usage
// ---------------------------------------------------------------------------

fn print_usage(progname: &str) {
    eprintln!("usage: {progname} [options] [script [args]].");
    eprintln!("Available options are:");
    eprintln!("  -e stat  execute string 'stat'");
    eprintln!("  -l name  require library 'name'");
    eprintln!("  -i       enter interactive mode after executing 'script'");
    eprintln!("  -v       show version information");
    eprintln!("  --       stop handling options");
    eprintln!("  -        execute stdin and stop handling options");
}

// ---------------------------------------------------------------------------
// LUA_INIT handling
// ---------------------------------------------------------------------------

fn handle_lua_init(lua: &mut Lua) -> Result<(), ()> {
    let Ok(init) = env::var("LUA_INIT") else {
        return Ok(());
    };

    let result = if let Some(path) = init.strip_prefix('@') {
        lua.exec_file(path)
    } else {
        lua.exec_bytes(init.as_bytes(), "=LUA_INIT")
    };

    match result {
        Ok(()) => Ok(()),
        Err(e) => {
            report(None, &e);
            Err(())
        }
    }
}

// ---------------------------------------------------------------------------
// Run -e and -l options (matches PUC-Rio runargs)
// ---------------------------------------------------------------------------

fn run_args(lua: &mut Lua, argv: &[String], script_idx: usize, progname: Option<&str>) -> bool {
    let limit = if script_idx > 0 {
        script_idx
    } else {
        argv.len()
    };
    let mut i = 1;

    while i < limit {
        let arg = &argv[i];
        if !arg.starts_with('-') {
            break;
        }
        let bytes = arg.as_bytes();
        if bytes.len() < 2 {
            break;
        }

        match bytes[1] {
            b'e' => {
                let chunk = if bytes.len() > 2 {
                    // Suffix form: -ecode
                    &arg[2..]
                } else {
                    i += 1;
                    &argv[i]
                };
                match lua.load_bytes(chunk.as_bytes(), "=(command line)") {
                    Ok(func) => {
                        if let Err(e) = lua.call_function_traced(&func, &[]) {
                            report(progname, &e);
                            return true;
                        }
                    }
                    Err(e) => {
                        report(progname, &e);
                        return true;
                    }
                }
            }
            b'l' => {
                let lib_name = if bytes.len() > 2 {
                    &arg[2..]
                } else {
                    i += 1;
                    &argv[i]
                };
                if do_library(lua, lib_name, progname).is_err() {
                    return true;
                }
            }
            _ => {}
        }

        i += 1;
    }

    false
}

/// Loads a library via `require(name)`. Matches PUC-Rio's `dolibrary()`.
fn do_library(lua: &mut Lua, name: &str, progname: Option<&str>) -> Result<(), ()> {
    let Ok(require_fn) = lua.global::<Function>("require") else {
        l_message(progname, "require not available");
        return Err(());
    };
    let name_val = lua.create_string(name.as_bytes());
    match lua.call_function(&require_fn, &[name_val]) {
        Ok(_) => Ok(()),
        Err(e) => {
            report(progname, &e);
            Err(())
        }
    }
}

// ---------------------------------------------------------------------------
// arg table construction (matches PUC-Rio getargs)
// ---------------------------------------------------------------------------

/// Builds the `arg` table and returns the script arguments as a Vec
/// for passing to the loaded script function.
///
/// Given `argv = [rilua, -e, code, -l, lib, script.lua, arg1, arg2]`
/// and `script_idx = 5`:
/// ```text
/// arg[-5] = "rilua"
/// arg[-4] = "-e"
/// arg[-3] = "code"
/// arg[-2] = "-l"
/// arg[-1] = "lib"
/// arg[0]  = "script.lua"
/// arg[1]  = "arg1"
/// arg[2]  = "arg2"
/// ```
fn build_arg_table(lua: &mut Lua, argv: &[String], script_idx: usize) -> Vec<Val> {
    let arg_table = lua.create_table();

    // Fill all entries: arg[i - script_idx] = argv[i].
    for (i, a) in argv.iter().enumerate() {
        let key = Val::Num((i as f64) - (script_idx as f64));
        let value = lua.create_string(a.as_bytes());
        // Ignore errors here (table_raw_set cannot really fail for valid keys).
        let _ = lua.table_raw_set(&arg_table, key, value);
    }

    // Set as global "arg".
    let _ = lua.set_global("arg", Val::Table(arg_table.gc_ref()));

    // Collect script arguments (argv[script_idx+1..]) as Val for the function call.
    let mut script_args = Vec::new();
    for a in argv.iter().skip(script_idx + 1) {
        script_args.push(lua.create_string(a.as_bytes()));
    }

    script_args
}

// ---------------------------------------------------------------------------
// Script execution (matches PUC-Rio handle_script)
// ---------------------------------------------------------------------------

fn handle_script(
    lua: &mut Lua,
    argv: &[String],
    script_idx: usize,
    progname: Option<&str>,
) -> bool {
    let script_args = build_arg_table(lua, argv, script_idx);

    let fname = &argv[script_idx];

    // "-" means stdin, unless preceded by "--".
    let load_result = if fname == "-" && (script_idx == 0 || argv[script_idx - 1] != "--") {
        lua.load_file(None)
    } else {
        lua.load_file(Some(fname))
    };

    let func = match load_result {
        Ok(f) => f,
        Err(e) => {
            report(progname, &e);
            return true;
        }
    };

    with_sigint(|| match lua.call_function_traced(&func, &script_args) {
        Ok(_) => false,
        Err(e) => {
            report(progname, &e);
            true
        }
    })
}

// ---------------------------------------------------------------------------
// REPL (matches PUC-Rio dotty + loadline)
// ---------------------------------------------------------------------------

fn dotty(lua: &mut Lua) {
    let stdin = io::stdin();

    loop {
        // Get prompt from _PROMPT global, default "> ".
        // PUC-Rio (non-readline): fputs(p, stdout), fflush(stdout).
        let prompt = get_prompt(lua, true);
        print!("{prompt}");
        let _ = io::stdout().flush();

        // Read first line.
        let Some(mut input) = read_line(&stdin) else {
            break;
        };

        // "= expr" shorthand: prepend "return ".
        if input.starts_with('=') {
            input = format!("return {}", &input[1..]);
        }

        // Try to load, with multiline continuation for incomplete chunks.
        let func = loop {
            match lua.load_bytes(input.as_bytes(), "=stdin") {
                Ok(f) => break Some(f),
                Err(e) => {
                    if is_incomplete(&e) {
                        // Print continuation prompt and read more.
                        let prompt2 = get_prompt(lua, false);
                        print!("{prompt2}");
                        let _ = io::stdout().flush();

                        match read_line(&stdin) {
                            Some(line) => {
                                input.push('\n');
                                input.push_str(&line);
                            }
                            None => break None,
                        }
                    } else {
                        // Real syntax error.
                        report(None, &e);
                        break None;
                    }
                }
            }
        };

        // Execute the loaded chunk.
        if let Some(func) = func {
            with_sigint(|| match lua.call_function_traced(&func, &[]) {
                Ok(results) => {
                    if !results.is_empty() {
                        // Print results via print().
                        print_results(lua, &results);
                    }
                }
                Err(e) => {
                    report(None, &e);
                }
            });
        }
    }

    // Final newline (matches PUC-Rio).
    println!();
}

/// Gets the prompt string from `_PROMPT` or `_PROMPT2` global.
fn get_prompt(lua: &mut Lua, first_line: bool) -> String {
    let global_name = if first_line { "_PROMPT" } else { "_PROMPT2" };
    let default = if first_line { "> " } else { ">> " };

    match lua.global::<Option<String>>(global_name) {
        Ok(Some(s)) => s,
        _ => default.to_string(),
    }
}

/// Reads a line from stdin, stripping the trailing newline.
fn read_line(stdin: &io::Stdin) -> Option<String> {
    let mut line = String::new();
    let result = stdin.lock().read_line(&mut line);
    match result {
        Ok(0) | Err(_) => None,
        Ok(_) => {
            // Strip trailing newline.
            if line.ends_with('\n') {
                line.pop();
                if line.ends_with('\r') {
                    line.pop();
                }
            }
            Some(line)
        }
    }
}

/// Calls `print(...)` with the given results.
fn print_results(lua: &mut Lua, results: &[Val]) {
    let Ok(print_fn) = lua.global::<Function>("print") else {
        return;
    };
    if let Err(e) = lua.call_function(&print_fn, results) {
        let msg = format!("error calling 'print' ({e})");
        l_message(None, &msg);
    }
}

// ---------------------------------------------------------------------------
// Main (matches PUC-Rio pmain)
// ---------------------------------------------------------------------------

fn main() {
    let argv: Vec<String> = env::args().collect();
    let progname = argv.first().map(String::as_str);

    // Create state. If RILUA_TEST_LIB=1 is set, include the internal
    // test library (T global) for PUC-Rio test suite compatibility.
    let libs = if env::var("RILUA_TEST_LIB").as_deref() == Ok("1") {
        StdLib::ALL | StdLib::TEST
    } else {
        StdLib::ALL
    };
    let Ok(mut lua) = Lua::new_with(libs) else {
        l_message(progname, "cannot create state");
        process::exit(1);
    };

    // Handle LUA_INIT.
    if handle_lua_init(&mut lua).is_err() {
        process::exit(1);
    }

    // Parse arguments.
    let Ok(flags) = collect_args(&argv) else {
        print_usage(progname.unwrap_or("lua"));
        process::exit(1);
    };

    // Print version if requested.
    // PUC-Rio uses l_message(NULL, ...) which goes to stderr.
    if flags.has_v {
        eprintln!("{LUA_VERSION}");
    }

    // Run -e and -l options.
    if run_args(&mut lua, &argv, flags.script, progname) {
        process::exit(1);
    }

    // Execute script if present.
    if flags.script > 0 && handle_script(&mut lua, &argv, flags.script, progname) {
        process::exit(1);
    }

    // Interactive mode or stdin.
    if flags.has_i {
        dotty(&mut lua);
    } else if flags.script == 0 && !flags.has_e && !flags.has_v {
        if io::stdin().is_terminal() {
            eprintln!("{LUA_VERSION}");
            dotty(&mut lua);
        } else {
            // Execute stdin as a file.
            match lua.load_file(None) {
                Ok(func) => {
                    if let Err(e) = with_sigint(|| lua.call_function_traced(&func, &[])) {
                        report(progname, &e);
                        process::exit(1);
                    }
                }
                Err(e) => {
                    report(progname, &e);
                    process::exit(1);
                }
            }
        }
    }
}
