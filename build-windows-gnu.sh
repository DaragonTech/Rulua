#!/bin/sh
# Builds lua5.1.dll with the MinGW-w64 toolchain (x86_64-pc-windows-gnu).
# Works on Windows (MSYS2) or as a cross build from Linux.
#   rustup target add x86_64-pc-windows-gnu
#   (Linux: apt install mingw-w64)
# If your toolchain has no prebuilt std for the target, add:
#   RUSTC_BOOTSTRAP=1 ... -Zbuild-std=std,panic_unwind   (needs rust-src)
set -e
cargo build --release --target x86_64-pc-windows-gnu "$@"
mkdir -p dist/include
cp target/x86_64-pc-windows-gnu/release/lua51.dll dist/lua5.1.dll
cp lib/lua5.1.lib lib/liblua5.1.dll.a lua5.1.def dist/
cp include/*.h dist/include/
echo "Built dist/lua5.1.dll"
