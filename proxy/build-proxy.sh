#!/bin/sh
# Builds lua51.dll: a tiny proxy whose exports jump into lua5.1.dll, for hosts
# and C modules linked against "lua51.dll" (LuaBinaries / Lua for Windows).
# Public API: direct import thunks. PUC-Rio internal functions (luaD_growstack
# & co, exported by LuaBinaries builds): resolved from lua5.1.dll at load
# time; if lua5.1.dll lacks one, calling it shows an error naming it.
#
#   ./build-proxy.sh                x64   (x86_64-w64-mingw32-gcc)
#   ARCH=x86 ./build-proxy.sh       Win32 (i686-w64-mingw32-gcc)
#   ARCH=arm64 CC="zig cc -target aarch64-windows-gnu" NOLIBC="-nodefaultlibs -nostartfiles" ./build-proxy.sh
# (NOLIBC: flags that drop the C runtime; zig's -nostdlib also drops the headers)
# LIB = import library of lua5.1.dll for that architecture
# (default ../lib/liblua5.1.dll.a, which is the x64 one). Output: $OUT (lua51.dll).
set -e
cd "$(dirname "$0")"
ARCH=${ARCH:-x64}
LIB=${LIB:-../lib/liblua5.1.dll.a}
OUT=${OUT:-lua51.dll}
case $ARCH in
  x64)   CC=${CC:-x86_64-w64-mingw32-gcc}; U="";  ENTRY=DllEntry ;;
  x86)   CC=${CC:-i686-w64-mingw32-gcc};   U="_"; ENTRY=_DllEntry@12 ;;
  arm64) CC=${CC:-aarch64-w64-mingw32-gcc}; U=""; ENTRY=DllEntry ;;
  *) echo "ARCH must be x64, x86 or arm64"; exit 1 ;;
esac
T=$(mktemp -d)
grep -E '^    ' lua51.def | tr -d ' ' > $T/syms.txt
{ echo '    .text'
  while read s; do
    printf '    .globl %spx_%s\n    .p2align 4\n%spx_%s:\n' "$U" $s "$U" $s
    case $ARCH in
      x64)   printf '    jmp *__imp_%s(%%rip)\n' $s ;;
      x86)   printf '    jmp *__imp__%s\n' $s ;;
      arm64) printf '    adrp x16, __imp_%s\n    ldr x16, [x16, :lo12:__imp_%s]\n    br x16\n' $s $s ;;
    esac
  done < $T/syms.txt
  while read s; do
    printf '    .globl %six_%s\n    .p2align 4\n%six_%s:\n' "$U" $s "$U" $s
    case $ARCH in
      x64)   printf '    jmp *pp_%s(%%rip)\n' $s ;;
      x86)   printf '    jmp *_pp_%s\n' $s ;;
      arm64) printf '    adrp x16, pp_%s\n    ldr x16, [x16, :lo12:pp_%s]\n    br x16\n' $s $s ;;
    esac
  done < internals.txt
  echo '    .data'
  while read s; do
    if [ $ARCH = x86 ]; then printf '    .globl _pp_%s\n    .p2align 2\n_pp_%s:\n    .long 0\n' $s $s
    else printf '    .globl pp_%s\n    .p2align 3\npp_%s:\n    .quad 0\n' $s $s; fi
  done < internals.txt
} > $T/stubs.S
{ echo '#include <windows.h>'
  echo 'static void missing(void) { MessageBoxA(NULL, "A module called a PUC-Rio internal Lua function that lua5.1.dll does not provide (see lua51.dll proxy).", "lua51.dll", MB_ICONERROR); ExitProcess(4); }'
  while read s; do echo "extern void *pp_$s;"; done < internals.txt
  echo 'BOOL WINAPI DllEntry(HINSTANCE h, DWORD r, LPVOID p) { (void)h; (void)p; if (r == DLL_PROCESS_ATTACH) { HMODULE m = GetModuleHandleA("lua5.1.dll"); void *f;'
  while read s; do echo "  f = m ? (void *)GetProcAddress(m, \"$s\") : NULL; pp_$s = f ? f : (void *)missing;"; done < internals.txt
  echo '} return TRUE; }'
} > $T/entry.c
{ echo 'LIBRARY "lua51.dll"'; echo EXPORTS; sed 's/.*/    & = px_&/' $T/syms.txt; sed 's/.*/    & = ix_&/' internals.txt; } > $T/lua51_build.def
$CC -O1 -shared ${NOLIBC:--nostdlib} -s -o "$OUT" $T/stubs.S $T/entry.c $T/lua51_build.def -Wl,--entry,$ENTRY "$LIB" -lkernel32 -luser32
rm -rf $T
echo "Built $OUT ($ARCH)"
