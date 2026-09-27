// Compiles the parts of the Lua 5.1 C API that are implemented in C:
//   * lauxlib.c   -- the unmodified auxiliary library from Lua 5.1.4
//   * lapi_fmt.c  -- lua_pushfstring / lua_pushvfstring (C varargs)
//
// On Windows the C objects are compiled with LUA_BUILD_AS_DLL so that the
// luaL_* functions carry __declspec(dllexport) and end up in the DLL's
// export table next to the Rust-implemented lua_* functions.
fn main() {
    let target = std::env::var("TARGET").unwrap_or_default();
    let windows = target.contains("windows");

    let mut b = cc::Build::new();
    b.include("include")
        .file("csrc/lauxlib.c")
        .file("csrc/lapi_fmt.c")
        .warnings(false);
    if windows {
        b.define("LUA_BUILD_AS_DLL", None).define("LUA_LIB", None);
        if target.contains("msvc") {
            b.define("_CRT_SECURE_NO_WARNINGS", None);
        }
    } else {
        // Unwind tables so Rust panics (lua_error) can travel through C frames.
        b.flag_if_supported("-fexceptions");
        b.define("LUA_USE_POSIX", None);
    }
    // +whole-archive keeps every object (and its dllexport directives) even
    // if nothing on the Rust side references it.
    b.link_lib_modifier("+whole-archive");
    b.compile("lua51_c");

    // 32-bit x86: errors travel by setjmp/longjmp (see csrc/ljmp.c).
    if std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("x86") {
        let mut j = cc::Build::new();
        j.include("include").file("csrc/ljmp.c").warnings(false);
        j.compile("lua51_ljmp");
    }

    println!("cargo:rerun-if-changed=csrc/lauxlib.c");
    println!("cargo:rerun-if-changed=csrc/lapi_fmt.c");
    println!("cargo:rerun-if-changed=csrc/ljmp.c");
    println!("cargo:rerun-if-changed=include");
}
