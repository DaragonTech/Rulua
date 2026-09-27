//! riluac — Lua 5.1.1 bytecode compiler/lister.
//!
//! Matches PUC-Rio's `luac` command-line interface:
//! - `-l` list bytecode (use twice for full listing)
//! - `-o name` output to file (default "luac.out")
//! - `-p` parse only (syntax check)
//! - `-s` strip debug information
//! - `-v` show version

use std::env;
use std::io::Read;
use std::process;

use rilua::compiler;
use rilua::vm::listing;
use rilua::vm::proto::{Proto, ProtoRef};

/// Version string matching PUC-Rio format.
const VERSION: &str = "Lua 5.1.1  Copyright (C) 1994-2006 Lua.org, PUC-Rio";

/// Program name for error messages.
const PROGNAME: &str = "riluac";

fn fatal(message: &str) -> ! {
    eprintln!("{PROGNAME}: {message}");
    process::exit(1);
}

fn usage(message: &str) -> ! {
    if !message.is_empty() {
        eprintln!("{PROGNAME}: {message}");
    }
    eprintln!(
        "usage: {PROGNAME} [options] [filenames].\n\
         Available options are:\n  \
         -        process stdin\n  \
         -l       list (use -l -l for full listing)\n  \
         -o name  output to file 'name' (default \"luac.out\")\n  \
         -p       parse only\n  \
         -s       strip debug information\n  \
         -v       show version information\n  \
         --       stop handling options"
    );
    process::exit(1);
}

/// Parsed command-line arguments (matches PUC-Rio's `doargs()`).
struct Args {
    /// Listing level: 0 = none, 1 = summary, 2+ = full.
    listing: u32,
    /// Parse only (syntax check, no output).
    parse_only: bool,
    /// Strip debug information from output.
    strip: bool,
    /// Output file name (default "luac.out").
    output_file: String,
    /// Input files (empty strings mean stdin).
    files: Vec<String>,
}

fn do_args() -> Args {
    let argv: Vec<String> = env::args().collect();
    let argc = argv.len();
    let mut listing: u32 = 0;
    let mut parse_only = false;
    let mut strip = false;
    let mut output_file = "luac.out".to_string();
    let mut files = Vec::new();
    let mut version_seen = false;
    let mut i = 1;

    while i < argc {
        let arg = &argv[i];
        if !arg.starts_with('-') {
            // Not an option — start of file list.
            break;
        }
        match arg.as_str() {
            "--" => {
                i += 1;
                // Stop handling options but check for -v after --.
                if i < argc && argv[i] == "-v" {
                    version_seen = true;
                }
                break;
            }
            "-" => {
                // Process stdin.
                files.push(String::new());
                i += 1;
                break;
            }
            "-l" => listing += 1,
            "-o" => {
                i += 1;
                if i >= argc {
                    usage("'-o' needs argument");
                }
                output_file.clone_from(&argv[i]);
            }
            "-p" => parse_only = true,
            "-s" => strip = true,
            "-v" => version_seen = true,
            other => {
                usage(&format!("unrecognized option '{other}'"));
            }
        }
        i += 1;
    }

    // Collect remaining arguments as input files.
    while i < argc {
        files.push(argv[i].clone());
        i += 1;
    }

    // Print version if requested (always after parsing all args).
    if version_seen {
        println!("{VERSION}");
    }

    // If no files and no listing/parse-only, show usage.
    if files.is_empty() && listing == 0 && !parse_only {
        if version_seen {
            process::exit(0);
        }
        usage("no input files given");
    }

    Args {
        listing,
        parse_only,
        strip,
        output_file,
        files,
    }
}

/// Compile a single source or load a binary chunk, reading from file or stdin.
fn compile_source(filename: &str) -> ProtoRef {
    let (source, name) = if filename.is_empty() {
        // Read from stdin.
        let mut buf = Vec::new();
        if let Err(e) = std::io::stdin().read_to_end(&mut buf) {
            fatal(&format!("cannot read stdin: {e}"));
        }
        (buf, "=stdin".to_string())
    } else {
        let buf = match std::fs::read(filename) {
            Ok(b) => b,
            Err(e) => fatal(&format!("cannot open {filename}: {e}")),
        };
        (buf, format!("@{filename}"))
    };

    // Detect binary chunks (starts with \x1bLua).
    if source.starts_with(rilua::vm::dump::LUA_SIGNATURE) {
        match rilua::vm::undump::undump(&source, &name) {
            Ok(proto) => proto,
            Err(e) => fatal(&format!("{e}")),
        }
    } else {
        match compiler::compile(&source, &name) {
            Ok(proto) => proto,
            Err(e) => fatal(&format!("{e}")),
        }
    }
}

/// Combine multiple Protos into a single wrapper Proto.
///
/// Matches PUC-Rio's `combine()` from `luac.c`: creates a main function
/// that calls CLOSURE+CALL for each input file's Proto.
fn combine(protos: Vec<ProtoRef>) -> ProtoRef {
    use rilua::vm::instructions::{Instruction, OpCode};

    if protos.len() == 1 {
        return protos.into_iter().next().unwrap_or_else(|| unreachable!());
    }

    let mut main = Proto::new(&format!("=({PROGNAME})"));
    main.is_vararg = rilua::vm::proto::VARARG_ISVARARG;
    // Need n*2 instructions (CLOSURE+CALL per file) + 1 RETURN.
    // Registers: 0 for the closure value.
    main.max_stack_size = 1;

    for (i, _) in protos.iter().enumerate() {
        // CLOSURE A Bx: R(0) = closure(KPROTO[i])
        main.code
            .push(Instruction::a_bx(OpCode::Closure, 0, i as u32).raw());
        main.line_info.push(0);

        // CALL A B C: R(0)(no args, no results) => CALL 0 1 1
        main.code
            .push(Instruction::abc(OpCode::Call, 0, 1, 1).raw());
        main.line_info.push(0);
    }

    // RETURN 0 1: return (no values).
    main.code
        .push(Instruction::abc(OpCode::Return, 0, 1, 0).raw());
    main.line_info.push(0);

    main.protos = protos;

    ProtoRef::new(main)
}

fn main() {
    let args = do_args();

    // If no input files and listing/parse-only mode, use "luac.out" as
    // input (PUC-Rio behavior).
    let files = if args.files.is_empty() {
        vec!["luac.out".to_string()]
    } else {
        args.files
    };

    // Compile all input files.
    let protos: Vec<ProtoRef> = files.iter().map(|f| compile_source(f)).collect();

    // Combine into a single Proto (wraps multiple files).
    let proto = combine(protos);

    // Listing mode.
    if args.listing > 0 {
        let full = args.listing > 1;
        let output = listing::list_function(&proto, full);
        print!("{output}");
    }

    // Parse-only mode: nothing else to do (compilation already succeeded).
    if args.parse_only {
        return;
    }

    // Write binary output (unless listing-only with no explicit -o).
    // PUC-Rio always writes output unless -p is given.
    // When -l is used without -o, PUC-Rio still writes luac.out.
    let bytes = rilua::vm::dump::dump(&proto, None, args.strip);
    if let Err(e) = std::fs::write(&args.output_file, &bytes) {
        fatal(&format!("cannot write {}: {e}", args.output_file));
    }
}
