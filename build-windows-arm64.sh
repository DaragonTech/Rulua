#!/bin/sh
# Builds the Windows ARM64 lua5.1.dll, import libraries, the lua51.dll proxy
# and lua.exe into dist-winarm64/, cross-compiling with zig (clang + lld,
# LLVM mingw ABI, Universal CRT):
#   rustup target add aarch64-pc-windows-gnullvm ; pip install ziglang
#   ZIG=/path/to/zig ./build-windows-arm64.sh
# Needs llvm-dlltool for the import libraries. If your toolchain has no
# prebuilt std for the target: RUSTC_BOOTSTRAP=1 ... -Zbuild-std=std,panic_unwind
set -e
cd "$(dirname "$0")"
T=aarch64-pc-windows-gnullvm
Z="$PWD/tools/zig/zigcc-winarm64"
export CC_aarch64_pc_windows_gnullvm="$Z" AR_aarch64_pc_windows_gnullvm="$PWD/tools/zig/zigar"
export CARGO_TARGET_AARCH64_PC_WINDOWS_GNULLVM_LINKER="$Z"
cargo build --release --target $T "$@"
ZC="${ZIG:-zig} cc -target aarch64-windows-gnu"
D=dist-winarm64
mkdir -p $D/include
cp target/$T/release/lua51.dll $D/lua5.1.dll
llvm-dlltool -m arm64 -d lua5.1.def -l $D/liblua5.1.dll.a -D lua5.1.dll
cp $D/liblua5.1.dll.a $D/lua5.1.lib
llvm-dlltool -m arm64 -d proxy/lua51.def -l $D/liblua51.dll.a -D lua51.dll
cp $D/liblua51.dll.a $D/lua51.lib
ARCH=arm64 CC="$ZC" NOLIBC="-nodefaultlibs -nostartfiles" LIB=$PWD/$D/liblua5.1.dll.a OUT=$PWD/$D/lua51.dll proxy/build-proxy.sh
$ZC -O2 -s -DLUA_BUILD_AS_DLL -Iinclude -o $D/lua.exe test/lua.c $D/liblua5.1.dll.a
rm -f $D/*.pdb $D/lua.lib $D/stubs.lib proxy/stubs.lib
cp lua5.1.def proxy/lua51.def $D/; cp include/* $D/include/
echo "Built $D/lua5.1.dll (Windows ARM64)"
