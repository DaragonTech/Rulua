//! Minimal platform layer: dynamic libraries and the executable's directory.

use std::ffi::{CString, c_char, c_void};

pub struct Library {
    pub path: String,
    handle: *mut c_void,
}

#[cfg(windows)]
mod imp {
    use super::*;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn LoadLibraryA(name: *const c_char) -> *mut c_void;
        fn GetProcAddress(h: *mut c_void, name: *const c_char) -> *mut c_void;
        fn FreeLibrary(h: *mut c_void) -> i32;
        fn GetLastError() -> u32;
        fn FormatMessageA(
            flags: u32,
            src: *const c_void,
            msgid: u32,
            lang: u32,
            buf: *mut c_char,
            size: u32,
            args: *mut c_void,
        ) -> u32;
        fn GetModuleFileNameA(h: *mut c_void, buf: *mut c_char, size: u32) -> u32;
    }

    /// PUC-Rio `pusherror` (loadlib.c, Windows flavour).
    pub fn last_error() -> String {
        unsafe {
            let err = GetLastError();
            let mut buf = [0 as c_char; 128];
            const FORMAT_MESSAGE_IGNORE_INSERTS: u32 = 0x0000_0200;
            const FORMAT_MESSAGE_FROM_SYSTEM: u32 = 0x0000_1000;
            let n = FormatMessageA(
                FORMAT_MESSAGE_IGNORE_INSERTS | FORMAT_MESSAGE_FROM_SYSTEM,
                std::ptr::null(),
                err,
                0,
                buf.as_mut_ptr(),
                buf.len() as u32,
                std::ptr::null_mut(),
            );
            if n > 0 {
                let bytes: Vec<u8> = buf[..n as usize].iter().map(|&c| c as u8).collect();
                String::from_utf8_lossy(&bytes).into_owned()
            } else {
                format!("system error {err}\n")
            }
        }
    }

    pub unsafe fn open(path: &CString) -> *mut c_void {
        LoadLibraryA(path.as_ptr())
    }
    pub unsafe fn sym(h: *mut c_void, name: &CString) -> *mut c_void {
        GetProcAddress(h, name.as_ptr())
    }
    pub unsafe fn close(h: *mut c_void) {
        FreeLibrary(h);
    }

    /// Directory of the running executable (PUC-Rio `setprogdir`).
    pub fn progdir() -> Option<String> {
        let mut buf = [0 as c_char; 260];
        let n = unsafe { GetModuleFileNameA(std::ptr::null_mut(), buf.as_mut_ptr(), buf.len() as u32) };
        if n == 0 || n as usize == buf.len() {
            return None;
        }
        let bytes: Vec<u8> = buf[..n as usize].iter().map(|&c| c as u8).collect();
        let s = String::from_utf8_lossy(&bytes).into_owned();
        s.rfind('\\').map(|i| s[..i].to_string())
    }
}

#[cfg(unix)]
mod imp {
    use super::*;

    const RTLD_NOW: i32 = 2;
    #[cfg_attr(target_os = "linux", link(name = "dl"))]
    unsafe extern "C" {
        fn dlopen(path: *const c_char, flags: i32) -> *mut c_void;
        fn dlsym(h: *mut c_void, name: *const c_char) -> *mut c_void;
        fn dlclose(h: *mut c_void) -> i32;
        fn dlerror() -> *const c_char;
    }

    pub fn last_error() -> String {
        unsafe {
            let e = dlerror();
            if e.is_null() {
                "unknown error".to_string()
            } else {
                std::ffi::CStr::from_ptr(e).to_string_lossy().into_owned()
            }
        }
    }
    pub unsafe fn open(path: &CString) -> *mut c_void {
        dlopen(path.as_ptr(), RTLD_NOW)
    }
    pub unsafe fn sym(h: *mut c_void, name: &CString) -> *mut c_void {
        dlsym(h, name.as_ptr())
    }
    pub unsafe fn close(h: *mut c_void) {
        dlclose(h);
    }
    #[allow(dead_code)]
    pub fn progdir() -> Option<String> {
        None
    }
}

#[allow(unused_imports)]
pub use imp::progdir;

impl Library {
    pub fn open(path: &str) -> Result<Library, String> {
        let c = CString::new(path).map_err(|_| "invalid library path".to_string())?;
        let h = unsafe { imp::open(&c) };
        if h.is_null() {
            Err(imp::last_error())
        } else {
            Ok(Library {
                path: path.to_string(),
                handle: h,
            })
        }
    }

    pub fn sym(&self, name: &str) -> Result<*mut c_void, String> {
        let c = CString::new(name).map_err(|_| "invalid symbol name".to_string())?;
        let p = unsafe { imp::sym(self.handle, &c) };
        if p.is_null() { Err(imp::last_error()) } else { Ok(p) }
    }
}

impl Drop for Library {
    fn drop(&mut self) {
        unsafe { imp::close(self.handle) };
    }
}

/// Writes a diagnostic line: `OutputDebugStringA` on Windows (visible in
/// Sysinternals DebugView), stderr elsewhere.
pub fn debug_out(msg: &str) {
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn OutputDebugStringA(s: *const c_char);
        }
        let mut b = msg.as_bytes().to_vec();
        b.retain(|&c| c != 0);
        b.extend_from_slice(b"\n\0");
        unsafe { OutputDebugStringA(b.as_ptr().cast()) };
    }
    #[cfg(not(windows))]
    eprintln!("{msg}");
}

/// Installs (once) a panic hook that reports internal Rust panics through
/// `debug_out` before the default hook runs.
pub fn install_panic_hook() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            debug_out(&format!("lua5.1 (rilua): INTERNAL PANIC: {info}"));
            prev(info);
        }));
    });
}

/// Flushes every C stdio stream (`fflush(NULL)`): output written by
/// `print` / `io.write` must not be lost if the host ends the process with
/// `ExitProcess` right after `lua_close`.
pub fn flush_all_c_streams() {
    unsafe extern "C" {
        fn fflush(f: *mut std::ffi::c_void) -> std::ffi::c_int;
    }
    unsafe { fflush(std::ptr::null_mut()) };
}
