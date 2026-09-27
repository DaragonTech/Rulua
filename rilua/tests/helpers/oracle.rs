//! Oracle comparison framework: runs Lua code in both rilua and PUC-Rio
//! Lua 5.1.1, comparing output for behavioral equivalence.
//!
//! The PUC-Rio `lua` binary path is read from the `LUA_REFERENCE_BIN`
//! environment variable. If not set, defaults to `./lua-5.1.1/src/lua`
//! (built from the official tarball). See `AGENTS.md` for setup
//! instructions. Tests that require the reference binary skip
//! gracefully if it is not available.

use std::path::PathBuf;
use std::process::Command;

/// Default path to the PUC-Rio Lua 5.1.1 binary.
///
/// Built from the official tarball at `./lua-5.1.1/`. See `AGENTS.md`
/// for download and build instructions. Override with the
/// `LUA_REFERENCE_BIN` environment variable.
const DEFAULT_LUA_BIN: &str = "./lua-5.1.1/src/lua";

/// Returns the path to the PUC-Rio Lua 5.1.1 reference binary.
///
/// Reads `LUA_REFERENCE_BIN` from the environment, falling back to
/// the default path if not set.
pub fn reference_bin() -> PathBuf {
    std::env::var("LUA_REFERENCE_BIN")
        .map_or_else(|_| PathBuf::from(DEFAULT_LUA_BIN), PathBuf::from)
}

/// Returns `true` if the reference Lua binary exists and is executable.
pub fn reference_available() -> bool {
    let bin = reference_bin();
    bin.exists()
}

/// Result of running Lua code in an interpreter.
#[derive(Debug)]
pub struct LuaOutput {
    /// Standard output.
    pub stdout: String,
    /// Standard error.
    pub stderr: String,
    /// Process exit code (0 = success).
    pub exit_code: i32,
}

/// Run Lua code in the PUC-Rio reference interpreter via `lua -e`.
///
/// Returns `None` if the reference binary is not available.
pub fn run_reference(code: &str) -> Option<LuaOutput> {
    let bin = reference_bin();
    if !bin.exists() {
        return None;
    }

    let output = Command::new(&bin).arg("-e").arg(code).output().ok()?;

    Some(LuaOutput {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        exit_code: output.status.code().unwrap_or(-1),
    })
}

/// Run a Lua file in the PUC-Rio reference interpreter.
///
/// Returns `None` if the reference binary is not available.
pub fn run_reference_file(path: &str) -> Option<LuaOutput> {
    let bin = reference_bin();
    if !bin.exists() {
        return None;
    }

    let output = Command::new(&bin).arg(path).output().ok()?;

    Some(LuaOutput {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        exit_code: output.status.code().unwrap_or(-1),
    })
}

/// Assert that the reference interpreter produces the expected stdout
/// for the given Lua code.
///
/// Skips the test if the reference binary is not available.
#[allow(dead_code)]
pub fn assert_reference_output(code: &str, expected_stdout: &str) {
    let Some(result) = run_reference(code) else {
        eprintln!("skipping: reference Lua binary not available");
        return;
    };
    assert_eq!(
        result.stdout, expected_stdout,
        "Reference Lua output mismatch for code: {code}\nstderr: {}",
        result.stderr,
    );
}

/// Run Lua code in rilua via `rilua -e`.
#[allow(dead_code, clippy::expect_used)]
pub fn run_rilua(code: &str) -> LuaOutput {
    let output = Command::new(env!("CARGO_BIN_EXE_rilua"))
        .arg("-e")
        .arg(code)
        .output()
        .expect("failed to run rilua binary");

    LuaOutput {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        exit_code: output.status.code().unwrap_or(-1),
    }
}

/// Run rilua with arbitrary arguments.
#[allow(dead_code, clippy::expect_used)]
pub fn run_rilua_args(args: &[&str]) -> LuaOutput {
    let output = Command::new(env!("CARGO_BIN_EXE_rilua"))
        .args(args)
        .output()
        .expect("failed to run rilua binary");

    LuaOutput {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        exit_code: output.status.code().unwrap_or(-1),
    }
}

/// Run rilua with arbitrary arguments and environment variables.
#[allow(dead_code, clippy::expect_used)]
pub fn run_rilua_args_env(args: &[&str], env_vars: &[(&str, &str)]) -> LuaOutput {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_rilua"));
    cmd.args(args);
    // Clear LUA_INIT to avoid interference, then set requested vars.
    cmd.env_remove("LUA_INIT");
    for (k, v) in env_vars {
        cmd.env(k, v);
    }
    let output = cmd.output().expect("failed to run rilua binary");

    LuaOutput {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        exit_code: output.status.code().unwrap_or(-1),
    }
}

/// Run rilua with piped stdin content.
#[allow(dead_code, clippy::expect_used)]
pub fn run_rilua_stdin(args: &[&str], stdin_data: &str) -> LuaOutput {
    use std::io::Write;
    use std::process::Stdio;

    let mut child = Command::new(env!("CARGO_BIN_EXE_rilua"))
        .args(args)
        .env_remove("LUA_INIT")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn rilua");

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(stdin_data.as_bytes()).ok();
    }

    let output = child.wait_with_output().expect("failed to wait on rilua");

    LuaOutput {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        exit_code: output.status.code().unwrap_or(-1),
    }
}

/// Run PUC-Rio reference with arbitrary arguments.
#[allow(dead_code)]
pub fn run_reference_args(args: &[&str]) -> Option<LuaOutput> {
    let bin = reference_bin();
    if !bin.exists() {
        return None;
    }

    let output = Command::new(&bin).args(args).output().ok()?;

    Some(LuaOutput {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        exit_code: output.status.code().unwrap_or(-1),
    })
}

/// Strips the binary-name prefix from stderr lines.
///
/// PUC-Rio outputs `"/path/to/lua: msg"` while rilua outputs
/// `"/path/to/rilua: msg"`. This function strips everything up to
/// and including the first `: ` on each line, returning only the
/// error content for comparison.
#[allow(dead_code)]
fn strip_progname(stderr: &str) -> String {
    stderr
        .lines()
        .map(|line| {
            if let Some(idx) = line.find(": ") {
                &line[idx + 2..]
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Assert that rilua and PUC-Rio produce matching stderr (with binary
/// prefixes stripped) and the same exit code.
///
/// Used for testing CLI error message and traceback formatting.
/// Skips the test if the reference binary is not available.
#[allow(dead_code)]
pub fn assert_matches_reference_stderr(code: &str) {
    let Some(reference) = run_reference(code) else {
        eprintln!("skipping: reference Lua binary not available");
        return;
    };
    let rilua = run_rilua(code);
    let rilua_err = strip_progname(&rilua.stderr);
    let ref_err = strip_progname(&reference.stderr);
    assert_eq!(
        rilua_err, ref_err,
        "Stderr mismatch for code: {code}\n  rilua stderr: {:?}\n  ref   stderr: {:?}",
        rilua.stderr, reference.stderr,
    );
    assert_eq!(
        rilua.exit_code, reference.exit_code,
        "Exit code mismatch for code: {code}\n  rilua: {}\n  ref:   {}",
        rilua.exit_code, reference.exit_code,
    );
}

/// Assert that rilua and PUC-Rio produce identical stdout for the given code.
///
/// Skips the test if the reference binary is not available.
#[allow(dead_code)]
pub fn assert_matches_reference(code: &str) {
    let Some(reference) = run_reference(code) else {
        eprintln!("skipping: reference Lua binary not available");
        return;
    };
    let rilua = run_rilua(code);
    assert_eq!(
        rilua.stdout, reference.stdout,
        "Output mismatch for code: {code}\n  rilua stdout: {:?}\n  ref   stdout: {:?}\n  rilua stderr: {}\n  ref   stderr: {}",
        rilua.stdout, reference.stdout, rilua.stderr, reference.stderr,
    );
    assert_eq!(
        rilua.exit_code, reference.exit_code,
        "Exit code mismatch for code: {code}\n  rilua: {}\n  ref:   {}",
        rilua.exit_code, reference.exit_code,
    );
}
