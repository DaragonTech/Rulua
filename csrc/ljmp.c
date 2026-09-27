/*
** ljmp.c -- setjmp/longjmp error transport, used on 32-bit x86 only.
**
** On x64 and ARM64 a Lua error travels from lua_error to the nearest
** protected boundary as a Rust unwind: every function on those platforms has
** unwind tables, whoever compiled it. 32-bit x86 has no such guarantee
** (MSVC- or Delphi-built modules have no DWARF tables), so there the library
** works like PUC-Rio's ldo.c: every call into C code (C functions, hooks)
** goes through lua51rs_ccall / lua51rs_hcall, which set a jump target, and
** lua_error longjmps to the innermost one.
*/
#include <setjmp.h>
#include "lua.h"

struct lua51rs_jmp {
  jmp_buf b;
  struct lua51rs_jmp *prev;
};

/* Calls f(L). Returns 0 and stores f's result in *res, or returns 1 if a
   Lua error was thrown (the error itself is kept by the caller). */
int lua51rs_ccall (lua_CFunction f, lua_State *L, void **chain, int *res) {
  struct lua51rs_jmp j;
  j.prev = (struct lua51rs_jmp *)*chain;
  *chain = &j;
  if (setjmp(j.b) == 0) {
    int r = f(L);
    *chain = j.prev;
    *res = r;
    return 0;
  }
  *chain = j.prev;
  return 1;
}

int lua51rs_hcall (lua_Hook h, lua_State *L, lua_Debug *ar, void **chain) {
  struct lua51rs_jmp j;
  j.prev = (struct lua51rs_jmp *)*chain;
  *chain = &j;
  if (setjmp(j.b) == 0) {
    h(L, ar);
    *chain = j.prev;
    return 0;
  }
  *chain = j.prev;
  return 1;
}

void lua51rs_throw (void **chain) {
  longjmp(((struct lua51rs_jmp *)*chain)->b, 1);
}
