//! I/O library: file operations with userdata file handles.
//!
//! Reference: `liolib.c` in PUC-Rio Lua 5.1.1.

use crate::error::{LuaError, LuaResult, RuntimeError};
use crate::vm::closure::{Closure, RustClosure};
use crate::vm::gc::arena::GcRef;
use crate::vm::state::LuaState;
use crate::vm::table::Table;
use crate::vm::value::{Userdata, Val};

use crate::platform::{
    self, LibcFile, c_fprintf_number, c_fscanf_number, c_pclose, c_popen, c_stderr, c_stdin,
    c_stdout, clearerr, fclose, ferror, fflush, fgets, fopen, fread, fseek, ftell, fwrite, getc,
    setvbuf, strlen, tmpfile, ungetc,
};

/// fseek whence constants (POSIX).
const SEEK_SET: i32 = 0;
const SEEK_CUR: i32 = 1;
const SEEK_END: i32 = 2;

/// setvbuf mode constants.
const IONBF: i32 = platform::bufmode::IONBF;
const IOFBF: i32 = platform::bufmode::IOFBF;
const IOLBF: i32 = platform::bufmode::IOLBF;

/// Default buffer size for setvbuf (matches LUAL_BUFFERSIZE).
const LUAL_BUFFERSIZE: usize = 8192;

/// EOF sentinel.
const EOF: i32 = -1;

/// `LUA_FILEHANDLE` -- the registry key for the FILE* metatable.
const FILE_HANDLE: &str = "FILE*";

/// Registry keys for default input/output file handles.
const IO_INPUT_KEY: &str = "_IO_input";
const IO_OUTPUT_KEY: &str = "_IO_output";

// ---------------------------------------------------------------------------
// IoFile: the data stored inside each file userdata
// ---------------------------------------------------------------------------

/// Internal data for an I/O file handle, stored in `Userdata` via `Box<dyn Any>`.
///
/// Mirrors PUC-Rio's `FILE**` userdata. The `is_pipe` flag replaces PUC-Rio's
/// function-environment-based close dispatch.
struct IoFile {
    /// Raw C `FILE*` pointer, or null when closed.
    file: *mut LibcFile,
    /// `true` for handles opened via `io.popen` (use `pclose` instead of `fclose`).
    is_pipe: bool,
    /// `true` for stdin/stdout/stderr (skip close in `__gc`).
    is_std_handle: bool,
}

// IoFile contains a raw pointer which is !Send by default. The FILE*
// pointer can be safely transferred between threads (only concurrent
// access is unsafe, which is prevented by &mut Lua).
#[cfg(feature = "send")]
#[allow(unsafe_code)]
unsafe impl Send for IoFile {}

// ---------------------------------------------------------------------------
// Argument helpers (same pattern as other stdlib modules)
// ---------------------------------------------------------------------------

#[inline]
fn nargs(state: &LuaState) -> usize {
    state.top.saturating_sub(state.base)
}

#[inline]
fn arg(state: &LuaState, n: usize) -> Val {
    let idx = state.base + n;
    if idx < state.top {
        state.stack_get(idx)
    } else {
        Val::Nil
    }
}

fn bad_argument(name: &str, n: usize, msg: &str) -> LuaError {
    LuaError::Runtime(RuntimeError {
        message: format!("bad argument #{n} to '{name}' ({msg})"),
        level: 0,
        traceback: vec![],
    })
}

fn runtime_error(msg: String) -> LuaError {
    LuaError::Runtime(RuntimeError {
        message: msg,
        level: 0,
        traceback: vec![],
    })
}

fn check_string(state: &LuaState, name: &str, n: usize) -> LuaResult<Vec<u8>> {
    let val = arg(state, n);
    match val {
        Val::Str(r) => state
            .gc
            .string_arena
            .get(r)
            .map(|s| s.data().to_vec())
            .ok_or_else(|| bad_argument(name, n + 1, "string expected")),
        Val::Num(_) => Ok(format!("{val}").into_bytes()),
        _ => Err(bad_argument(name, n + 1, "string expected")),
    }
}

fn opt_string(state: &LuaState, name: &str, n: usize) -> LuaResult<Option<Vec<u8>>> {
    if nargs(state) <= n || matches!(arg(state, n), Val::Nil) {
        Ok(None)
    } else {
        check_string(state, name, n).map(Some)
    }
}

// ---------------------------------------------------------------------------
// Return helpers
// ---------------------------------------------------------------------------

/// Pushes result in PUC-Rio `pushresult` style: true on success,
/// nil + error message + errno on failure.
///
/// Uses `std::io::Error::last_os_error()` instead of raw errno/strerror FFI.
#[allow(clippy::unnecessary_wraps)]
fn pushresult(state: &mut LuaState, ok: bool, filename: Option<&str>) -> LuaResult<u32> {
    if ok {
        state.push(Val::Bool(true));
        return Ok(1);
    }
    let os_err = std::io::Error::last_os_error();
    let en = os_err.raw_os_error().unwrap_or(0);
    let msg = os_err.to_string();
    state.push(Val::Nil);
    let full_msg = if let Some(fname) = filename {
        format!("{fname}: {msg}")
    } else {
        msg
    };
    let msg_val = Val::Str(state.gc.intern_string(full_msg.as_bytes()));
    state.push(msg_val);
    state.push(Val::Num(f64::from(en)));
    Ok(3)
}

// ---------------------------------------------------------------------------
// File handle creation and validation
// ---------------------------------------------------------------------------

/// Creates a new file userdata with the FILE* metatable. The FILE* is
/// initially null (closed). Caller must set `io_file.file` after opening.
///
/// Matches PUC-Rio's `newfile` in `liolib.c`.
fn newfile(state: &mut LuaState) -> LuaResult<(GcRef<Userdata>, Val)> {
    let io_file = IoFile {
        file: std::ptr::null_mut(),
        is_pipe: false,
        is_std_handle: false,
    };
    let mt = super::new_metatable(state, FILE_HANDLE)?;
    let ud = Userdata::with_metatable(Box::new(io_file), mt);
    let ud_ref = state.gc.alloc_userdata(ud);
    let val = Val::Userdata(ud_ref);
    Ok((ud_ref, val))
}

/// Validates that argument at position `arg_n` (0-based) is a file userdata
/// and returns the `GcRef<Userdata>`. Does NOT check if the file is open.
///
/// Matches PUC-Rio's `topfile` macro in `liolib.c`.
fn topfile(state: &mut LuaState, arg_n: usize) -> LuaResult<GcRef<Userdata>> {
    super::check_userdata(state, arg_n, FILE_HANDLE)
}

/// Validates that arg 0 is an open file and returns the raw `FILE*` pointer.
///
/// Matches PUC-Rio's `tofile` in `liolib.c`.
#[allow(unsafe_code)]
fn tofile(state: &mut LuaState, arg_n: usize) -> LuaResult<*mut LibcFile> {
    let ud_ref = topfile(state, arg_n)?;
    let ud = state
        .gc
        .userdata
        .get(ud_ref)
        .ok_or_else(|| runtime_error("invalid file handle".into()))?;
    let io_file = ud
        .downcast_ref::<IoFile>()
        .ok_or_else(|| runtime_error("invalid file handle".into()))?;
    if io_file.file.is_null() {
        return Err(runtime_error("attempt to use a closed file".into()));
    }
    Ok(io_file.file)
}

/// Gets the default input or output file userdata from the registry.
fn getiofile(state: &mut LuaState, key: &str) -> LuaResult<GcRef<Userdata>> {
    let key_ref = state.gc.intern_string(key.as_bytes());
    let registry = state
        .gc
        .tables
        .get(state.registry)
        .ok_or_else(|| runtime_error("registry not found".into()))?;
    let val = registry.get_str(key_ref, &state.gc.string_arena);
    match val {
        Val::Userdata(r) => Ok(r),
        _ => Err(runtime_error(format!(
            "standard {} file is closed",
            if key == IO_INPUT_KEY {
                "input"
            } else {
                "output"
            }
        ))),
    }
}

/// Gets the FILE* from the default input or output file handle.
#[allow(unsafe_code)]
fn getiofile_ptr(state: &mut LuaState, key: &str) -> LuaResult<*mut LibcFile> {
    let ud_ref = getiofile(state, key)?;
    let ud = state
        .gc
        .userdata
        .get(ud_ref)
        .ok_or_else(|| runtime_error("invalid file handle".into()))?;
    let io_file = ud
        .downcast_ref::<IoFile>()
        .ok_or_else(|| runtime_error("invalid file handle".into()))?;
    if io_file.file.is_null() {
        return Err(runtime_error(format!(
            "standard {} file is closed",
            if key == IO_INPUT_KEY {
                "input"
            } else {
                "output"
            }
        )));
    }
    Ok(io_file.file)
}

/// Stores a file userdata as the default input or output in the registry.
fn setiofile(state: &mut LuaState, key: &str, val: Val) -> LuaResult<()> {
    let key_ref = state.gc.intern_string(key.as_bytes());
    let registry = state.registry;
    let reg = state
        .gc
        .tables
        .get_mut(registry)
        .ok_or_else(|| runtime_error("registry not found".into()))?;
    reg.raw_set(Val::Str(key_ref), val, &state.gc.string_arena)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Close dispatch
// ---------------------------------------------------------------------------

/// Closes a file handle, dispatching between `fclose` and `pclose`.
/// Sets the FILE* to null after closing.
///
/// Matches PUC-Rio's `aux_close` in `liolib.c`.
#[allow(unsafe_code)]
fn aux_close(state: &mut LuaState, ud_ref: GcRef<Userdata>) -> LuaResult<u32> {
    let ud = state
        .gc
        .userdata
        .get_mut(ud_ref)
        .ok_or_else(|| runtime_error("invalid file handle".into()))?;
    let io_file = ud
        .downcast_mut::<IoFile>()
        .ok_or_else(|| runtime_error("invalid file handle".into()))?;
    let fp = io_file.file;
    if fp.is_null() {
        return Err(runtime_error("attempt to use a closed file".into()));
    }
    io_file.file = std::ptr::null_mut();
    if io_file.is_pipe {
        let ok = c_pclose(fp) == 0;
        pushresult(state, ok, None)
    } else {
        let ok = unsafe { fclose(fp) } == 0;
        pushresult(state, ok, None)
    }
}

// ---------------------------------------------------------------------------
// io.type(obj)
// ---------------------------------------------------------------------------

/// `io.type(obj)` -- Returns "file", "closed file", or nil.
///
/// Matches PUC-Rio's `io_type` in `liolib.c`.
pub fn io_type(state: &mut LuaState) -> LuaResult<u32> {
    if nargs(state) == 0 {
        return Err(bad_argument("type", 1, "value expected"));
    }
    let val = arg(state, 0);
    let Val::Userdata(ud_ref) = val else {
        state.push(Val::Nil);
        return Ok(1);
    };

    // Check if the userdata has the FILE* metatable.
    let expected_mt = super::get_registry_metatable(state, FILE_HANDLE);
    let actual_mt = state.gc.userdata.get(ud_ref).and_then(Userdata::metatable);

    match (expected_mt, actual_mt) {
        (Some(expected), Some(actual)) if expected == actual => {
            // It's a file handle -- check if open or closed.
            let is_open = state
                .gc
                .userdata
                .get(ud_ref)
                .and_then(|ud| ud.downcast_ref::<IoFile>())
                .is_some_and(|io| !io.file.is_null());
            if is_open {
                let s = state.gc.intern_string(b"file");
                state.push(Val::Str(s));
            } else {
                let s = state.gc.intern_string(b"closed file");
                state.push(Val::Str(s));
            }
        }
        _ => {
            state.push(Val::Nil);
        }
    }
    Ok(1)
}

// ---------------------------------------------------------------------------
// io.open(filename [, mode])
// ---------------------------------------------------------------------------

/// `io.open(filename [, mode])` -- Opens a file.
///
/// Returns file handle on success, or nil + error message on failure.
/// Matches PUC-Rio's `io_open` in `liolib.c`.
#[allow(unsafe_code)]
pub fn io_open(state: &mut LuaState) -> LuaResult<u32> {
    let filename = check_string(state, "open", 0)?;
    let mode = opt_string(state, "open", 1)?.unwrap_or_else(|| b"r".to_vec());

    // Create file handle (initially closed).
    let (ud_ref, ud_val) = newfile(state)?;

    // NUL-terminate for C.
    let mut fname_c = filename.clone();
    fname_c.push(0);
    let mut mode_c = mode;
    mode_c.push(0);

    let fp = unsafe { fopen(fname_c.as_ptr(), mode_c.as_ptr()) };
    if fp.is_null() {
        let fname_str = String::from_utf8_lossy(&filename);
        return pushresult(state, false, Some(&fname_str));
    }

    // Set the FILE* on the userdata.
    let ud = state
        .gc
        .userdata
        .get_mut(ud_ref)
        .ok_or_else(|| runtime_error("file handle lost".into()))?;
    let io_file = ud
        .downcast_mut::<IoFile>()
        .ok_or_else(|| runtime_error("invalid file handle".into()))?;
    io_file.file = fp;

    state.push(ud_val);
    Ok(1)
}

// ---------------------------------------------------------------------------
// io.close([file]) / file:close()
// ---------------------------------------------------------------------------

/// `io.close([file])` -- Closes a file, or the default output.
///
/// Matches PUC-Rio's `io_close` in `liolib.c`.
pub fn io_close(state: &mut LuaState) -> LuaResult<u32> {
    if nargs(state) == 0 || matches!(arg(state, 0), Val::Nil) {
        // Close default output.
        let ud_ref = getiofile(state, IO_OUTPUT_KEY)?;
        return aux_close(state, ud_ref);
    }
    let ud_ref = topfile(state, 0)?;
    aux_close(state, ud_ref)
}

// ---------------------------------------------------------------------------
// __gc metamethod
// ---------------------------------------------------------------------------

/// `__gc` metamethod for file handles.
///
/// Closes the file unless it's a standard handle or already closed.
/// Matches PUC-Rio's `io_gc` in `liolib.c`.
#[allow(unsafe_code)]
pub fn io_gc(state: &mut LuaState) -> LuaResult<u32> {
    let ud_ref = topfile(state, 0)?;
    let ud = state
        .gc
        .userdata
        .get(ud_ref)
        .ok_or_else(|| runtime_error("invalid file handle".into()))?;
    let io_file = ud
        .downcast_ref::<IoFile>()
        .ok_or_else(|| runtime_error("invalid file handle".into()))?;
    if !io_file.file.is_null() && !io_file.is_std_handle {
        // Close the file, ignoring errors.
        let _ = aux_close(state, ud_ref);
    }
    Ok(0)
}

// ---------------------------------------------------------------------------
// __tostring metamethod
// ---------------------------------------------------------------------------

/// `__tostring` metamethod for file handles.
///
/// Returns `"file (0xADDR)"` or `"file (closed)"`.
/// Matches PUC-Rio's `io_tostring` in `liolib.c`.
pub fn io_tostring(state: &mut LuaState) -> LuaResult<u32> {
    let ud_ref = topfile(state, 0)?;
    let ud = state
        .gc
        .userdata
        .get(ud_ref)
        .ok_or_else(|| runtime_error("invalid file handle".into()))?;
    let io_file = ud
        .downcast_ref::<IoFile>()
        .ok_or_else(|| runtime_error("invalid file handle".into()))?;
    let s = if io_file.file.is_null() {
        "file (closed)".to_string()
    } else {
        format!("file ({:p})", io_file.file)
    };
    let val = Val::Str(state.gc.intern_string(s.as_bytes()));
    state.push(val);
    Ok(1)
}

// ---------------------------------------------------------------------------
// Write operations
// ---------------------------------------------------------------------------

/// Generic write function. Writes all arguments starting at `first_arg`
/// to the given FILE* pointer.
///
/// Matches PUC-Rio's `g_write` in `liolib.c`.
#[allow(unsafe_code)]
fn g_write(state: &mut LuaState, fp: *mut LibcFile, first_arg: usize) -> LuaResult<u32> {
    let n = nargs(state);
    let mut status = true;
    for i in first_arg..n {
        let val = arg(state, i);
        match val {
            Val::Num(d) => {
                // fprintf(f, LUA_NUMBER_FMT, d) where LUA_NUMBER_FMT is "%.14g"
                let ok = c_fprintf_number(fp, d) > 0;
                status = status && ok;
            }
            Val::Str(r) => {
                let data = state
                    .gc
                    .string_arena
                    .get(r)
                    .map_or(&b""[..], crate::vm::string::LuaString::data);
                let len = data.len();
                let written = unsafe { fwrite(data.as_ptr(), 1, len, fp) };
                status = status && (written == len);
            }
            _ => {
                return Err(bad_argument(
                    "write",
                    i - first_arg + 1,
                    "string or number expected",
                ));
            }
        }
    }
    pushresult(state, status, None)
}

/// `io.write(...)` -- Writes to the default output file.
pub fn io_write(state: &mut LuaState) -> LuaResult<u32> {
    let fp = getiofile_ptr(state, IO_OUTPUT_KEY)?;
    g_write(state, fp, 0)
}

/// `file:write(...)` -- Writes to the file handle.
pub fn f_write(state: &mut LuaState) -> LuaResult<u32> {
    let fp = tofile(state, 0)?;
    g_write(state, fp, 1)
}

// ---------------------------------------------------------------------------
// Flush operations
// ---------------------------------------------------------------------------

/// `io.flush()` -- Flushes the default output file.
#[allow(unsafe_code)]
pub fn io_flush(state: &mut LuaState) -> LuaResult<u32> {
    let fp = getiofile_ptr(state, IO_OUTPUT_KEY)?;
    let ok = unsafe { fflush(fp) } == 0;
    pushresult(state, ok, None)
}

/// `file:flush()` -- Flushes the file handle.
#[allow(unsafe_code)]
pub fn f_flush(state: &mut LuaState) -> LuaResult<u32> {
    let fp = tofile(state, 0)?;
    let ok = unsafe { fflush(fp) } == 0;
    pushresult(state, ok, None)
}

// ---------------------------------------------------------------------------
// Read operations
// ---------------------------------------------------------------------------

/// Reads a number from the file using `fscanf(f, "%lf", &d)`.
///
/// Returns `true` on success (pushes number), `false` on failure (pushes nothing).
/// Matches PUC-Rio's `read_number` in `liolib.c`.
fn read_number(state: &mut LuaState, fp: *mut LibcFile) -> bool {
    if let Some(d) = c_fscanf_number(fp) {
        state.push(Val::Num(d));
        true
    } else {
        false
    }
}

/// Reads a line from the file, stripping the trailing `\n`.
///
/// Returns `true` if something was read (pushes string), `false` at pure EOF.
/// Matches PUC-Rio's `read_line` in `liolib.c`.
#[allow(unsafe_code)]
fn read_line(state: &mut LuaState, fp: *mut LibcFile) -> bool {
    let mut result = Vec::new();
    let mut buf = [0u8; 1024];
    loop {
        let p = unsafe {
            fgets(
                buf.as_mut_ptr(),
                #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
                {
                    buf.len() as i32
                },
                fp,
            )
        };
        if p.is_null() {
            // EOF or error.
            if result.is_empty() {
                return false;
            }
            let s = state.gc.intern_string(&result);
            state.push(Val::Str(s));
            return true;
        }
        let len = unsafe { strlen(buf.as_ptr()) };
        if len > 0 && buf[len - 1] == b'\n' {
            // Got end-of-line: add everything except the \n.
            result.extend_from_slice(&buf[..len - 1]);
            let s = state.gc.intern_string(&result);
            state.push(Val::Str(s));
            return true;
        }
        // No newline yet -- line is longer than buffer.
        result.extend_from_slice(&buf[..len]);
    }
}

/// Reads exactly `n` bytes from the file.
///
/// Returns `true` if all bytes were read (or at least some for partial),
/// `false` if nothing was read.
/// Matches PUC-Rio's `read_chars` in `liolib.c`.
#[allow(unsafe_code)]
fn read_chars(state: &mut LuaState, fp: *mut LibcFile, mut n: usize) -> bool {
    let mut result = Vec::with_capacity(n.min(8192));
    let mut buf = [0u8; 8192];
    while n > 0 {
        let to_read = n.min(buf.len());
        let nr = unsafe { fread(buf.as_mut_ptr(), 1, to_read, fp) };
        result.extend_from_slice(&buf[..nr]);
        n -= nr;
        if nr < to_read {
            break; // EOF or error
        }
    }
    // PUC-Rio: success if we read all bytes OR if we read at least something
    let success = n == 0 || !result.is_empty();
    let s = state.gc.intern_string(&result);
    state.push(Val::Str(s));
    success
}

/// Tests for EOF by reading and ungetting a character.
///
/// Pushes empty string and returns true if NOT at EOF.
/// Matches PUC-Rio's `test_eof` in `liolib.c`.
#[allow(unsafe_code)]
fn test_eof(state: &mut LuaState, fp: *mut LibcFile) -> bool {
    let c = unsafe { getc(fp) };
    unsafe { ungetc(c, fp) };
    let s = state.gc.intern_string(b"");
    state.push(Val::Str(s));
    c != EOF
}

/// Generic read function. Reads from file based on format arguments.
///
/// Matches PUC-Rio's `g_read` in `liolib.c`.
#[allow(unsafe_code)]
fn g_read(state: &mut LuaState, fp: *mut LibcFile, first_arg: usize) -> LuaResult<u32> {
    let n_total_args = nargs(state);
    let n_read_args = n_total_args.saturating_sub(first_arg);

    unsafe { clearerr(fp) };

    if n_read_args == 0 {
        // No format arguments: default is read line.
        let success = read_line(state, fp);
        if unsafe { ferror(fp) } != 0 {
            return pushresult(state, false, None);
        }
        if !success {
            // Pop the failed result and push nil.
            state.pop();
            state.push(Val::Nil);
        }
        return Ok(1);
    }

    let mut count = 0u32;
    let mut success = true;
    for i in first_arg..n_total_args {
        if !success {
            break;
        }
        let val = arg(state, i);
        match val {
            Val::Num(d) => {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let n = d as usize;
                if n == 0 {
                    success = test_eof(state, fp);
                } else {
                    success = read_chars(state, fp, n);
                }
                count += 1;
            }
            Val::Str(r) => {
                let data = state
                    .gc
                    .string_arena
                    .get(r)
                    .map(|s| s.data().to_vec())
                    .unwrap_or_default();
                // Must start with '*'.
                if data.first() != Some(&b'*') {
                    return Err(bad_argument("read", i - first_arg + 1, "invalid option"));
                }
                match data.get(1) {
                    Some(b'n') => {
                        success = read_number(state, fp);
                        count += 1;
                    }
                    Some(b'l') => {
                        success = read_line(state, fp);
                        count += 1;
                    }
                    Some(b'a') => {
                        // Read all remaining content.
                        read_chars(state, fp, usize::MAX);
                        success = true; // always succeeds
                        count += 1;
                    }
                    _ => {
                        return Err(bad_argument("read", i - first_arg + 1, "invalid format"));
                    }
                }
            }
            _ => {
                return Err(bad_argument("read", i - first_arg + 1, "invalid option"));
            }
        }
    }

    if unsafe { ferror(fp) } != 0 {
        return pushresult(state, false, None);
    }

    if !success {
        // Pop the last pushed value and push nil.
        state.pop();
        state.push(Val::Nil);
    }

    Ok(count)
}

/// `io.read(...)` -- Reads from the default input file.
pub fn io_read(state: &mut LuaState) -> LuaResult<u32> {
    let fp = getiofile_ptr(state, IO_INPUT_KEY)?;
    g_read(state, fp, 0)
}

/// `file:read(...)` -- Reads from the file handle.
pub fn f_read(state: &mut LuaState) -> LuaResult<u32> {
    let fp = tofile(state, 0)?;
    g_read(state, fp, 1)
}

// ---------------------------------------------------------------------------
// io.input / io.output
// ---------------------------------------------------------------------------

/// Generic get/set default I/O file.
///
/// - No args: return current default.
/// - String arg: open file with given mode, set as default, return it.
/// - File handle arg: set as default, return it.
///
/// Matches PUC-Rio's `g_iofile` in `liolib.c`.
#[allow(unsafe_code)]
fn g_iofile(state: &mut LuaState, key: &str, mode: &str) -> LuaResult<u32> {
    if nargs(state) > 0 && !matches!(arg(state, 0), Val::Nil) {
        let val = arg(state, 0);
        if let Val::Str(_) = val {
            // String argument: open the file.
            let func_name = if key == IO_INPUT_KEY {
                "input"
            } else {
                "output"
            };
            let filename = check_string(state, func_name, 0)?;
            let (ud_ref, ud_val) = newfile(state)?;

            let mut fname_c = filename.clone();
            fname_c.push(0);
            let mut mode_c = mode.as_bytes().to_vec();
            mode_c.push(0);

            let fp = unsafe { fopen(fname_c.as_ptr(), mode_c.as_ptr()) };
            if fp.is_null() {
                let fname_str = String::from_utf8_lossy(&filename);
                let os_err = std::io::Error::last_os_error();
                return Err(bad_argument(
                    func_name,
                    1,
                    &format!("{fname_str}: {os_err}"),
                ));
            }

            let ud = state
                .gc
                .userdata
                .get_mut(ud_ref)
                .ok_or_else(|| runtime_error("file handle lost".into()))?;
            let io_file = ud
                .downcast_mut::<IoFile>()
                .ok_or_else(|| runtime_error("invalid file handle".into()))?;
            io_file.file = fp;

            setiofile(state, key, ud_val)?;
        } else {
            // Should be a file handle -- validate it.
            let _ = topfile(state, 0)?;
            setiofile(state, key, val)?;
        }
    }

    // Return current default.
    let ud_ref = getiofile(state, key)?;
    state.push(Val::Userdata(ud_ref));
    Ok(1)
}

/// `io.input([file])` -- Gets/sets the default input file.
pub fn io_input(state: &mut LuaState) -> LuaResult<u32> {
    g_iofile(state, IO_INPUT_KEY, "r")
}

/// `io.output([file])` -- Gets/sets the default output file.
pub fn io_output(state: &mut LuaState) -> LuaResult<u32> {
    g_iofile(state, IO_OUTPUT_KEY, "w")
}

// ---------------------------------------------------------------------------
// file:seek([whence [, offset]])
// ---------------------------------------------------------------------------

/// `file:seek([whence [, offset]])` -- Sets/gets the file position.
///
/// whence: "set" (default), "cur", "end". offset default 0.
/// Returns new position or nil + error message.
/// Matches PUC-Rio's `f_seek` in `liolib.c`.
#[allow(unsafe_code)]
pub fn f_seek(state: &mut LuaState) -> LuaResult<u32> {
    let fp = tofile(state, 0)?;

    // Parse whence (default "cur").
    let whence_str = opt_string(state, "seek", 1)?.unwrap_or_else(|| b"cur".to_vec());
    let whence = match whence_str.as_slice() {
        b"set" => SEEK_SET,
        b"cur" => SEEK_CUR,
        b"end" => SEEK_END,
        _ => return Err(bad_argument("seek", 2, "invalid option")),
    };

    // Parse offset (default 0).
    let offset = if nargs(state) > 2 && !matches!(arg(state, 2), Val::Nil) {
        match arg(state, 2) {
            Val::Num(d) => {
                #[allow(clippy::cast_possible_truncation)]
                {
                    d as i64
                }
            }
            _ => return Err(bad_argument("seek", 3, "number expected")),
        }
    } else {
        0
    };

    let result = unsafe { fseek(fp, offset, whence) };
    if result != 0 {
        return pushresult(state, false, None);
    }

    let pos = unsafe { ftell(fp) };
    #[allow(clippy::cast_precision_loss)]
    state.push(Val::Num(pos as f64));
    Ok(1)
}

// ---------------------------------------------------------------------------
// file:setvbuf(mode [, size])
// ---------------------------------------------------------------------------

/// `file:setvbuf(mode [, size])` -- Sets the buffering mode.
///
/// mode: "no", "full", "line". size defaults to LUAL_BUFFERSIZE.
/// Matches PUC-Rio's `f_setvbuf` in `liolib.c`.
#[allow(unsafe_code)]
pub fn f_setvbuf(state: &mut LuaState) -> LuaResult<u32> {
    let fp = tofile(state, 0)?;

    let mode_str = check_string(state, "setvbuf", 1)?;
    let mode = match mode_str.as_slice() {
        b"no" => IONBF,
        b"full" => IOFBF,
        b"line" => IOLBF,
        _ => return Err(bad_argument("setvbuf", 1, "invalid option")),
    };

    let size = if nargs(state) > 2 && !matches!(arg(state, 2), Val::Nil) {
        match arg(state, 2) {
            Val::Num(d) => {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                {
                    d as usize
                }
            }
            _ => LUAL_BUFFERSIZE,
        }
    } else {
        LUAL_BUFFERSIZE
    };

    let result = unsafe { setvbuf(fp, std::ptr::null_mut(), mode, size) };
    pushresult(state, result == 0, None)
}

// ---------------------------------------------------------------------------
// Lines iterator
// ---------------------------------------------------------------------------

/// Creates an iterator closure that reads lines from a file.
///
/// Upvalues: `[file_userdata_val, toclose_bool]`.
/// Matches PUC-Rio's `aux_lines` in `liolib.c`.
fn aux_lines(state: &mut LuaState, file_val: Val, toclose: bool) -> u32 {
    let closure = Closure::Rust(RustClosure {
        func: io_readline,
        upvalues: vec![file_val, Val::Bool(toclose)],
        name: "(for generator)".to_string(),
        env: None,
        native: 0,
    });
    let closure_ref = state.gc.alloc_closure(closure);
    state.push(Val::Function(closure_ref));
    1
}

/// Iterator function for `io.lines` / `file:lines()`.
///
/// Reads one line per call. On EOF, optionally closes the file (if
/// upvalue[1] is true) and returns 0 to stop iteration.
/// Matches PUC-Rio's `io_readline` in `liolib.c`.
#[allow(unsafe_code)]
fn io_readline(state: &mut LuaState) -> LuaResult<u32> {
    // Read upvalues from the closure.
    let func_idx = state.call_stack[state.ci].func;
    let func_val = state.stack_get(func_idx);
    let Val::Function(closure_ref) = func_val else {
        return Ok(0);
    };

    let (file_val, toclose) = {
        let cl = state
            .gc
            .closures
            .get(closure_ref)
            .ok_or_else(|| runtime_error("io_readline: invalid closure".into()))?;
        let upvalues = match cl {
            Closure::Rust(rc) => &rc.upvalues,
            Closure::Lua(_) => return Ok(0),
        };
        if upvalues.len() < 2 {
            return Ok(0);
        }
        (upvalues[0], matches!(upvalues[1], Val::Bool(true)))
    };

    // Get the FILE* from the upvalue.
    let Val::Userdata(ud_ref) = file_val else {
        return Err(runtime_error("file is already closed".into()));
    };
    let fp = {
        let ud = state
            .gc
            .userdata
            .get(ud_ref)
            .ok_or_else(|| runtime_error("file is already closed".into()))?;
        let io_file = ud
            .downcast_ref::<IoFile>()
            .ok_or_else(|| runtime_error("file is already closed".into()))?;
        if io_file.file.is_null() {
            return Err(runtime_error("file is already closed".into()));
        }
        io_file.file
    };

    let success = read_line(state, fp);

    if unsafe { ferror(fp) } != 0 {
        let os_err = std::io::Error::last_os_error();
        return Err(runtime_error(os_err.to_string()));
    }

    if success {
        Ok(1)
    } else {
        // EOF: optionally close the file.
        if toclose {
            let _ = aux_close(state, ud_ref);
        }
        Ok(0)
    }
}

/// `file:lines()` -- Returns a line iterator.
pub fn f_lines(state: &mut LuaState) -> LuaResult<u32> {
    let _ = tofile(state, 0)?; // validate open file
    let file_val = arg(state, 0);
    Ok(aux_lines(state, file_val, false))
}

/// `io.lines([filename])` -- Returns a line iterator.
///
/// No args: iterate over default input.
/// String arg: open file for reading, iterate, auto-close on EOF.
/// Matches PUC-Rio's `io_lines` in `liolib.c`.
#[allow(unsafe_code)]
pub fn io_lines(state: &mut LuaState) -> LuaResult<u32> {
    if nargs(state) == 0 || matches!(arg(state, 0), Val::Nil) {
        // Use default input.
        let ud_ref = getiofile(state, IO_INPUT_KEY)?;
        let file_val = Val::Userdata(ud_ref);
        return Ok(aux_lines(state, file_val, false));
    }

    // Open the file for reading.
    let filename = check_string(state, "lines", 0)?;
    let (ud_ref, ud_val) = newfile(state)?;

    let mut fname_c = filename.clone();
    fname_c.push(0);
    let mode_c = b"r\0";

    let fp = unsafe { fopen(fname_c.as_ptr(), mode_c.as_ptr()) };
    if fp.is_null() {
        let fname_str = String::from_utf8_lossy(&filename);
        let os_err = std::io::Error::last_os_error();
        return Err(bad_argument("lines", 1, &format!("{fname_str}: {os_err}")));
    }

    let ud = state
        .gc
        .userdata
        .get_mut(ud_ref)
        .ok_or_else(|| runtime_error("file handle lost".into()))?;
    let io_file = ud
        .downcast_mut::<IoFile>()
        .ok_or_else(|| runtime_error("invalid file handle".into()))?;
    io_file.file = fp;

    Ok(aux_lines(state, ud_val, true)) // toclose=true: auto-close on EOF
}

// ---------------------------------------------------------------------------
// io.tmpfile()
// ---------------------------------------------------------------------------

/// `io.tmpfile()` -- Creates a temporary file.
///
/// Returns file handle or nil + error message.
/// Matches PUC-Rio's `io_tmpfile` in `liolib.c`.
#[allow(unsafe_code)]
pub fn io_tmpfile(state: &mut LuaState) -> LuaResult<u32> {
    let (ud_ref, ud_val) = newfile(state)?;
    let fp = unsafe { tmpfile() };
    if fp.is_null() {
        return pushresult(state, false, None);
    }
    let ud = state
        .gc
        .userdata
        .get_mut(ud_ref)
        .ok_or_else(|| runtime_error("file handle lost".into()))?;
    let io_file = ud
        .downcast_mut::<IoFile>()
        .ok_or_else(|| runtime_error("invalid file handle".into()))?;
    io_file.file = fp;

    state.push(ud_val);
    Ok(1)
}

// ---------------------------------------------------------------------------
// io.popen(prog [, mode])
// ---------------------------------------------------------------------------

/// `io.popen(prog [, mode])` -- Opens a process with a pipe.
///
/// Returns file handle or nil + error message.
/// Matches PUC-Rio's `io_popen` in `liolib.c`.
#[allow(unsafe_code)]
pub fn io_popen(state: &mut LuaState) -> LuaResult<u32> {
    let command = check_string(state, "popen", 0)?;
    let mode = opt_string(state, "popen", 1)?.unwrap_or_else(|| b"r".to_vec());

    let (ud_ref, ud_val) = newfile(state)?;

    let mut cmd_c = command.clone();
    cmd_c.push(0);
    let mut mode_c = mode;
    mode_c.push(0);

    let fp = c_popen(cmd_c.as_ptr(), mode_c.as_ptr());
    if fp.is_null() {
        let cmd_str = String::from_utf8_lossy(&command);
        return pushresult(state, false, Some(&cmd_str));
    }

    let ud = state
        .gc
        .userdata
        .get_mut(ud_ref)
        .ok_or_else(|| runtime_error("file handle lost".into()))?;
    let io_file = ud
        .downcast_mut::<IoFile>()
        .ok_or_else(|| runtime_error("invalid file handle".into()))?;
    io_file.file = fp;
    io_file.is_pipe = true;

    state.push(ud_val);
    Ok(1)
}

// ---------------------------------------------------------------------------
// Registration: metatable creation + library opening
// ---------------------------------------------------------------------------

/// Creates the FILE* metatable with all file methods and metamethods.
///
/// Matches PUC-Rio's `createmeta` in `liolib.c`.
fn createmeta(state: &mut LuaState) -> LuaResult<GcRef<Table>> {
    let mt = super::new_metatable(state, FILE_HANDLE)?;

    // Set __index = metatable (self-indexing for method lookup).
    let index_key = state.gc.intern_string(b"__index");
    let mt_table = state
        .gc
        .tables
        .get_mut(mt)
        .ok_or_else(|| runtime_error("FILE* metatable not found".into()))?;
    mt_table.raw_set(Val::Str(index_key), Val::Table(mt), &state.gc.string_arena)?;

    // Register file methods + metamethods.
    super::register_table_fn(state, mt, "close", io_close)?;
    super::register_table_fn(state, mt, "flush", f_flush)?;
    super::register_table_fn(state, mt, "lines", f_lines)?;
    super::register_table_fn(state, mt, "read", f_read)?;
    super::register_table_fn(state, mt, "seek", f_seek)?;
    super::register_table_fn(state, mt, "setvbuf", f_setvbuf)?;
    super::register_table_fn(state, mt, "write", f_write)?;
    super::register_table_fn(state, mt, "__gc", io_gc)?;
    super::register_table_fn(state, mt, "__tostring", io_tostring)?;

    Ok(mt)
}

/// Creates a standard file handle userdata (stdin/stdout/stderr) and
/// stores it as a field on the io table + optionally as the default
/// input/output in the registry.
///
/// Matches PUC-Rio's `createstdfile` in `liolib.c`.
fn createstdfile(
    state: &mut LuaState,
    fp: *mut LibcFile,
    io_table: GcRef<Table>,
    field_name: &str,
    registry_key: Option<&str>,
) -> LuaResult<()> {
    let mt = super::new_metatable(state, FILE_HANDLE)?;
    let io_file = IoFile {
        file: fp,
        is_pipe: false,
        is_std_handle: true,
    };
    let ud = Userdata::with_metatable(Box::new(io_file), mt);
    let ud_ref = state.gc.alloc_userdata(ud);
    let ud_val = Val::Userdata(ud_ref);

    // Store in the io table.
    let key = state.gc.intern_string(field_name.as_bytes());
    let table = state
        .gc
        .tables
        .get_mut(io_table)
        .ok_or_else(|| runtime_error("io table not found".into()))?;
    table.raw_set(Val::Str(key), ud_val, &state.gc.string_arena)?;

    // Optionally store as default input/output.
    if let Some(rkey) = registry_key {
        setiofile(state, rkey, ud_val)?;
    }

    Ok(())
}

/// Opens the I/O library and registers it as the `io` global.
///
/// Matches PUC-Rio's `luaopen_io` in `liolib.c`.
#[allow(unsafe_code)]
pub fn open_io_lib(state: &mut LuaState) -> LuaResult<()> {
    // Create FILE* metatable with methods.
    createmeta(state)?;

    // Create the io library table.
    let io_table = state.gc.alloc_table(Table::new());

    // Register library functions.
    super::register_table_fn(state, io_table, "close", io_close)?;
    super::register_table_fn(state, io_table, "flush", io_flush)?;
    super::register_table_fn(state, io_table, "input", io_input)?;
    super::register_table_fn(state, io_table, "lines", io_lines)?;
    super::register_table_fn(state, io_table, "open", io_open)?;
    super::register_table_fn(state, io_table, "output", io_output)?;
    super::register_table_fn(state, io_table, "popen", io_popen)?;
    super::register_table_fn(state, io_table, "read", io_read)?;
    super::register_table_fn(state, io_table, "tmpfile", io_tmpfile)?;
    super::register_table_fn(state, io_table, "type", io_type)?;
    super::register_table_fn(state, io_table, "write", io_write)?;

    // Create standard file handles.
    createstdfile(state, c_stdin(), io_table, "stdin", Some(IO_INPUT_KEY))?;
    createstdfile(state, c_stdout(), io_table, "stdout", Some(IO_OUTPUT_KEY))?;
    createstdfile(state, c_stderr(), io_table, "stderr", None)?;

    // Register as global "io".
    super::register_global_val(state, "io", Val::Table(io_table))?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::any::Any;

    use super::*;

    #[test]
    fn iofile_null_on_creation() {
        let io = IoFile {
            file: std::ptr::null_mut(),
            is_pipe: false,
            is_std_handle: false,
        };
        assert!(io.file.is_null());
        assert!(!io.is_pipe);
        assert!(!io.is_std_handle);
    }

    #[test]
    fn iofile_is_any() {
        // Verify IoFile can be stored in Box<dyn Any> and downcast.
        let io = IoFile {
            file: std::ptr::null_mut(),
            is_pipe: false,
            is_std_handle: false,
        };
        let boxed: Box<dyn Any> = Box::new(io);
        assert!(boxed.downcast_ref::<IoFile>().is_some());
    }

    #[test]
    fn libc_stdin_not_null() {
        assert!(!c_stdin().is_null());
    }

    #[test]
    fn libc_stdout_not_null() {
        assert!(!c_stdout().is_null());
    }

    #[test]
    fn libc_stderr_not_null() {
        assert!(!c_stderr().is_null());
    }

    #[test]
    #[allow(unsafe_code)]
    fn libc_tmpfile_open_close() {
        let fp = unsafe { tmpfile() };
        assert!(!fp.is_null());
        let result = unsafe { fclose(fp) };
        assert_eq!(result, 0);
    }

    #[test]
    #[allow(unsafe_code)]
    fn libc_fopen_nonexistent() {
        let path = b"/tmp/__rilua_nonexistent_file__\0";
        let mode = b"r\0";
        let fp = unsafe { fopen(path.as_ptr(), mode.as_ptr()) };
        assert!(fp.is_null());
    }

    #[test]
    #[allow(unsafe_code)]
    fn libc_fopen_fwrite_fread_fclose() {
        let path = b"/tmp/__rilua_io_test__\0";
        let wmode = b"w\0";
        let rmode = b"r\0";

        // Write.
        let fp = unsafe { fopen(path.as_ptr(), wmode.as_ptr()) };
        assert!(!fp.is_null());
        let data = b"hello world";
        let written = unsafe { fwrite(data.as_ptr(), 1, data.len(), fp) };
        assert_eq!(written, data.len());
        assert_eq!(unsafe { fclose(fp) }, 0);

        // Read back.
        let fp = unsafe { fopen(path.as_ptr(), rmode.as_ptr()) };
        assert!(!fp.is_null());
        let mut buf = [0u8; 64];
        let n = unsafe { fread(buf.as_mut_ptr(), 1, buf.len(), fp) };
        assert_eq!(n, data.len());
        assert_eq!(&buf[..n], data);
        assert_eq!(unsafe { fclose(fp) }, 0);

        // Clean up.
        let _ = std::fs::remove_file("/tmp/__rilua_io_test__");
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    #[allow(unsafe_code, clippy::expect_used)]
    fn libc_popen_echo() {
        let cmd = b"echo hello\0";
        let mode = b"r\0";
        let fp = c_popen(cmd.as_ptr(), mode.as_ptr());
        assert!(!fp.is_null());
        let mut buf = [0u8; 64];
        let n = unsafe { fread(buf.as_mut_ptr(), 1, buf.len(), fp) };
        assert!(n > 0);
        assert!(
            std::str::from_utf8(&buf[..n])
                .expect("valid utf8")
                .starts_with("hello")
        );
        assert_eq!(c_pclose(fp), 0);
    }

    #[test]
    fn seek_constants() {
        assert_eq!(SEEK_SET, 0);
        assert_eq!(SEEK_CUR, 1);
        assert_eq!(SEEK_END, 2);
    }

    #[test]
    fn setvbuf_constants_distinct() {
        // Values are platform-specific but must be distinct.
        assert_ne!(IONBF, IOFBF);
        assert_ne!(IONBF, IOLBF);
        assert_ne!(IOFBF, IOLBF);
    }
}
