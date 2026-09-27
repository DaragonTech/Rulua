#!/bin/sh
# Builds liblua5.1.dylib (+ lua) for macOS 11+:
#   ./build-macos.sh                 Apple Silicon (arm64)  -> dist-macos/
#   ARCH=x86_64 ./build-macos.sh     Intel                  -> dist-macos-x86_64/
#
#  On a Mac:  Rust (rustup) + Xcode command line tools, then ./build-macos.sh
#  From Linux: rustup target add aarch64-apple-darwin x86_64-apple-darwin ;
#              pip install ziglang (or any zig >= 0.13 on PATH) ; ld64.lld
#              (LLVM lld), then  CROSS=1 [ARCH=x86_64] ./build-macos.sh
#              (extra cargo flags via CARGO_ARGS, e.g. -Zbuild-std=std,panic_unwind)
#              (if zig is not on PATH, set ZIG=/path/to/zig -- with pip's
#               ziglang it is the `zig` file inside the ziglang package dir)
set -e
cd "$(dirname "$0")"
ARCH=${ARCH:-arm64}
case $ARCH in
  arm64|aarch64) ZA=aarch64; MA=arm64;  D=dist-macos ;;
  x86_64)        ZA=x86_64;  MA=x86_64; D=dist-macos-x86_64 ;;
  *) echo "ARCH must be arm64 or x86_64"; exit 1 ;;
esac
TARGET=$ZA-apple-darwin
if [ -n "$CROSS" ]; then
  export ZIG_ARCH=$ZA
  CC="$PWD/tools/zig/zigcc-macos"
  tl=$(echo $TARGET | tr '-' '_'); TU=$(echo $TARGET | tr 'a-z-' 'A-Z_')
  export CC_$tl="$CC" AR_$tl="$PWD/tools/zig/zigar" CARGO_TARGET_${TU}_LINKER="$CC"
  # final links call zig directly: the wrapper drops -dead_strip for rustc
  CC="${ZIG:-zig} cc -target $ZA-macos.11.0"
else
  CC="${CC:-cc} -arch $MA -mmacosx-version-min=11.0"
fi
cargo build --release --target $TARGET $CARGO_ARGS
LIB=target/$TARGET/release/liblua51.a
mkdir -p $D
# Link the dylib ourselves: cargo's cdylib would hide the C luaL_* symbols.
if [ -n "$CROSS" ]; then
  # zig's Mach-O linker ignores -exported_symbols_list, so the dylib is
  # linked with LLVM's ld64.lld against zig's libSystem stub instead
  # (LD64_LLD=ld64.lld-18 etc. if the plain name is not on PATH).
  ZLIB=$(${ZIG:-zig} env | sed -n 's/.*\.lib_dir = "\(.*\)".*/\1/p')
  ${LD64_LLD:-ld64.lld} -arch $MA -platform_version macos 11.0 11.0 -dylib \
      -o $D/liblua5.1.dylib -force_load $LIB -dead_strip -x \
      -exported_symbols_list macos-exports.txt \
      -install_name @rpath/liblua5.1.dylib \
      -compatibility_version 1.0.0 -current_version 1.0.0 \
      -L"$ZLIB/libc/darwin" -lSystem
else
  $CC -dynamiclib -s -o $D/liblua5.1.dylib -Wl,-force_load,$LIB -Wl,-dead_strip \
      -Wl,-exported_symbols_list,macos-exports.txt \
      -Wl,-install_name,@rpath/liblua5.1.dylib
fi
$CC -O2 -o $D/lua test/lua.c -Iinclude -L$D -llua5.1 \
    -Wl,-rpath,@executable_path
cp -r include $D/
echo "Built $D/liblua5.1.dylib ($ARCH)"
