#!/bin/sh
# Builds liblua5.1.so (+ lua) linked against glibc 2.28, so it runs on any
# distribution from ~2018 on (Ubuntu 18.10+, Debian 10+, RHEL/Rocky 8+),
# whatever glibc the build machine has.
#   ./build-linux-portable.sh           x86_64  -> dist-linux/
#   ARCH=aarch64 ./build-linux-portable.sh      -> dist-linux-arm64/
# Needs zig (pip install ziglang, or zig on PATH; set ZIG=/path/to/zig) and
# the Rust target (rustup target add x86_64-unknown-linux-gnu /
# aarch64-unknown-linux-gnu). Extra cargo flags are passed through.
# For a plain build for the current machine only, `make` is enough.
set -e
cd "$(dirname "$0")"
ARCH=${ARCH:-x86_64}
case $ARCH in
  x86_64)  D=dist-linux ;;
  aarch64) D=dist-linux-arm64 ;;
  *) echo "ARCH must be x86_64 or aarch64"; exit 1 ;;
esac
T=$ARCH-unknown-linux-gnu
TU=$(echo $T | tr 'a-z-' 'A-Z_'); tl=$(echo $T | tr '-' '_')
export ZIG_ARCH=$ARCH
CC="$PWD/tools/zig/zigcc-linux"
export CC_$tl="$CC" AR_$tl="$PWD/tools/zig/zigar" CARGO_TARGET_${TU}_LINKER="$CC"
cargo build --release --target $T --target-dir target-portable "$@"
LIB=target-portable/$T/release/liblua51.a
mkdir -p $D
$CC -shared -s -o $D/liblua5.1.so -Wl,--whole-archive $LIB -Wl,--no-whole-archive \
    -Wl,--version-script=lua5.1.map -Wl,-soname,liblua5.1.so -lm -ldl -lpthread -lgcc_s
$CC -O2 -s -o $D/lua test/lua.c -Iinclude -L$D -llua5.1 -Wl,-rpath,'$ORIGIN' -Wl,-E
cp -r include $D/
echo "Built $D/liblua5.1.so ($ARCH, glibc >= 2.28)"
