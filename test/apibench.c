#include <stdio.h>
#include <time.h>
#include "lua.h"
#include "lauxlib.h"
#include "lualib.h"
static int add(lua_State *L) { lua_pushnumber(L, lua_tonumber(L, 1) + lua_tonumber(L, 2)); return 1; }
int main(void) {
  lua_State *L = luaL_newstate(); int i; clock_t t0, t1, t2;
  luaL_openlibs(L);
  lua_register(L, "cadd", add);
  luaL_dostring(L, "function ladd(a, b) return a + b end");
  t0 = clock();
  for (i = 0; i < 1000000; i++) {           /* C -> Lua calls */
    lua_getglobal(L, "ladd"); lua_pushinteger(L, i); lua_pushinteger(L, 1);
    lua_call(L, 2, 1); lua_pop(L, 1);
  }
  t1 = clock();
  luaL_dostring(L, "local s = 0 for i = 1, 1000000 do s = cadd(s, i) end");  /* Lua -> C calls */
  t2 = clock();
  printf("C->Lua 1M calls %.2fs   Lua->C 1M calls %.2fs\n", (double)(t1-t0)/CLOCKS_PER_SEC, (double)(t2-t1)/CLOCKS_PER_SEC);
  lua_close(L); return 0;
}
