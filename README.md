![Rulua-Icon](./Rulua-icon.png)

# Rulua — a Rust port of Lua 5.1, binary-compatible with lua5.1.dll / .so / .dylib

> **A note from the developer**
>
> Rulua is the first project where I guided an AI to write 100% of the code.
> I didn't type the Rust, the C or the build scripts. My part was to say what
> I needed, run everything on my own machines, send back the logs, and decide
> what came next. The AI wrote the rest, including the tracer that tracked
> down a "bug" that turned out to be a typo in my own command.
>
> Not long ago this wasn't possible. A drop-in replacement for a C library,
> compatible down to the binary interface, on Windows, macOS and Linux, used
> to mean months of careful low-level work. To me this project represents a
> real shift: what matters most now is knowing exactly what you want, and
> checking, honestly, that you got it. The tests below are that check.
>
> — Felipe, DaragonTech

**Rulua** is a port of the **Lua 5.1 shared library** to Rust, for Windows,
macOS and Linux. It is binary-compatible with the original C library from
PUC-Rio: it exports the complete **Lua 5.1 C API** (`lua.h`, `lauxlib.h`,
`lualib.h`) with the same names and calling conventions, so existing
programs and C modules built for `lua5.1.dll`, `liblua5.1.so` or
`liblua5.1.dylib` load it without being recompiled.

Rulua's own code does the porting: the full C API layer (stacks, `lua_State`
handles, C functions and closures, errors crossing C code, coroutines,
userdata, the debug interface and hooks, C-module loading), a `lua51.dll`
compatibility proxy, and the builds for seven platforms. Only the `luaL_*`
helper library (`lauxlib.c`) is kept as PUC-Rio's original C, unchanged. For
the language itself (the virtual machine, compiler, garbage collector and
standard libraries) it builds on [rilua](https://crates.io/crates/rilua), a
Lua 5.1 interpreter written in Rust. rilua is included in this repository
with fixes and optimizations made for Rulua. On Windows they make Rulua
**1.24× faster than upstream rilua 0.1.24** over the whole benchmark
(1.5× per test on average, up to 2× on string code and 300× on
very large strings). Rulua is still **1.5–2.4× slower than the
original C Lua 5.1.4**, depending on the machine
([Performance](#performance)).

| Platform | Library | Status |
|---|---|---|
| Windows x64 | `lua5.1.dll` (+ `lua51.dll` proxy) | in production use with a Rust host and its C modules |
| Windows x86 (Win32) | `lua5.1.dll` (+ `lua51.dll` proxy) | C API test suite passes (Wine), incl. modules without unwind tables and an MSVC-ABI host |
| Windows ARM64 | `lua5.1.dll` (+ `lua51.dll` proxy) | builds and links cleanly; to be verified on ARM64 hardware |
| macOS arm64 (Apple Silicon) | `liblua5.1.dylib` | C API test suite passes on a real MacOS M3 |
| macOS x86_64 (Intel) | `liblua5.1.dylib` | C API test suite and benchmark checksums pass on a real Intel MacBook Air |
| Linux x86_64 | `liblua5.1.so` | C API test suite passes; portable build needs only glibc ≥ 2.28 |
| Linux ARM64 (aarch64) | `liblua5.1.so` | C API test suite and benchmark checksums pass (QEMU); needs only glibc ≥ 2.28 |

Existing C/C++ hosts and C modules compiled against the Lua 5.1 headers link
against it unchanged: the stock `lua.c` interpreter, LPeg, lua-cjson, and
modules loaded through `require` / `package.loadlib` all run on it.

## Repository Layout

| Path | Description |
|---|---|
| `src/global.rs` | `lua_State` handles, thread tracking, error propagation, and the C-function trampoline |
| `src/api.rs` | Implementation of the `lua_*` API defined by `lua.h` |
| `src/debug.rs` | Lua debug API: `lua_getstack`, `lua_getinfo`, locals/upvalues, and C hooks |
| `src/libs.rs` | Standard library entry points (`luaopen_*`), `luaL_openlibs`, and C-module loading for `require` / `package.loadlib` |
| `src/sys.rs` | Platform abstraction layer: `LoadLibrary` / `dlopen`, `OutputDebugString`, and panic handling |
| `csrc/lauxlib.c` | Unmodified Lua 5.1.4 auxiliary library, providing the `luaL_*` API |
| `csrc/ljmp.c` | setjmp/longjmp error transport, used on 32-bit x86 only (see *How it works*) |
| `csrc/lapi_fmt.c` | `lua_pushfstring` / `lua_pushvfstring` implementation; C variadic functions cannot be implemented directly in stable Rust |
| `include/` | Unmodified Lua 5.1.4 headers used when compiling against the library |
| `rilua/` | Vendored rilua 0.1.24 with project-specific patches; see `RILUA_PATCHES.diff` |
| `lua5.1.def`, `lib/` | Windows export definition and prebuilt import libraries: `lua5.1.lib` for MSVC and `liblua5.1.dll.a` for MinGW |
| `lua5.1.map`, `macos-exports.txt` | Export lists for Linux and macOS |
| `proxy/` | Source, prebuilt DLL, and import libraries for the `lua51.dll` forwarding proxy |
| `tools/tracer/` | API-call tracing build of `lua5.1.dll` for debugging host applications |
| `tools/zig/` | `zig cc` wrappers used for the macOS, Windows ARM64 and portable Linux (x86_64 / ARM64) builds |
| `test/` | Differential C API tests, sample C module, stress tests, and API-overhead tests |
| `bench/` | `bench.lua` performance suite and `compare.lua` comparison tool |

## Binary Releases

Prebuilt binaries for Windows, Linux, and macOS are available from the project's **Releases** page:

- **Windows:** `.dll` for x64, x86 (Win32) and ARM64
- **Linux:** `.so` for x86_64 and ARM64
- **macOS:** `.dylib` for Apple Silicon (arm64) and Intel (x86_64)

The binaries expose the complete Lua 5.1 API expected by applications built against the official Lua 5.1 shared library.

### Exports

Each platform exports **123 symbols**:

- all **121 Lua 5.1 API functions**
- `luaL_openlib` for compatibility
- `lua_setlevel`

The exported API is kept identical across the Windows, Linux, and macOS builds.

## Building

Rust ≥ 1.92 on all platforms. Release builds use LTO, a single codegen unit,
mimalloc as the allocator and, on x86_64, `target-cpu=x86-64-v2`
(`.cargo/config.toml`; every x86-64 CPU since ~2009, required by Windows 11).

### Windows, MSVC (recommended)
Rust (`stable-x86_64-pc-windows-msvc`) and Visual Studio Build Tools:

```bat
build-windows-msvc.bat
```
Produces `dist\lua5.1.dll`, `dist\lua5.1.lib`, `dist\include\*.h`.
Link your host with `lua5.1.lib` and compile with `-DLUA_BUILD_AS_DLL`
(as with any Lua DLL).

### Windows, MinGW-w64 (or cross-compiling from Linux)
```sh
rustup target add x86_64-pc-windows-gnu
./build-windows-gnu.sh
```

### Windows x86 (Win32)
MinGW-w64 (`i686-w64-mingw32-gcc`) and `llvm-dlltool`:
```sh
rustup target add i686-pc-windows-gnu
./build-windows-x86.sh      # dist-win32/: DLL, import libraries, lua51.dll proxy, lua.exe
```
C runtime `msvcrt.dll`, like LuaBinaries' MinGW builds. On this target Lua
errors use `setjmp`/`longjmp` (see *How it works*).

### Windows ARM64
Cross-compiled with [zig](https://ziglang.org) (clang + lld, LLVM-MinGW ABI):
```sh
rustup target add aarch64-pc-windows-gnullvm
ZIG=/path/to/zig ./build-windows-arm64.sh   # dist-winarm64/
```
C runtime: the Universal CRT (Windows 10+). An ARM64 DLL is for ARM64
programs and ARM64 C modules; x64 programs running under emulation on
Windows on ARM keep using the x64 DLL.

### `lua51.dll` proxy
Many Windows hosts and C modules link against `lua51.dll` instead of
`lua5.1.dll`. `proxy/build-proxy.sh` (MinGW-w64) builds a small `lua51.dll`
whose exports jump into `lua5.1.dll` (`ARCH=x86` / `ARCH=arm64` for the
other Windows targets; the Win32 and ARM64 scripts above build it); a
prebuilt x64 one is in `proxy/`. Replace
any old proxy with it. Old ones (e.g. from LuaBinaries) link to PUC-Rio
internals such as `luaD_growstack`, which this library doesn't have, and
fail to load with *"procedure entry point … could not be located"*.

The new proxy still *exports* those internals, so modules that import them
load. They are looked up in `lua5.1.dll` at load time; if a module actually
**calls** one that the loaded `lua5.1.dll` lacks, a message box names it and
the process exits (with a PUC-Rio `lua5.1.dll` behind it they simply work).

### macOS (Apple Silicon and Intel)
On a Mac, with the Xcode command line tools:
```sh
./build-macos.sh                # Apple Silicon -> dist-macos/
ARCH=x86_64 ./build-macos.sh    # Intel         -> dist-macos-x86_64/
```
Cross-compiling from Linux needs [zig](https://ziglang.org) (`pip install
ziglang` works) as C compiler and linker, plus LLVM's `ld64.lld` for the
dylib itself (zig's Mach-O linker ignores `-exported_symbols_list`):
```sh
rustup target add aarch64-apple-darwin x86_64-apple-darwin
CROSS=1 ZIG=/path/to/zig ./build-macos.sh
ARCH=x86_64 CROSS=1 ZIG=/path/to/zig ./build-macos.sh
```
The dylib has install name `@rpath/liblua5.1.dylib`, exports exactly the
123 API functions, depends only on `libSystem` and targets macOS 11+; the
Apple Silicon one is ad-hoc signed, the Intel one is built for x86-64-v2
(every Intel Mac that runs macOS 11 has SSE4.2). Downloaded copies are
quarantined by Gatekeeper: `xattr -dr com.apple.quarantine <dir>`.
C modules: build as usual (`-bundle` or `-shared -undefined dynamic_lookup`).

### Linux
For the current machine:
```sh
make          # dist/liblua5.1.so + dist/lua (stock lua.c linked to it)
make test     # differential C API tests
```
For a binary to ship to other machines, link against an old glibc with zig:
```sh
ZIG=/path/to/zig ./build-linux-portable.sh                # x86_64 -> dist-linux/
ARCH=aarch64 ZIG=/path/to/zig ./build-linux-portable.sh   # ARM64  -> dist-linux-arm64/
```
That covers Ubuntu 18.10+, Debian 10+ and RHEL/Rocky 8+, and needs only
libc/libm/libdl/libpthread (the unwinder is linked in statically). If a host
expects Ubuntu's `liblua5.1.so.0`, symlink it to `liblua5.1.so`.

On Linux and macOS the shared library is linked from the Rust staticlib by
the scripts above (not by cargo), because a cargo `cdylib` would not export
the C-implemented `luaL_*` functions on those platforms.

## Debugging a host: the tracer

`tools/tracer/` builds a stand-in `lua5.1.dll` (Windows, MinGW) that logs
every Lua C API call with its arguments, results and stack top, then forwards
the call to `luacore.dll`, which is any real `lua5.1.dll` renamed. Tracing
this library and the original PUC-Rio DLL on the same host and diffing the two
logs shows the first place where they behave differently.

```sh
cd tools/tracer && ./build.sh     # python3 gen_tracer.py ../../include regenerates the sources
```
* Output goes to `OutputDebugString` (Sysinternals DebugView) or to a file
  with `LUATRACE_FILE=C:\temp\trace.log`.
* `LUATRACE_LUA=1` also logs every Lua-level function call (`=2`: every
  line). `LUATRACE_OFF=1` disables tracing.
* Log lines: `#N > lua_call(args) top=T` before a call, `#N < … = result`
  after it. A `>` line without its `<` line means the call raised an error or
  the process died inside it.
* Calls to PUC-Rio internals through the `lua51.dll` proxy are logged as
  `!!! INTERNAL <name> called`.

This library also writes diagnostics of its own on every platform (debugger
output on Windows, stderr elsewhere): `lua5.1 (rilua): UNPROTECTED ERROR …`
before running the `lua_atpanic` handler, and `INTERNAL PANIC` for a bug in
the Rust side.

## How it works (short version)

* **`lua_State*`**: each Lua thread (main thread and coroutines) gets one
  stable handle. rilua keeps the *running* thread's stack in `LuaState` and
  parks others in their thread objects; each API call finds where the
  addressed thread's stack currently lives.
* **C functions**: a `lua_CFunction` becomes a rilua Rust closure whose
  function is a trampoline and whose `native` field holds the C pointer. C
  closures keep their upvalues and environment (`lua_upvalueindex`,
  `LUA_ENVIRONINDEX` work as usual).
* **Errors**: `lua_error` / `luaL_error` never return, as in PUC-Rio, but
  instead of `longjmp` the error is a Rust unwind that travels through the C
  frames (`extern "C-unwind"`) to the nearest trampoline, `lua_pcall` or
  `lua_cpcall`. With no protection active, the `lua_atpanic` function runs
  and the process exits, as in PUC-Rio. This relies on unwind tables, which
  every function has on x64 and ARM64 (and that GCC/Clang emit on Linux and
  macOS). 32-bit x86 has no such guarantee (MSVC- or Delphi-built modules
  have no DWARF tables), so there the library does what PUC-Rio's `ldo.c`
  does: every call into C code goes through a small C helper that sets a
  `setjmp` target (`csrc/ljmp.c`), and `lua_error` stores the error and
  `longjmp`s to the innermost one.
* **Coroutines**: `lua_newthread`, `lua_resume`, `lua_yield` (including
  `return lua_yield(L, n)` from C functions), `lua_xmove` and `lua_status`
  are supported. As in PUC-Rio, a thread that finished normally can be
  reused by pushing a new function and resuming again.
* **Strings**: rilua strings are now stored NUL-terminated, so
  `lua_tolstring` returns a stable pointer into the string itself (valid
  while the value is reachable, as in PUC-Rio).
* **Userdata**: `lua_newuserdata` returns 16-byte-aligned memory owned by
  the GC; `__gc`, `lua_setfenv`/`lua_getfenv`, `luaL_checkudata` etc. work.
  Finalizers run on collection and on `lua_close`.
* **Output**: `print` and `io.write` go through the C runtime's `stdout` and
  are flushed on `lua_close`, so output is not lost when a host or a module
  ends the process directly (e.g. with `ExitProcess`).

## Verification

* **Differential C API tests** (`test/capi_test*.c`, ~250 lines of checked
  output covering stack ops, conversions, metatables, userdata/`__gc`, C
  closures and upvalues, error paths, `pcall` handlers, `cpcall`, load/dump,
  buffers, refs, environments, coroutines with C yields, debug API, hooks,
  GC, `require` of a C module, the panic path). Each is compiled against real
  PUC-Rio 5.1.4 and against this library, and the outputs diffed. The only
  differences left are a printed table address and one message worded per
  Lua 5.1.1 (see below). Same result on:
  * Linux x86_64 (native and the glibc 2.28 portable build),
  * Linux ARM64 (glibc 2.28 portable build, run under QEMU user-mode
    emulation), including the benchmark checksums,
  * macOS arm64 on real hardware (Mac Studio),
  * macOS x86_64 on real hardware (Intel MacBook Air), including the
    benchmark checksums,
  * Windows x64 via Wine, including an **MSVC-ABI host** (clang
    `x86_64-pc-windows-msvc` + `lld-link`, linked through `lua5.1.lib`), with
    errors unwinding through its C frames,
  * Windows x86 via Wine, with the test programs and the sample module built
    **without unwind tables**, plus a 32-bit MSVC-ABI host; a stress test
    raising 1,000,000 errors from such C code (directly, through nested
    `lua_call`, inside coroutines) keeps memory flat.
* On real Windows it replaces the original `lua5.1.dll` + `lua51.dll` of a
  Rust application with several C modules. Traces of the application on
  this library and on the original DLL were compared call by call.
* Stock `lua.c` runs the Lua 5.1.4 `test/*.lua` programs with output
  identical to PUC-Rio.
* Real C modules: **LPeg 1.0.2** full test suite passes; **lua-cjson**
  gives identical results to PUC-Rio.
* `bench/bench.lua`: all 26 checksums identical to PUC-Rio 5.1.4.
* String library fuzzing (60,000 random `find`/`match`/`gsub`/`sub`/`byte`
  calls) gives identical results to PUC-Rio.
* rilua's own test suite (1,050 tests) still passes after the patches.
* Valgrind: no invalid accesses and no leaks. A 200k-object stress test
  keeps memory flat and runs every finalizer.

## Performance

`bench/bench.lua` runs 26 workloads and prints a checksum for each, so both
implementations are checked for identical results as well as speed:

```sh
lua-puc bench.lua > puc.txt       # the original interpreter / DLL
lua     bench.lua > rilua.txt     # this library
lua compare.lua puc.txt rilua.txt
```
(`lua bench.lua 0.2` for a quick run, `lua bench.lua 1 string` to filter.)

Results on real Windows 11 ARM64, drop-in replacement for the original
`lua5.1.dll`:

| | Original PUC-Rio DLL | Rulua |
|---|---|---|
| Whole suite | 11.1 s | 16.4 s (**1.47×**) |
| String library (`find`, `gsub`, `gmatch`, `concat`) | | about the same speed |
| Plain VM code (loops, calls, tables) | | 1.5–2.5× slower |
| Coroutine switches | | ~7× slower |
| Allocation-heavy code (`gc_churn`) | | ~2.4× slower |

Coroutines and allocation are the architectural costs of rilua's design
(stacks are swapped on every resume/yield; the GC is an arena with generation
checks); the rest is its interpreter loop. On other machines the whole suite
runs 2.0× slower than the original on an Intel Mac, and 2.4× on Linux x86_64.

### Compared with upstream rilua

The same suite run with the official, unmodified `rilua` 0.1.24 interpreter
(`rilua.exe` from its Windows release, and the same version built from
crates.io on Linux), against Rulua's `lua` and the original PUC-Rio 5.1.4
(best of two runs; every checksum is the same for all three):

| | Windows x64 (Wine) | Linux x86_64 |
|---|---|---|
| Whole suite: upstream rilua / Rulua / PUC-Rio | 48.0 s / 18.2 s / 9.9 s | 42.8 s / 18.5 s / 7.6 s |
| … without `string_large` | 22.5 s / 18.1 s / 9.9 s | 18.3 s / 18.4 s / 7.6 s |
| Rulua vs upstream rilua, typical test (geometric mean) | **1.53× faster** | 1.29× faster |
| String library (`find`, `gsub`, `gmatch`, `sub`, `byte`) | 1.6–1.9× faster | 1.1–1.35× faster |
| `string_large` (walking a 1 MB string) | 25.5 s → 0.08 s (**327×**) | 24.5 s → 0.08 s (**314×**) |

Most of the gains come from Rulua's patches to rilua. String arguments are no
longer copied on each call. Character classes and case conversion no longer
make one C runtime call per character, which is costly with Windows'
`msvcrt.dll`. mimalloc replaces the system allocator. On Linux, with a faster
C runtime and allocator, ordinary code runs at about the same speed as
upstream rilua, and `%` is about 10% slower because it now computes exactly
like PUC-Rio.

## Differences from PUC-Rio you may notice

* **Language/stdlib behavior is rilua's**, which targets Lua **5.1.1**
  (World of Warcraft flavour). A few library messages are worded differently
  from 5.1.4, e.g. `cannot resume non-suspended coroutine`, `bad argument #1
  to 'setmetatable' (table expected)`, and `string.gsub` does not reject some
  malformed patterns.
* `lua_newstate(f, ud)`: the allocator is stored and returned by
  `lua_getallocf`, but rilua manages its own memory, so `f` is not called
  for Lua objects. `lua_gc(LUA_GCCOUNT)` reports rilua's estimate.
* `lua_topointer` returns unique, stable identifiers for tables, functions
  and threads (not real addresses), so `tostring({})` prints e.g.
  `table: 0x00000017`; for userdata it returns the block pointer.
* PUC-Rio **internal** functions (`luaD_*`, `luaH_*`, …) are not provided.
  Only modules that bypass the public API use them (see the proxy section).
* On x64 and ARM64, Lua errors travel as Rust unwinds, not `longjmp` (on
  32-bit x86 they use `longjmp`, as in PUC-Rio). C++ code between a
  `lua_error` and its handler that does `catch (...)` would intercept them.
  Plain C and C++ with RAII are fine (destructors actually run, unlike with
  `longjmp`).
* Calling a function *on a non-running thread* (`lua_call`/`lua_pcall` on a
  coroutine's `lua_State`) executes it on the running thread's stack; the
  results land on the target thread as expected.
* `os.setlocale`: the fast built-in paths for character classes, case
  conversion, string comparison and number parsing apply only while the
  locale is `"C"` (the default); other locales use the C runtime as before.

## Changes made to rilua

All in `RILUA_PATCHES.diff` (against rilua 0.1.24):

* Strings keep a trailing NUL byte (for `lua_tolstring`).
* `RustClosure.native` field to carry a C function pointer; `Userdata::data_ptr`;
  a thread-identity field for resumer threads; `auxresume` made public.
* Fidelity fixes found by differential testing against PUC-Rio 5.1.4:
  * library errors now carry the caller's `file:line:` prefix (`luaL_where`),
  * `luaO_chunkid` is ported byte-for-byte (source names in messages),
  * no phantom `[C]: ?` bottom frame in tracebacks and `getinfo` levels,
  * `print` writes through C `stdout` (stays ordered with `io.write` and host `printf`),
  * debug hooks are suppressed inside `__gc` finalizers, as in PUC-Rio,
  * NaN prints as `nan` / `-nan` by sign, like `%.14g`.
* Windows: C runtime imports fixed for non-MSVC builds, 64-bit
  `ftell`/`fseek`/`mktime` entry points (C `long` is 32-bit on Windows).
* Performance:
  * `string.gmatch` no longer copies the whole subject string per match
    (a 900 KB subject went from 3.1 s to 0.05 s),
  * `string.len/byte/sub/find/match` read their string argument in place
    instead of copying it (walking a 1 MB string: ~1000× faster),
  * in the "C" locale, character classes (`%a`, `%d`...), `upper`/`lower`,
    string comparison and number parsing use built-in ASCII rules instead of
    one C runtime call per character (big win with msvcrt.dll on Windows),
  * faster VM register access (one bounds check per access), a number-only
    arithmetic fast path, one instruction fetch per VM step,
  * mimalloc as the allocator; x86_64 builds target x86-64-v2 (hardware `floor`).
* Correctness: `%` uses PUC-Rio's unfused `a - floor(a/b)*b` (also in
  constant folding); `string.find/match` clamp a start index past the end
  (`match` used to crash with a Rust panic there).

## License and credits

Copyright (c) 2026 DaragonTech Unipessoal Lda. Licensed under **MIT OR
Apache-2.0**, at your option, the same terms as rilua; see [LICENSE](LICENSE)
and [LICENSE-APACHE](LICENSE-APACHE).

* **DaragonTech**: project owner, direction, testing on real hosts.
* **Claude (Anthropic)**: design and implementation of the C API layer,
  rilua patches, build tooling, proxy, tracer, tests and benchmarks.
* **rilua developers** (Daniel S. Reichenbach and WoW Emulation Contributors):
  the Lua 5.1 VM, compiler, GC and standard libraries in Rust (MIT OR Apache-2.0).
* **Lua.org, PUC-Rio**: Lua 5.1.4; the headers in `include/`, `csrc/lauxlib.c`
  and `test/lua.c` are used unmodified (MIT, see `include/COPYRIGHT-lua-5.1.4`).

The built libraries also contain mimalloc (MIT, Microsoft Corporation) and
the Rust standard library (MIT OR Apache-2.0). All notices are in `LICENSE`,
which should accompany redistributed binaries.
