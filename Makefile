# Linux / Unix: builds liblua5.1.so and a stock `lua` interpreter on top of it.
# (Cargo's own cdylib output cannot export the C-implemented luaL_* functions
# on ELF targets, so the shared object is linked here from the staticlib.)
CC ?= cc
LIBS = -lm -ldl -lpthread

all: dist/liblua5.1.so dist/lua

target/release/liblua51.a: FORCE
	cargo build --release

dist/liblua5.1.so: target/release/liblua51.a lua5.1.map
	mkdir -p dist
	$(CC) -shared -o $@ -Wl,--whole-archive $< -Wl,--no-whole-archive \
	  -Wl,--version-script=lua5.1.map -Wl,-soname,liblua5.1.so $(LIBS)

# Stock lua.c from Lua 5.1.4 (put lua.c in test/ or point LUA_C at it).
LUA_C ?= test/lua.c
dist/lua: dist/liblua5.1.so
	$(CC) -O2 -o $@ $(LUA_C) -Iinclude -Ldist -llua5.1 -Wl,-rpath,'$$ORIGIN' -Wl,-E

test: dist/lua
	cd test && ./run-tests.sh

clean:
	cargo clean
	rm -rf dist

FORCE:
.PHONY: all test clean FORCE
