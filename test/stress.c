/* GC / memory stress through the C API. Prints memory in KB periodically;
** it must stay bounded. */
#include <stdio.h>
#include <string.h>
#include "lua.h"
#include "lauxlib.h"
#include "lualib.h"

static int finalized = 0;
static int ud_gc(lua_State *L) { (void)L; finalized++; return 0; }
static int cfun(lua_State *L) {
  lua_pushvalue(L, lua_upvalueindex(1));
  lua_pushinteger(L, lua_gettop(L));
  return 2;
}
static int yielder(lua_State *L) { return lua_yield(L, lua_gettop(L)); }

int main(void) {
  lua_State *L = luaL_newstate();
  int round, i;
  luaL_openlibs(L);
  luaL_newmetatable(L, "Stress");
  lua_pushcfunction(L, ud_gc); lua_setfield(L, -2, "__gc");
  lua_pop(L, 1);
  lua_pushcfunction(L, yielder); lua_setglobal(L, "cyield");
  for (round = 0; round < 10; round++) {
    for (i = 0; i < 20000; i++) {
      char buf[64];
      /* userdata with finalizer */
      lua_newuserdata(L, 64 + (i % 100));
      luaL_getmetatable(L, "Stress");
      lua_setmetatable(L, -2);
      lua_pop(L, 1);
      /* unique strings */
      sprintf(buf, "string-%d-%d", round, i);
      lua_pushstring(L, buf);
      /* C closure with the string as upvalue, called from C */
      lua_pushcclosure(L, cfun, 1);
      lua_call(L, 0, 2);
      lua_pop(L, 2);
      /* tables */
      lua_createtable(L, 4, 4);
      lua_pushinteger(L, i); lua_rawseti(L, -2, 1);
      lua_pop(L, 1);
      /* coroutine created from C, resumed twice (Lua yield + C yield) */
      if (i % 20 == 0) {
        lua_State *co = lua_newthread(L);
        luaL_loadstring(co, "local x = coroutine.yield(1) cyield(x, 2) return 'end'");
        lua_resume(co, 0);
        lua_settop(co, 0);
        lua_pushinteger(co, i);
        lua_resume(co, 1);
        lua_settop(co, 0);
        lua_resume(co, 0);
        lua_pop(L, 1); /* drop thread */
      }
    }
    luaL_dostring(L, "local t = {} for i = 1, 2000 do t[i] = tostring(i) .. 'x' end");
    printf("round %d: mem=%dKB top=%d\n", round, lua_gc(L, LUA_GCCOUNT, 0), lua_gettop(L));
  }
  lua_gc(L, LUA_GCCOLLECT, 0);
  printf("after full gc: mem=%dKB finalized>=%d\n", lua_gc(L, LUA_GCCOUNT, 0), finalized >= 150000);
  lua_close(L);
  printf("finalized total=%d (expected 200000)\n", finalized);
  return 0;
}
