#!/bin/sh
# Builds the Win32 (32-bit x86) lua5.1.dll with MinGW-w64, plus import
# libraries and the lua51.dll proxy, into dist-win32/.
#   rustup target add i686-pc-windows-gnu      (Linux: apt install mingw-w64)
# Needs llvm-dlltool for the MSVC import library (lua5.1.lib).
# If your toolchain has no prebuilt std for the target, add:
#   RUSTC_BOOTSTRAP=1 ./build-windows-x86.sh -Zbuild-std=std,panic_unwind
# (then rustc also needs rsbegin.o/rsend.o in its i686-pc-windows-gnu lib dir,
#  compiled from library/rtstartup of the Rust sources).
# Lua errors use setjmp/longjmp on this target (csrc/ljmp.c), so modules
# built without DWARF unwind tables (MSVC, Delphi) can raise errors.
set -e
cd "$(dirname "$0")"
T=i686-pc-windows-gnu
cargo build --release --target $T "$@"
D=dist-win32
mkdir -p $D/include
cp target/$T/release/lua51.dll $D/lua5.1.dll
i686-w64-mingw32-dlltool -d lua5.1.def -l $D/liblua5.1.dll.a -D lua5.1.dll
llvm-dlltool -m i386 -d lua5.1.def -l $D/lua5.1.lib -D lua5.1.dll
i686-w64-mingw32-dlltool -d proxy/lua51.def -l $D/liblua51.dll.a -D lua51.dll
llvm-dlltool -m i386 -d proxy/lua51.def -l $D/lua51.lib -D lua51.dll
ARCH=x86 LIB=$PWD/$D/liblua5.1.dll.a OUT=$PWD/$D/lua51.dll proxy/build-proxy.sh
i686-w64-mingw32-gcc -O2 -s -DLUA_BUILD_AS_DLL -Iinclude -o $D/lua.exe test/lua.c $D/liblua5.1.dll.a -static-libgcc
cp lua5.1.def proxy/lua51.def $D/; cp include/* $D/include/
echo "Built $D/lua5.1.dll (Win32)"
