# Reference Implementations

Classification of implementations studied during architecture design.
Each was examined for its approach to compilation, VM design, memory
management, and API surface.

## Tier 1 — Primary References

### PUC-Rio Lua 5.1.1 (C)

- **Source**: `./lua-5.1.1/` (vendored in repo; see `AGENTS.md` for setup)
- **Role**: The specification. All behavioral questions are answered here.
- **Architecture**: 17,654 lines. Single-pass recursive descent compiler
  emitting 38 register-based opcodes (u32 packed). CallInfo chain for
  call stack. Incremental tri-color mark-sweep GC with write barriers.
  Array+hash tables. Interned strings with cached hash. longjmp/setjmp
  error handling. Stack-based C API.
- **What we take**: Opcode semantics, GC behavioral contract, weak table
  semantics, finalizer protocol, `collectgarbage()` API, standard library
  behavior, error messages.
- **What we change**: Single-pass compilation (we use AST), longjmp (we
  use Result), C API (we use traits), raw pointers (we use arenas).

### Luau (C++)

- **GitHub**: <https://github.com/luau-lang/luau>
- **Role**: Architecture reference. Lua 5.1-compatible scripting language.
- **Architecture**: Lexer -> Parser -> AST -> Compiler -> VM. 83
  register-based opcodes (count evolves with development). Incremental tri-color GC. CallInfo chain.
  Array+hash tables. String interning. No longjmp (C++ exceptions).
- **What we take**: AST-based pipeline design, separation of compiler
  phases, Proto as non-GC value (owned by closures), CallInfo chain
  pattern, incremental GC debt model.
- **What we change**: Opcode count (we use PUC-Rio's 38, not Luau's 83),
  language extensions (we implement standard Lua 5.1.1 only), native
  codegen (out of scope).

## Tier 2 — Selective References

### tsuki (Rust, Lua 5.4)

- **GitHub**: <https://github.com/nickmass/tsuki>
- **Role**: Proves Result-based error handling works for a Lua VM.
- **Architecture**: c2rust translation of Lua 5.4. PUC-Rio file naming
  (llex.rs, lparser.rs, lcode.rs). 83 opcodes. Incremental GC via raw
  pointers. Heavy use of unsafe code. Memory-safe public API over unsafe internals.
- **What we take**: Result-based error propagation pattern, memory-safe
  public API design, packed u32 instruction format for
  serialization.
- **What we avoid**: c2rust code style, heavy unsafe, wrong Lua version
  (5.4 vs 5.1.1).

### full-moon (Rust, parser only)

- **GitHub**: <https://github.com/Kampfkarren/full-moon>
- **Role**: Best Rust patterns for Lua parsing.
- **Architecture**: Hand-written recursive descent parser. Lossless AST
  preserving whitespace and comments. Error recovery with partial AST.
  Multi-dialect support (5.1-5.4, Luau) via feature flags. Sealed
  traits, visitor pattern, builder pattern.
- **What we take**: Recursive descent parsing patterns, AST node design
  idioms, error recovery approach, sealed trait pattern.
- **What we avoid**: Lossless parsing (unnecessary for a VM), multi-dialect
  support (we only target 5.1.1), external dependencies.

### mlua (Rust, FFI wrapper)

- **GitHub**: <https://github.com/mlua-rs/mlua>
- **Role**: API design reference for embedding Lua in Rust.
- **Architecture**: FFI wrapper around C Lua. Memory-safe Rust API over
  unsafe C bindings. Trait-based type conversions (`IntoLua`, `FromLua`).
  Registry-based lifetime management. Scoped values. UserData trait.
- **What we take**: Trait-based API design (`IntoLua`/`FromLua` pattern),
  UserData trait approach, scope-based lifetime management, error type
  design.
- **Not applicable**: FFI wrapping (we implement from scratch), C Lua
  compilation, vendored builds.

### lua-rs / CppCXY (Rust, Lua 5.5)

- **GitHub**: <https://github.com/CppCXY/lua-rs>
- **Role**: Architecture comparison. Full Lua 5.5 port to Rust with
  pointer-based VM design.
- **Architecture**: 64k lines across workspace (luars, luars-derive,
  luars_interpreter, luars_wasm). Faithful port of C Lua's architecture:
  pointer-based VM dispatch, 86 Lua 5.5 opcodes, tri-color incremental +
  generational GC, 16-byte TValue (union + type tag), Brent's collision
  tables, interned strings, pointer-based upvalues. 399 unsafe blocks.
  Multi-crate workspace with proc-macro derive crate. 28/30 official Lua
  5.5 tests pass. External dependencies: ahash, rand, chrono, itoa,
  smol_str, syn/quote (macros).
- **What we can study**: Async/await coroutine bridging (novel feature),
  proc-macro derive for UserData, generational GC implementation,
  Lua 5.5 opcode set, lightweight 1-byte error enum with message stored
  in VM, platform abstraction layer design, WASM support approach.
- **Key differences from rilua**: Targets Lua 5.5 (not 5.1.1), uses raw
  pointers and union types (C-style, 399 unsafe blocks vs rilua's
  arena-based zero-unsafe GC), has external dependencies (ahash, rand,
  chrono, smol_str vs rilua's zero-dependency policy), pointer-based
  upvalues (vs rilua's arena indices), larger scope (64k LOC vs rilua's
  ~17k), proc-macro crate for ergonomic API.

## Tier 3 — Limited Relevance

### lua-in-rust (Rust, Lua 5.1.1)

- **GitHub**: <https://github.com/cjneidhart/lua-in-rust>
- **Role**: Previous base for rilua. Studied for lessons learned.
- **Architecture**: Single-pass compiler, stack-based VM (~40 custom
  opcodes), stop-the-world mark-sweep GC with raw pointers, string
  interning, HashMap-only tables. ~5,000 lines. ~40% complete.
- **Lessons**: Stack-based VM diverges too far from PUC-Rio's
  register-based design. Single-pass compilation makes the compiler
  hard to test and extend. Raw pointer GC works but is fragile.
  String interning with pointer equality is the right approach.

### lua-rs / lonng (Rust, compiler only)

- **GitHub**: <https://github.com/lonng/lua-rs>
- **Role**: Shows AST-to-bytecode compilation for Lua 5.1.
- **Architecture**: Multi-pass with explicit AST. Scanner -> Parser ->
  AST -> Compiler -> FunctionProto. No VM. Broken build. Zero unsafe.
- **Useful for**: AST node design, constant folding patterns, debug
  info tracking. Not useful for VM or runtime design.

### lua-rs (Rust, full interpreter, CppCXY)

- **Local path**: `~/Repos/github.com/CppCXY/lua-rs`
- **GitHub**: <https://github.com/CppCXY/lua-rs>
- **Role**: Performance comparison target. One-to-one C Lua 5.5 port.
- **Architecture**: Direct port of PUC-Rio's C source to Rust. Same
  pipeline (Lexer -> Parser -> Code Generator -> Bytecode -> VM), 86
  opcodes, tri-color incremental/generational GC, string interning.
  ~60K lines, ~400 unsafe blocks for GC pointer ops. 5 runtime deps
  (ahash, rand, chrono, itoa, smol_str). Requires nightly Rust.
- **Useful for**: Benchmarking (runs ~1.2x PUC-Rio on micro-benchmarks),
  comparing pure-Rust Lua implementation approaches and the
  safety-vs-performance trade-off.
- **Differences**: Targets Lua 5.5 (not 5.1.1), uses `unsafe`
  extensively, requires nightly. See `docs/src/comparison.md`.

### coppermoon (Rust, mlua wrapper)

- **GitHub**: <https://github.com/coppermoondev/coppermoon>
- **Role**: None for VM/compiler work.
- **Architecture**: Node.js-like runtime wrapping mlua (C Lua 5.4 FFI).
  Not a from-scratch implementation. Provides batteries-included stdlib
  (HTTP, SQLite, WebSocket, etc.) over the C VM.
- **Not applicable**: Everything. This wraps C Lua, not implements it.

### hematita (Rust, Lua 5.3)

- **GitHub**: <https://github.com/danii/hematita>
- **Role**: Reference for hardened Lua interpreter patterns in Rust.
- **Architecture**: From-scratch Lua interpreter in Rust targeting 5.3.
  Focuses on safety and correctness. Useful for comparing implementation
  approaches for parsing, value representation, and standard library.
- **What we take**: Comparison point for implementation patterns and
  edge case handling.
- **Differences**: Targets Lua 5.3 (not 5.1.1), different design goals.
