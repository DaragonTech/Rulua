/* Second differential test of the Lua 5.1 C API. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "lua.h"
#include "lauxlib.h"
#include "lualib.h"

#define SECTION(name) printf("\n== %s ==\n", name)

static void run(lua_State *L, const char *code) {
  if (luaL_loadstring(L, code) || lua_pcall(L, 0, LUA_MULTRET, 0)) {
    printf("ERROR: %s\n", lua_tostring(L, -1));
    lua_pop(L, 1);
  }
  lua_settop(L, 0);
}

typedef struct { char *data; size_t len; } Chunk;

static int chunk_writer(lua_State *L, const void *p, size_t sz, void *ud) {
  Chunk *c = (Chunk *)ud;
  (void)L;
  c->data = (char *)realloc(c->data, c->len + sz);
  memcpy(c->data + c->len, p, sz);
  c->len += sz;
  return 0;
}

typedef struct { const char *parts[4]; int i; } Pieces;
static const char *piece_reader(lua_State *L, void *ud, size_t *sz) {
  Pieces *p = (Pieces *)ud;
  (void)L;
  const char *s = p->parts[p->i];
  if (s == NULL) return NULL;
  p->i++;
  *sz = strlen(s);
  return s;
}

static int c_check(lua_State *L) {
  static const char *const opts[] = {"alpha", "beta", NULL};
  int o = luaL_checkoption(L, 1, "beta", opts);
  lua_Integer n = luaL_checkinteger(L, 2);
  size_t len;
  const char *s = luaL_optlstring(L, 3, "dflt", &len);
  luaL_argcheck(L, n >= 0, 2, "must be non-negative");
  lua_pushfstring(L, "opt=%d n=%d s=%s len=%d", o, (int)n, s, (int)len);
  return 1;
}

static int c_calltrace(lua_State *L) {
  luaL_checktype(L, 1, LUA_TFUNCTION);
  lua_call(L, lua_gettop(L) - 1, 1);
  return 1;
}

static int c_setlocal(lua_State *L) {
  lua_Debug ar;
  if (!lua_getstack(L, 1, &ar)) return 0;
  lua_pushstring(L, "changed");
  lua_pushstring(L, lua_setlocal(L, &ar, 1));
  return 1;
}

static int c_anyarg(lua_State *L) {
  luaL_checkany(L, 1);
  lua_pushstring(L, luaL_typename(L, 1));
  return 1;
}

static int counthook_n = 0;
static void counthook(lua_State *L, lua_Debug *ar) {
  (void)L;
  if (ar->event == LUA_HOOKCOUNT) counthook_n++;
}

static int linehook_err_armed = 0;
static void errhook(lua_State *L, lua_Debug *ar) {
  lua_getinfo(L, "Sl", ar);
  if (linehook_err_armed && ar->currentline == 3) {
    linehook_err_armed = 0;
    luaL_error(L, "hook error at line %d", ar->currentline);
  }
}

static int c_len_meta(lua_State *L) {
  lua_pushinteger(L, 99);
  return 1;
}

static int c_gc_err(lua_State *L) {
  return luaL_error(L, "error in gc");
}

static int panic_fn(lua_State *L) {
  printf("PANIC HANDLER: %s\n", lua_tostring(L, -1));
  fflush(stdout);
  return 0;
}

static const luaL_Reg funcs[] = {
  {"check", c_check},
  {"calltrace", c_calltrace},
  {"setlocal", c_setlocal},
  {"anyarg", c_anyarg},
  {NULL, NULL}
};

int main(int argc, char **argv) {
  lua_State *L = luaL_newstate();
  int st;
  luaL_openlibs(L);
  luaL_register(L, "t", funcs);
  lua_pop(L, 1);

  if (argc > 1 && strcmp(argv[1], "panic") == 0) {
    lua_atpanic(L, panic_fn);
    lua_pushstring(L, "unprotected!");
    lua_error(L);
    printf("not reached\n");
    return 0;
  }
  if (argc > 1 && strcmp(argv[1], "panic2") == 0) {
    lua_atpanic(L, panic_fn);
    luaL_loadstring(L, "local x = nil; return x.field");
    lua_call(L, 0, 0);
    printf("not reached\n");
    return 0;
  }

  SECTION("argument errors");
  run(L, "print(t.check('alpha', 3))");
  run(L, "print(t.check(nil, 4, 'xyz'))");
  run(L, "print(pcall(t.check, 'gamma', 1))");
  run(L, "print(pcall(t.check, 'alpha', 'x'))");
  run(L, "print(pcall(t.check, 'alpha', -1))");
  run(L, "print(pcall(function() return t.check('alpha', {}) end))");
  run(L, "local obj = {check = t.check} print(pcall(function() return obj:check(1) end))");
  run(L, "print(pcall(function() return t.anyarg() end))");
  run(L, "print(t.anyarg(nil))");
  run(L, "print(pcall(function() local s = ('x'):rep(3) return s:bad() end))");

  SECTION("tracebacks through C");
  run(L, "print(t.calltrace(function(a) return debug.traceback('tb:' .. a) end, 'z'))");
  run(L, "print(pcall(t.calltrace, function() error('from lua', 1) end))");
  run(L, "print(xpcall(function() t.calltrace(function() error('deep') end) end, debug.traceback))");
  run(L, "print(pcall(t.calltrace, t.calltrace, t.calltrace, error, 'nested'))");

  SECTION("dump/load");
  {
    Chunk c = {NULL, 0};
    Pieces p = {{"local a, b = ...\n", "return a .. b, ", "select('#', ...)", NULL}, 0};
    st = lua_load(L, piece_reader, &p, "=pieces");
    printf("load pieces st=%d\n", st);
    st = lua_dump(L, chunk_writer, &c);
    printf("dump st=%d size>0=%d sig=%d\n", st, c.len > 0, c.len > 4 && c.data[0] == 27 && memcmp(c.data + 1, "Lua", 3) == 0);
    lua_pop(L, 1);
    st = luaL_loadbuffer(L, c.data, c.len, "=bin");
    printf("load binary st=%d\n", st);
    lua_pushstring(L, "x"); lua_pushstring(L, "y"); lua_pushstring(L, "z");
    lua_call(L, 3, 2);
    printf("binary result: %s %s\n", lua_tostring(L, -2), lua_tostring(L, -1));
    lua_settop(L, 0);
    st = luaL_loadbuffer(L, c.data, c.len / 2, "=truncated");
    printf("truncated binary st=%d\n", st);
    lua_settop(L, 0);
    free(c.data);
    lua_pushinteger(L, 5);
    printf("dump non-function=%d\n", lua_dump(L, chunk_writer, &c));
    lua_settop(L, 0);
  }
  {
    FILE *f = fopen("tmp_chunk.lua", "w");
    fputs("#!/usr/bin/lua\nreturn 'from file', ...\n", f);
    fclose(f);
    st = luaL_loadfile(L, "tmp_chunk.lua");
    printf("loadfile st=%d\n", st);
    lua_pushstring(L, "arg1");
    lua_call(L, 1, 2);
    printf("file result: %s %s\n", lua_tostring(L, -2), lua_tostring(L, -1));
    lua_settop(L, 0);
    st = luaL_loadfile(L, "does_not_exist.lua");
    printf("loadfile missing st=%d msg=%s\n", st, lua_tostring(L, -1));
    lua_settop(L, 0);
    st = luaL_dofile(L, "tmp_chunk.lua");
    printf("dofile st=%d top=%d\n", st, lua_gettop(L));
    lua_settop(L, 0);
    remove("tmp_chunk.lua");
  }

  SECTION("debug");
  run(L, "local function f()\n local x = 'orig'\n local r = t.setlocal()\n return x, r\nend\nprint(f())");
  lua_sethook(L, counthook, LUA_MASKCOUNT, 10);
  run(L, "local s = 0 for i = 1, 1000 do s = s + i end");
  lua_sethook(L, NULL, 0, 0);
  printf("count hook fired: %s (count=%d)\n", counthook_n > 50 ? "yes" : "no", lua_gethookcount(L));
  linehook_err_armed = 1;
  lua_sethook(L, errhook, LUA_MASKLINE, 0);
  run(L, "local a = 1\nlocal b = 2\nlocal c = 3\nprint('not reached?')");
  lua_sethook(L, NULL, 0, 0);
  run(L, "print(debug.getinfo(1, 'S').what, debug.getinfo(t.check, 'S').what)");
  run(L, "local co = coroutine.create(function(x) local inside = x * 2; coroutine.yield(inside) end)"
         " coroutine.resume(co, 21) print(debug.getlocal(co, 1, 1), debug.getlocal(co, 1, 2))"
         " print(debug.traceback(co))");

  SECTION("metamethods on non-tables");
  lua_pushcfunction(L, c_len_meta);
  lua_setglobal(L, "lenmeta");
  run(L, "local u = newproxy(true) getmetatable(u).__len = lenmeta print(#u)");
  run(L, "local u = newproxy(true) getmetatable(u).__index = function(_, k) return k .. '?' end print(u.abc)");
  run(L, "print(pcall(function() return 1 < {} end))");
  run(L, "print(pcall(function() return {} .. 'x' end))");
  run(L, "print(pcall(function() return #5 end))");
  lua_pushnumber(L, 1);
  lua_newtable(L);
  st = lua_pcall(L, 0, 0, 0); /* calling a number */
  printf("call non-function st=%d msg=%s\n", st, lua_tostring(L, -1));
  lua_settop(L, 0);

  SECTION("coroutine errors");
  run(L, "local co = coroutine.create(function() t.calltrace(function() error('inside co via C') end) end)"
         " print(coroutine.resume(co)) print(coroutine.status(co))");
  run(L, "local co = coroutine.wrap(function() error({code = 7}) end) local ok, e = pcall(co) print(ok, type(e), e.code)");
  run(L, "local co co = coroutine.create(function() return coroutine.resume(co) end) print(coroutine.resume(co))");
  run(L, "print(coroutine.running(), coroutine.status(coroutine.create(print ~= nil and function() end)))");

  SECTION("gc finalizer error");
  lua_pushcfunction(L, c_gc_err);
  lua_setglobal(L, "gcerr");
  run(L, "do local u = newproxy(true) getmetatable(u).__gc = gcerr end print(pcall(collectgarbage))");

  SECTION("string edge cases");
  lua_pushlstring(L, "", 0);
  lua_pushlstring(L, NULL, 0);
  printf("empty strings: '%s' len=%d eq=%d\n", lua_tostring(L, -1), (int)lua_objlen(L, -1), lua_rawequal(L, -1, -2));
  lua_pushstring(L, NULL);
  printf("pushstring(NULL) -> %s\n", luaL_typename(L, -1));
  lua_settop(L, 0);
  lua_pushlstring(L, "a\0b\0c", 5);
  {
    size_t n; const char *s = lua_tolstring(L, -1, &n);
    printf("embedded zeros len=%d bytes=%d%d%d%d%d nul-terminated=%d\n", (int)n, s[0], s[1], s[2], s[3], s[4], s[5] == 0);
  }
  lua_setglobal(L, "zs");
  run(L, "print(#zs, zs:byte(1, -1))");
  run(L, "print(string.format('%q', zs))");

  SECTION("done");
  lua_close(L);
  printf("closed\n");
  return 0;
}
