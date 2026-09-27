#!/bin/sh
# Builds the tracing lua5.1.dll (MinGW-w64). It logs every Lua C API call
# (OutputDebugString / LUATRACE_FILE) and forwards it to luacore.dll, which is
# any real lua5.1.dll renamed. Run from this directory.
#   python3 gen_tracer.py ../../include   regenerates luatrace.c / luatrace.def
set -e
x86_64-w64-mingw32-gcc -O2 -shared -s -I../../include -o lua5.1.dll \
    luatrace.c internals.S luatrace.def -luser32
echo "Built lua5.1.dll (tracer). Put it next to luacore.dll + ../../proxy/lua51.dll"
