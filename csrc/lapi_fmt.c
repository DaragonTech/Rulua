/*
** lua_pushfstring / lua_pushvfstring for the rilua-backed Lua 5.1 C API.
**
** These take C varargs, which stable Rust cannot define, so they live in C.
** Logic follows luaO_pushvfstring (lobject.c, Lua 5.1.4) but is expressed
** purely through the public API (lua_pushstring / lua_pushnumber /
** lua_concat), which the Rust side implements.
*/
#include <stdarg.h>
#include <stdio.h>
#include <string.h>

#define LUA_CORE
#include "lua.h"

LUA_API const char *lua_pushvfstring (lua_State *L, const char *fmt,
                                      va_list argp) {
  int n = 1;
  lua_pushstring(L, "");
  for (;;) {
    const char *e = strchr(fmt, '%');
    if (e == NULL) break;
    lua_pushlstring(L, fmt, (size_t)(e - fmt));
    switch (*(e + 1)) {
      case 's': {
        const char *s = va_arg(argp, char *);
        if (s == NULL) s = "(null)";
        lua_pushstring(L, s);
        break;
      }
      case 'c': {
        char buff[2];
        buff[0] = (char)va_arg(argp, int);
        buff[1] = '\0';
        lua_pushstring(L, buff);
        break;
      }
      case 'd': {
        lua_pushnumber(L, (lua_Number)va_arg(argp, int));
        break;
      }
      case 'f': {
        lua_pushnumber(L, (lua_Number)va_arg(argp, LUAI_UACNUMBER));
        break;
      }
      case 'p': {
        char buff[4 * sizeof(void *) + 8]; /* should be enough space for a `%p' */
        sprintf(buff, "%p", va_arg(argp, void *));
        lua_pushstring(L, buff);
        break;
      }
      case '%': {
        lua_pushstring(L, "%");
        break;
      }
      default: {
        char buff[3];
        buff[0] = '%';
        buff[1] = *(e + 1);
        buff[2] = '\0';
        lua_pushstring(L, buff);
        break;
      }
    }
    n += 2;
    fmt = e + 2;
  }
  lua_pushstring(L, fmt);
  lua_concat(L, n + 1);
  return lua_tostring(L, -1);
}

LUA_API const char *lua_pushfstring (lua_State *L, const char *fmt, ...) {
  const char *msg;
  va_list argp;
  va_start(argp, fmt);
  msg = lua_pushvfstring(L, fmt, argp);
  va_end(argp);
  return msg;
}
