/* Differential test of the Lua 5.1 C API.
** Build against PUC-Rio liblua.a and against lua51 (rilua) and diff output. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "lua.h"
#include "lauxlib.h"
#include "lualib.h"

#define SECTION(name) printf("\n== %s ==\n", name)

static void dumpstack(lua_State *L, const char *label) {
  int i, n = lua_gettop(L);
  printf("[%s] top=%d:", label, n);
  for (i = 1; i <= n; i++) {
    int t = lua_type(L, i);
    switch (t) {
      case LUA_TNUMBER: printf(" %g", lua_tonumber(L, i)); break;
      case LUA_TSTRING: printf(" '%s'", lua_tostring(L, i)); break;
      case LUA_TBOOLEAN: printf(" %s", lua_toboolean(L, i) ? "true" : "false"); break;
      default: printf(" <%s>", lua_typename(L, t)); break;
    }
  }
  printf("\n");
}

static void run(lua_State *L, const char *code) {
  if (luaL_loadstring(L, code) || lua_pcall(L, 0, LUA_MULTRET, 0)) {
    printf("ERROR: %s\n", lua_tostring(L, -1));
    lua_pop(L, 1);
  }
}

/* ---- C functions exposed to Lua ---- */

static int c_add(lua_State *L) {
  lua_Number a = luaL_checknumber(L, 1);
  lua_Number b = luaL_optnumber(L, 2, 100);
  lua_pushnumber(L, a + b);
  lua_pushstring(L, "sum");
  return 2;
}

static int c_counter(lua_State *L) {
  int n = (int)lua_tointeger(L, lua_upvalueindex(1));
  n += (int)luaL_optinteger(L, 1, 1);
  lua_pushinteger(L, n);
  lua_replace(L, lua_upvalueindex(1));
  lua_pushinteger(L, n);
  lua_pushvalue(L, lua_upvalueindex(2));
  return 2;
}

static int c_fail(lua_State *L) {
  return luaL_error(L, "c_fail says %s %d", luaL_checkstring(L, 1), 42);
}

static int c_throw_table(lua_State *L) {
  lua_newtable(L);
  lua_pushstring(L, "custom");
  lua_setfield(L, -2, "kind");
  return lua_error(L);
}

static int c_callback(lua_State *L) {
  /* calls arg1(arg2, arg3) and returns its results plus a marker */
  int top;
  luaL_checktype(L, 1, LUA_TFUNCTION);
  lua_pushvalue(L, 1);
  lua_pushvalue(L, 2);
  lua_pushvalue(L, 3);
  lua_call(L, 2, LUA_MULTRET);
  top = lua_gettop(L);
  lua_pushstring(L, "after-call");
  return top - 3 + 1;
}

static int c_where(lua_State *L) {
  lua_Debug ar;
  luaL_where(L, 1);
  if (lua_getstack(L, 1, &ar)) {
    lua_getinfo(L, "nSl", &ar);
    lua_pushfstring(L, "caller: what=%s src=%s line=%d name=%s namewhat=%s",
                    ar.what, ar.short_src, ar.currentline,
                    ar.name ? ar.name : "(null)", ar.namewhat);
  } else {
    lua_pushstring(L, "no caller");
  }
  if (lua_getstack(L, 0, &ar)) {
    lua_getinfo(L, "nS", &ar);
    lua_pushfstring(L, "self: what=%s name=%s", ar.what, ar.name ? ar.name : "(null)");
  }
  return 3;
}

static int c_locals(lua_State *L) {
  lua_Debug ar;
  int i;
  const char *name;
  if (!lua_getstack(L, 1, &ar)) return 0;
  luaL_Buffer b;
  luaL_buffinit(L, &b);
  for (i = 1; (name = lua_getlocal(L, &ar, i)) != NULL; i++) {
    lua_pushfstring(L, "%s=%s;", name, luaL_typename(L, -1));
    luaL_addvalue(&b);
    lua_pop(L, 1);
  }
  luaL_pushresult(&b);
  return 1;
}

static int c_env(lua_State *L) {
  lua_getfield(L, LUA_ENVIRONINDEX, "envmark");
  lua_getfield(L, LUA_GLOBALSINDEX, "globmark");
  return 2;
}

static int c_yielder(lua_State *L) {
  int n = lua_gettop(L);
  lua_pushstring(L, "from-C");
  lua_insert(L, 1);
  return lua_yield(L, n + 1);
}

/* userdata type */
typedef struct { double x, y; } Point;

static int pt_new(lua_State *L) {
  Point *p = (Point *)lua_newuserdata(L, sizeof(Point));
  p->x = luaL_checknumber(L, 1);
  p->y = luaL_checknumber(L, 2);
  luaL_getmetatable(L, "Point");
  lua_setmetatable(L, -2);
  return 1;
}
static int pt_len(lua_State *L) {
  Point *p = (Point *)luaL_checkudata(L, 1, "Point");
  lua_pushnumber(L, p->x * p->x + p->y * p->y);
  return 1;
}
static int pt_tostring(lua_State *L) {
  Point *p = (Point *)luaL_checkudata(L, 1, "Point");
  lua_pushfstring(L, "Point(%f, %f)", p->x, p->y);
  return 1;
}
static int pt_gc(lua_State *L) {
  Point *p = (Point *)luaL_checkudata(L, 1, "Point");
  printf("gc Point(%g,%g)\n", p->x, p->y);
  return 0;
}
static int pt_add(lua_State *L) {
  Point *a = (Point *)luaL_checkudata(L, 1, "Point");
  Point *b = (Point *)luaL_checkudata(L, 2, "Point");
  Point *r = (Point *)lua_newuserdata(L, sizeof(Point));
  r->x = a->x + b->x; r->y = a->y + b->y;
  luaL_getmetatable(L, "Point");
  lua_setmetatable(L, -2);
  return 1;
}
static const luaL_Reg pt_methods[] = {
  {"len2", pt_len},
  {NULL, NULL}
};

static int cp_func(lua_State *L) {
  int *ud = (int *)lua_touserdata(L, 1);
  printf("cpcall got %d, top=%d\n", *ud, lua_gettop(L));
  if (*ud == 2) luaL_error(L, "cpcall error %d", *ud);
  return 0;
}

static int writer(lua_State *L, const void *p, size_t sz, void *ud) {
  luaL_Buffer *b = (luaL_Buffer *)ud;
  (void)L;
  luaL_addlstring(b, (const char *)p, sz);
  return 0;
}

static int hook_lines = 0, hook_calls = 0, hook_rets = 0;
static void hookf(lua_State *L, lua_Debug *ar) {
  (void)L;
  if (ar->event == LUA_HOOKLINE) hook_lines++;
  else if (ar->event == LUA_HOOKCALL) hook_calls++;
  else if (ar->event == LUA_HOOKRET || ar->event == LUA_HOOKTAILRET) hook_rets++;
}

static int panicked(lua_State *L) {
  printf("panic: %s\n", lua_tostring(L, -1));
  fflush(stdout);
  return 0;
}

static const luaL_Reg mylib[] = {
  {"add", c_add},
  {"fail", c_fail},
  {"throwt", c_throw_table},
  {"callback", c_callback},
  {"where", c_where},
  {"locals", c_locals},
  {"yielder", c_yielder},
  {NULL, NULL}
};

int main(int argc, char **argv) {
  lua_State *L = luaL_newstate();
  int i, st;
  (void)argc; (void)argv;
  luaL_openlibs(L);

  SECTION("stack");
  lua_pushinteger(L, 1);
  lua_pushstring(L, "two");
  lua_pushboolean(L, 1);
  lua_pushnil(L);
  lua_pushnumber(L, 5.5);
  dumpstack(L, "push");
  lua_insert(L, 2);
  dumpstack(L, "insert");
  lua_remove(L, -2);
  dumpstack(L, "remove");
  lua_pushvalue(L, 1);
  lua_replace(L, 3);
  dumpstack(L, "replace");
  lua_settop(L, 7);
  dumpstack(L, "settop7");
  lua_settop(L, -5);
  dumpstack(L, "settop-5");
  printf("checkstack=%d absidx type(-1)=%d type(10)=%d\n", lua_checkstack(L, 100),
         lua_type(L, -1), lua_type(L, 10));
  lua_settop(L, 0);

  SECTION("conversions");
  lua_pushstring(L, "10");
  lua_pushstring(L, " 0x1A ");
  lua_pushstring(L, "abc");
  lua_pushnumber(L, 3.25);
  printf("isnumber: %d %d %d %d\n", lua_isnumber(L, 1), lua_isnumber(L, 2),
         lua_isnumber(L, 3), lua_isnumber(L, 4));
  printf("tonumber: %g %g %g\n", lua_tonumber(L, 1), lua_tonumber(L, 2), lua_tonumber(L, 3));
  printf("tointeger: %ld\n", (long)lua_tointeger(L, 4));
  printf("isstring(4)=%d type before=%s\n", lua_isstring(L, 4), luaL_typename(L, 4));
  {
    size_t len;
    const char *s = lua_tolstring(L, 4, &len);
    printf("tolstring(3.25)='%s' len=%d type after=%s\n", s, (int)len, luaL_typename(L, 4));
  }
  lua_pushnumber(L, 1e15); printf("1e15 -> %s\n", lua_tostring(L, -1));
  lua_pushnumber(L, 0.1); printf("0.1 -> %s\n", lua_tostring(L, -1));
  lua_pushnumber(L, -0.0); printf("-0 -> %s\n", lua_tostring(L, -1));
  lua_pushlstring(L, "a\0b", 3); printf("objlen embedded=%d\n", (int)lua_objlen(L, -1));
  printf("toboolean nil=%d none=%d zero=%d\n", lua_toboolean(L, -100 + 99 - 1 + 1) , lua_toboolean(L, 50), lua_toboolean(L, 1));
  lua_settop(L, 0);
  printf("typename: %s %s %s\n", lua_typename(L, LUA_TNONE), lua_typename(L, LUA_TLIGHTUSERDATA), lua_typename(L, LUA_TTHREAD));

  SECTION("tables");
  lua_createtable(L, 4, 4);
  for (i = 1; i <= 5; i++) { lua_pushinteger(L, i * 10); lua_rawseti(L, -2, i); }
  lua_pushstring(L, "v"); lua_setfield(L, -2, "k");
  lua_pushstring(L, "key2"); lua_pushinteger(L, 77); lua_settable(L, -3);
  printf("objlen=%d\n", (int)lua_objlen(L, -1));
  lua_rawgeti(L, -1, 3); printf("t[3]=%d\n", (int)lua_tointeger(L, -1)); lua_pop(L, 1);
  lua_getfield(L, -1, "k"); printf("t.k=%s\n", lua_tostring(L, -1)); lua_pop(L, 1);
  lua_pushstring(L, "key2"); lua_rawget(L, -2); printf("t.key2=%d\n", (int)lua_tointeger(L, -1)); lua_pop(L, 1);
  {
    int count = 0; double sum = 0;
    lua_pushnil(L);
    while (lua_next(L, -2)) {
      count++;
      if (lua_type(L, -1) == LUA_TNUMBER) sum += lua_tonumber(L, -1);
      lua_pop(L, 1);
    }
    printf("next: count=%d sum=%g top=%d\n", count, sum, lua_gettop(L));
  }
  lua_setglobal(L, "T");
  run(L, "local s = {} for k,v in pairs(T) do s[#s+1]=tostring(k)..'='..tostring(v) end table.sort(s) print(table.concat(s, ','))");
  /* metatables */
  run(L, "MT = {__index=function(t,k) return 'idx:'..k end, __newindex=function(t,k,v) rawset(t,k,v*2) end,"
         "__eq=function() return true end, __lt=function(a,b) return rawget(a,'n') < rawget(b,'n') end}"
         "A = setmetatable({n=1}, MT) B = setmetatable({n=2}, MT)");
  lua_getglobal(L, "A");
  lua_getfield(L, -1, "zzz"); printf("A.zzz=%s\n", lua_tostring(L, -1)); lua_pop(L, 1);
  lua_pushinteger(L, 21); lua_setfield(L, -2, "w");
  lua_getfield(L, -1, "w"); printf("A.w=%d\n", (int)lua_tointeger(L, -1)); lua_pop(L, 1);
  lua_getglobal(L, "B");
  printf("equal=%d rawequal=%d lessthan=%d lessthan(rev)=%d\n", lua_equal(L, -1, -2),
         lua_rawequal(L, -1, -2), lua_lessthan(L, -2, -1), lua_lessthan(L, -1, -2));
  printf("getmetatable=%d ", lua_getmetatable(L, -1));
  lua_getglobal(L, "MT");
  printf("same mt=%d\n", lua_rawequal(L, -1, -2));
  lua_settop(L, 0);
  /* string metatable via gettable on a string value */
  lua_pushstring(L, "hello");
  lua_getfield(L, -1, "upper");
  lua_pushvalue(L, -2);
  lua_call(L, 1, 1);
  printf("('hello'):upper() via API = %s\n", lua_tostring(L, -1));
  lua_settop(L, 0);
  /* indexing a nil inside pcall */
  run(L, "local t = nil; return t.x");

  SECTION("C functions");
  luaL_register(L, "mylib", mylib);
  lua_pop(L, 1);
  run(L, "print(mylib.add(1, 2)) print(mylib.add(5)) print(package.loaded.mylib == mylib)");
  run(L, "print(pcall(mylib.add, 'x'))");
  run(L, "print(pcall(mylib.add))");
  run(L, "local o = {add=mylib.add} print(pcall(o.add, o))");
  run(L, "print(pcall(mylib.fail, 'hi'))");
  run(L, "local ok, e = pcall(mylib.throwt) print(ok, type(e), e.kind)");
  run(L, "print(mylib.callback(function(a,b) return a+b, a*b end, 3, 4))");
  run(L, "print(pcall(mylib.callback, function() error('inner') end))");
  run(L, "print(pcall(mylib.callback, function() error({1,2,3}) end))");
  run(L, "local function outer() return mylib.where() end\nprint(outer())");
  run(L, "local function f(a, b)\n local c = 3\n return mylib.locals()\nend\nprint(f(1, 'x'))");
  lua_pushinteger(L, 0);
  lua_pushstring(L, "tag");
  lua_pushcclosure(L, c_counter, 2);
  lua_setglobal(L, "counter");
  run(L, "print(counter()) print(counter(5)) print(counter())");
  lua_getglobal(L, "counter");
  printf("iscfunction=%d tocfunction ok=%d\n", lua_iscfunction(L, -1), lua_tocfunction(L, -1) == c_counter);
  printf("getupvalue(1)=%s->", lua_getupvalue(L, -1, 1)); printf("%d\n", (int)lua_tointeger(L, -1)); lua_pop(L, 1);
  lua_pushinteger(L, 100); lua_setupvalue(L, -2, 1);
  lua_pop(L, 1);
  run(L, "print(counter())");

  SECTION("userdata");
  luaL_newmetatable(L, "Point");
  lua_pushcfunction(L, pt_tostring); lua_setfield(L, -2, "__tostring");
  lua_pushcfunction(L, pt_gc); lua_setfield(L, -2, "__gc");
  lua_pushcfunction(L, pt_add); lua_setfield(L, -2, "__add");
  lua_newtable(L); luaL_register(L, NULL, pt_methods); lua_setfield(L, -2, "__index");
  lua_pop(L, 1);
  lua_register(L, "Point", pt_new);
  run(L, "local p = Point(3, 4) print(p:len2(), tostring(p), type(p)) local q = p + Point(1,1) print(tostring(q))");
  run(L, "print(pcall(Point, 'a', 1))");
  run(L, "local p = Point(1,2) print(pcall(p.len2, {}))");
  lua_gc(L, LUA_GCCOLLECT, 0);
  printf("after collect\n");
  {
    Point *p = (Point *)lua_newuserdata(L, sizeof(Point));
    p->x = 7; p->y = 8;
    printf("objlen(ud)=%d same ptr=%d\n", (int)lua_objlen(L, -1), lua_touserdata(L, -1) == (void *)p);
    luaL_getmetatable(L, "Point");
    lua_setmetatable(L, -2);
    lua_setglobal(L, "keep");
    lua_pushlightuserdata(L, p);
    printf("light type=%s isuserdata=%d same=%d\n", luaL_typename(L, -1), lua_isuserdata(L, -1), lua_touserdata(L, -1) == (void *)p);
    lua_pop(L, 1);
  }

  SECTION("errors");
  run(L, "error('plain')");
  run(L, "error('lvl2', 2)");
  run(L, "error(setmetatable({}, {__tostring=function() return 'tbl' end}))");
  lua_pushcfunction(L, c_fail);
  lua_pushstring(L, "direct");
  st = lua_pcall(L, 1, 0, 0);
  printf("pcall status=%d msg=%s\n", st, lua_tostring(L, -1));
  lua_pop(L, 1);
  lua_getglobal(L, "debug"); lua_getfield(L, -1, "traceback"); lua_remove(L, -2);
  luaL_loadstring(L, "local function lvl3() error('deep') end\nlocal function lvl2() lvl3() end\nlvl2()");
  st = lua_pcall(L, 0, 0, 1);
  printf("pcall+traceback status=%d\n%s\n", st, lua_tostring(L, -1));
  lua_settop(L, 0);
  lua_pushcfunction(L, c_fail); /* handler that itself errors */
  luaL_loadstring(L, "error('x')");
  st = lua_pcall(L, 0, 0, 1);
  printf("errhandler error status=%d msg=%s\n", st, lua_tostring(L, -1));
  lua_settop(L, 0);
  st = luaL_loadstring(L, "x = = 1");
  printf("syntax status=%d msg=%s\n", st, lua_tostring(L, -1));
  lua_pop(L, 1);
  {
    int v = 1;
    st = lua_cpcall(L, cp_func, &v);
    printf("cpcall status=%d top=%d\n", st, lua_gettop(L));
    v = 2;
    st = lua_cpcall(L, cp_func, &v);
    printf("cpcall status=%d msg=%s\n", st, lua_tostring(L, -1));
    lua_pop(L, 1);
  }
  run(L, "print(select('#', pcall(error)))");
  run(L, "local t = setmetatable({}, {__index = function(t, k) error('meta '..k) end}) print(pcall(function() return t.x end))");
  run(L, "local function r() return r() end print('tailcall ok')");
  run(L, "local function rec(n) if n == 0 then return 0 end return 1 + rec(n-1) end print(pcall(rec, 150))");
  run(L, "local function inf() return 1 + inf() end local ok, e = pcall(inf) print(ok, e)");

  SECTION("load/dump");
  luaL_loadbuffer(L, "local a, b = ... return a * b, 'mul'", 36, "=mulchunk");
  {
    luaL_Buffer b;
    luaL_buffinit(L, &b);
    lua_pushvalue(L, -2);
    /* dump the function below the buffer placeholder */
    lua_dump(L, writer, &b);
    lua_pop(L, 1);
    luaL_pushresult(&b);
    {
      size_t n; const char *bc = lua_tolstring(L, -1, &n);
      printf("dump size>0=%d sig=%d\n", n > 0, bc[0] == 27 && bc[1] == 'L');
      st = luaL_loadbuffer(L, bc, n, "=binchunk");
      printf("reload status=%d\n", st);
      lua_pushinteger(L, 6); lua_pushinteger(L, 7);
      lua_call(L, 2, 2);
      printf("binary call -> %s %s\n", lua_tostring(L, -2), lua_tostring(L, -1));
    }
  }
  lua_settop(L, 0);
  run(L, "local f = loadstring(string.dump(function(x) return x..'!' end)) print(f('dumped'))");

  SECTION("buffer/gsub/findtable/ref");
  {
    luaL_Buffer b;
    luaL_buffinit(L, &b);
    for (i = 0; i < 3000; i++) luaL_addchar(&b, 'a' + (i % 26));
    luaL_addstring(&b, "-end");
    luaL_pushresult(&b);
    printf("buffer len=%d tail=%s\n", (int)lua_objlen(L, -1), lua_tostring(L, -1) + 2995);
    lua_pop(L, 1);
    printf("gsub=%s\n", luaL_gsub(L, "a.b.c", ".", "::"));
    lua_pop(L, 1);
    printf("findtable=%s\n", luaL_findtable(L, LUA_GLOBALSINDEX, "x.y.z", 1) ? "fail" : "ok");
    lua_pushinteger(L, 9); lua_setfield(L, -2, "w"); lua_pop(L, 1);
    run(L, "print(x.y.z.w)");
    lua_pushstring(L, "refd");
    {
      int r1 = luaL_ref(L, LUA_REGISTRYINDEX);
      int r2;
      lua_pushnil(L);
      r2 = luaL_ref(L, LUA_REGISTRYINDEX);
      lua_rawgeti(L, LUA_REGISTRYINDEX, r1);
      printf("ref ok=%d val=%s nilref=%d\n", r1 > 0, lua_tostring(L, -1), r2);
      lua_pop(L, 1);
      luaL_unref(L, LUA_REGISTRYINDEX, r1);
      lua_pushstring(L, "again");
      printf("ref reuse=%d\n", luaL_ref(L, LUA_REGISTRYINDEX) == r1);
    }
    printf("pushfstring: %s\n", lua_pushfstring(L, "%s|%d|%f|%c|%%|%5", "s", -12, 2.5, 'Z'));
    lua_pop(L, 1);
    lua_pushnumber(L, 12); lua_pushstring(L, "x"); lua_pushinteger(L, 3);
    lua_concat(L, 3);
    printf("concat=%s top=%d\n", lua_tostring(L, -1), lua_gettop(L));
    lua_concat(L, 0);
    printf("concat0='%s'\n", lua_tostring(L, -1));
    lua_settop(L, 0);
    printf("checkoption: ");
    run(L, "print(pcall(string.rep))");
  }

  SECTION("environments");
  lua_newtable(L);
  lua_pushstring(L, "env-value"); lua_setfield(L, -2, "envmark");
  lua_pushstring(L, "glob-value"); lua_setglobal(L, "globmark");
  lua_pushcfunction(L, c_env);
  lua_pushvalue(L, -2);
  printf("setfenv=%d\n", lua_setfenv(L, -2));
  lua_call(L, 0, 2);
  printf("env: %s %s\n", lua_tostring(L, -2), lua_tostring(L, -1));
  lua_settop(L, 0);
  luaL_loadstring(L, "return who");
  lua_newtable(L); lua_pushstring(L, "sandbox"); lua_setfield(L, -2, "who");
  lua_setfenv(L, -2);
  lua_call(L, 0, 1);
  printf("sandboxed chunk -> %s\n", lua_tostring(L, -1));
  lua_getfenv(L, -1); printf("getfenv(string)=%s\n", luaL_typename(L, -1));
  lua_settop(L, 0);
  lua_getglobal(L, "print");
  lua_getfenv(L, -1);
  lua_pushvalue(L, LUA_GLOBALSINDEX);
  printf("print env is _G=%d\n", lua_rawequal(L, -1, -2));
  lua_settop(L, 0);

  SECTION("coroutines");
  {
    lua_State *co = lua_newthread(L);
    printf("newthread type=%s status=%d top(co)=%d\n", luaL_typename(L, -1), lua_status(co), lua_gettop(co));
    luaL_loadstring(co, "local a, b = ...\nlocal x = coroutine.yield(a + b)\nlocal y, z = coroutine.yield(x * 2)\nreturn 'done', y, z");
    lua_pushinteger(co, 3); lua_pushinteger(co, 4);
    st = lua_resume(co, 2);
    printf("resume1 st=%d n=%d v=%s status=%d\n", st, lua_gettop(co), lua_tostring(co, -1), lua_status(co));
    lua_pop(co, lua_gettop(co));
    lua_pushinteger(co, 10);
    st = lua_resume(co, 1);
    printf("resume2 st=%d n=%d v=%s\n", st, lua_gettop(co), lua_tostring(co, -1));
    lua_pop(co, lua_gettop(co));
    lua_pushstring(co, "p"); lua_pushstring(co, "q");
    st = lua_resume(co, 2);
    printf("resume3 st=%d n=%d:", st, lua_gettop(co));
    for (i = 1; i <= lua_gettop(co); i++) printf(" %s", lua_tostring(co, i));
    printf(" status=%d\n", lua_status(co));
    lua_xmove(co, L, 2);
    printf("xmove -> L top=%d co top=%d [%s %s]\n", lua_gettop(L), lua_gettop(co), lua_tostring(L, -2), lua_tostring(L, -1));
    st = lua_resume(co, 0);
    printf("resume dead st=%d msg=%s\n", st, lua_tostring(co, -1));
    lua_settop(L, 0);
  }
  {
    lua_State *co = lua_newthread(L);
    luaL_loadstring(co, "local r = {mylib.yielder(1, 2)}\nreturn 'back', unpack(r)");
    st = lua_resume(co, 0);
    printf("C-yield st=%d n=%d:", st, lua_gettop(co));
    for (i = 1; i <= lua_gettop(co); i++) printf(" %s", lua_tostring(co, i));
    printf("\n");
    lua_settop(co, 0);
    lua_pushstring(co, "r1"); lua_pushstring(co, "r2");
    st = lua_resume(co, 2);
    printf("C-yield resumed st=%d n=%d:", st, lua_gettop(co));
    for (i = 1; i <= lua_gettop(co); i++) printf(" %s", lua_tostring(co, i));
    printf("\n");
    lua_settop(L, 0);
  }
  {
    lua_State *co = lua_newthread(L);
    luaL_loadstring(co, "error('in coroutine')");
    st = lua_resume(co, 0);
    printf("co error st=%d msg=%s\n", st, lua_tostring(co, -1));
    lua_settop(L, 0);
  }
  {
    /* use a fresh thread as an independent stack for pcall */
    lua_State *t2 = lua_newthread(L);
    luaL_loadstring(t2, "return 1 + 1, ...");
    lua_pushstring(t2, "arg");
    st = lua_pcall(t2, 1, LUA_MULTRET, 0);
    printf("pcall on thread st=%d top=%d [%s %s]\n", st, lua_gettop(t2), lua_tostring(t2, 1), lua_tostring(t2, 2));
    lua_settop(L, 0);
  }
  run(L, "local co = coroutine.wrap(function(...) local s = 0 for i, v in ipairs({...}) do s = s + coroutine.yield(v) end return s end)"
         " print(co(1, 2, 3), co(10), co(20), co(30))");
  run(L, "print(coroutine.resume(coroutine.create(function() return mylib.yielder(9) end)))");
  run(L, "print(pcall(mylib.yielder, 1))");
  run(L, "print(coroutine.resume(coroutine.create(function() return pcall(coroutine.yield, 1) end)))");
  printf("pushthread(main)=%d ", lua_pushthread(L));
  printf("tothread==L %d\n", lua_tothread(L, -1) == L);
  lua_settop(L, 0);

  SECTION("debug/hooks");
  lua_sethook(L, hookf, LUA_MASKLINE | LUA_MASKCALL | LUA_MASKRET, 0);
  printf("gethook ok=%d mask=%d\n", lua_gethook(L) == hookf, lua_gethookmask(L));
  run(L, "local function f(x)\n  return x + 1\nend\nlocal s = 0\nfor i = 1, 3 do\n  s = s + f(i)\nend");
  lua_sethook(L, NULL, 0, 0);
  printf("hooks: lines=%d calls=%d rets=%d gethook null=%d\n", hook_lines, hook_calls, hook_rets, lua_gethook(L) == NULL);
  run(L, "local function f() local lv = 5 return debug.getinfo(1, 'nSl') end local i = f() print(i.what, i.short_src, i.currentline, i.name, i.namewhat)");
  run(L, "print(debug.getinfo(print).what, debug.getinfo(mylib.add, 'S').source)");
  {
    lua_Debug ar;
    luaL_loadstring(L, "local up = 5; return function() return up end");
    lua_call(L, 0, 1);
    lua_pushvalue(L, -1);
    lua_getinfo(L, ">Su", &ar);
    printf("getinfo '>': what=%s nups=%d linedefined=%d src=%s\n", ar.what, ar.nups, ar.linedefined, ar.short_src);
    printf("getupvalue name=%s ", lua_getupvalue(L, -1, 1));
    printf("val=%d\n", (int)lua_tointeger(L, -1));
    lua_pop(L, 1);
    lua_pushinteger(L, 55);
    printf("setupvalue=%s ", lua_setupvalue(L, -2, 1));
    lua_call(L, 0, 1);
    printf("-> %d\n", (int)lua_tointeger(L, -1));
    printf("getstack at top level=%d\n", lua_getstack(L, 0, &ar));
    lua_settop(L, 0);
  }

  SECTION("gc");
  printf("gc count>0=%d\n", lua_gc(L, LUA_GCCOUNT, 0) > 0);
  lua_gc(L, LUA_GCSTOP, 0);
  lua_gc(L, LUA_GCRESTART, 0);
  printf("setpause old=%d\n", lua_gc(L, LUA_GCSETPAUSE, 150));
  printf("setstepmul old=%d\n", lua_gc(L, LUA_GCSETSTEPMUL, 300));
  run(L, "local t = {} for i = 1, 20000 do t[i] = {i, tostring(i)} end t = nil collectgarbage() print('gc stress ok')");
  lua_gc(L, LUA_GCSTEP, 100);
  printf("gc step ok\n");

  SECTION("require C module");
  run(L, "package.cpath = './?.so;' .. package.cpath local m = require('cmod') print(m.hello('world'), m.VERSION)");
  run(L, "local m = require('cmod.sub') print(m.name)");
  run(L, "local f, e, w = package.loadlib('./cmod.so', 'luaopen_cmod') print(type(f), e, w)");
  run(L, "local f, e, w = package.loadlib('./cmod.so', 'nope') print(f, e ~= nil, w)");
  run(L, "local f, e, w = package.loadlib('./missing.so', 'x') print(f, e ~= nil, w)");
  run(L, "print(pcall(require, 'no_such_mod'))");

  SECTION("close");
  lua_atpanic(L, panicked);
  lua_close(L);
  printf("closed\n");
  return 0;
}
