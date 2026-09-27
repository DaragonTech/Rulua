#include "lua.h"
#include "lauxlib.h"
static int hello(lua_State *L) {
  lua_pushfstring(L, "hello, %s!", luaL_checkstring(L, 1));
  return 1;
}
static const luaL_Reg funcs[] = {{"hello", hello}, {NULL, NULL}};
int luaopen_cmod(lua_State *L) {
  luaL_register(L, "cmod", funcs);
  lua_pushstring(L, "1.0");
  lua_setfield(L, -2, "VERSION");
  return 1;
}
int luaopen_cmod_sub(lua_State *L) {
  lua_newtable(L);
  lua_pushstring(L, "sub-module");
  lua_setfield(L, -2, "name");
  return 1;
}
