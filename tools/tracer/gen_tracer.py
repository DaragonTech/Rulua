#!/usr/bin/env python3
"""Generates luatrace.c + luatrace.def: a lua5.1.dll that logs every Lua C API
call (OutputDebugStringA, optional file) and forwards it to luacore.dll."""
import re, sys

inc = sys.argv[1] if len(sys.argv) > 1 else 'include'
src = ''.join(open(f'{inc}/{h}').read() for h in ('lua.h', 'lauxlib.h', 'lualib.h'))
src = re.sub(r'/\*.*?\*/', '', src, flags=re.S)
src = re.sub(r'#if defined\(LUA_COMPAT_GETN\).*?#endif', '', src, flags=re.S)

protos = []
seen = set()
for m in re.finditer(r'(?:LUA_API|LUALIB_API)\s+([^;]*?);', src, flags=re.S):
    decl = ' '.join(m.group(1).split())
    mm = re.match(r'^(.*?)\(\s*(\w+)\s*\)\s*\((.*)\)$', decl) or re.match(r'^(.*?)\b(\w+)\s*\((.*)\)$', decl)
    if not mm:
        continue
    ret, name, args = mm.group(1).strip(), mm.group(2), mm.group(3).strip()
    if not re.match(r'^(lua_|luaL_|luaI_|luaopen_)', name) or name in seen:
        continue
    seen.add(name)
    if name == 'luaI_openlib':
        name = 'luaL_openlib'
    protos.append((ret, name, args))

def parse_args(args):
    if args.strip() in ('void', ''):
        return []
    out = []
    for a in args.split(','):
        a = a.strip()
        if a == '...':
            out.append(('...', '...'))
            continue
        m = re.match(r'(.*?)(\w+)\s*(\[\])?$', a)
        typ, nm = m.group(1).strip(), m.group(2)
        if m.group(3):
            typ += ' *'
        out.append((typ, nm))
    return out

INDEX_NAMES = {'idx', 'idx1', 'idx2', 'objindex', 'obj', 'ud', 'narg', 'numArg', 'nArg', 'numarg', 'funcindex'}
STATUS_FUNCS = {'lua_pcall', 'lua_cpcall', 'lua_resume', 'lua_load', 'luaL_loadfile', 'luaL_loadbuffer', 'luaL_loadstring'}

def is_index(fn, nm):
    if fn in ('lua_settop', 'lua_resume'):
        return False
    return nm in INDEX_NAMES or (fn in ('luaL_ref', 'luaL_unref') and nm == 't')

def fmt_arg(fn, typ, nm):
    """C statements appending `nm=...` to buf."""
    t = typ.replace(' ', '')
    if t == 'lua_State*':
        return f'P("{nm}=%p", (void*){nm});'
    if t == 'int' and is_index(fn, nm):
        return f'P("{nm}="); IDX(L, {nm});'
    if t == 'int':
        return f'P("{nm}=%d", {nm});'
    if t == 'lua_Number':
        return f'P("{nm}=%.14g", {nm});'
    if t == 'lua_Integer':
        return f'P("{nm}=%lld", (long long){nm});'
    if t == 'size_t':
        return f'P("{nm}=%llu", (unsigned long long){nm});'
    if t in ('constchar*',) :
        return f'P("{nm}="); QS({nm}, (size_t)-1);'
    if t == 'constluaL_Reg*':
        return f'P("{nm}="); REGS({nm});'
    if t == 'constchar*const*':
        return f'P("{nm}="); OPTS({nm});'
    if t == 'va_list':
        return f'P("{nm}=<va_list>");'
    return f'P("{nm}=%p", (void*){nm});'

def fmt_ret(fn, ret):
    t = ret.replace(' ', '')
    if fn in STATUS_FUNCS:
        return 'P(" = %d (%s)", rv, SN(rv)); if (rv != 0 && rv != 1) { P(" err="); IDX(L, -1); }'
    if t == 'int':
        return 'P(" = %d", rv);'
    if t == 'lua_Number':
        return 'P(" = %.14g", rv);'
    if t == 'lua_Integer':
        return 'P(" = %lld", (long long)rv);'
    if t == 'size_t':
        return 'P(" = %llu", (unsigned long long)rv);'
    if t in ('constchar*', 'char*'):
        return 'P(" = "); QS(rv, (size_t)-1);'
    return 'P(" = %p", (void*)rv);'

HAND = {'lua_pushfstring', 'luaL_error', 'lua_pushvfstring', 'lua_error'}
BUF_FUNCS = {'luaL_prepbuffer', 'luaL_addlstring', 'luaL_addstring', 'luaL_addvalue', 'luaL_pushresult'}

out = []
ptrs = []
w = out.append
for ret, fn, args in protos:
    pa = parse_args(args)
    ptrs.append(fn)
    if fn in HAND:
        continue
    params = ', '.join(f'{t} {n}' if not t.endswith('*') else f'{t}{n}' for t, n in pa) or 'void'
    params = params.replace('const char *const *lst', 'const char *const lst[]')
    callargs = ', '.join(n for t, n in pa)
    has_L = any(n == 'L' and 'lua_State' in t for t, n in pa)
    is_buf = fn in BUF_FUNCS or fn == 'luaL_buffinit'
    w(f'{ret} tr_{fn}({params}) {{')
    w('  char buf[TRBUF]; int bn = 0; unsigned long seq = ++g_seq;')
    if is_buf and not has_L:
        w('  lua_State *L = B->L;')
    elif not has_L:
        w('  lua_State *L = NULL;')
    w('  (void)L;')
    w(f'  load_core();')
    w(f'  P("#%lu > {fn}(", seq);')
    first = True
    for t, n in pa:
        if not first:
            w('  P(", ");')
        first = False
        w('  ' + fmt_arg(fn, t, n))
    w('  P(")");')
    if has_L and fn not in ('lua_close',):
        w('  P(" top=%d", p_lua_gettop(L));')
    if fn == 'lua_call' or fn == 'lua_pcall':
        w('  P(" func="); IDX(L, -(nargs + 1));')
    w('  EMIT();')
    if ret == 'void':
        w(f'  p_{fn}({callargs});')
        if fn == 'lua_close':
            w('  fflush(NULL); /* do not lose buffered print/io.write output */')
            w(f'  bn = 0; P("#%lu < lua_close (C stdio streams flushed)", seq); EMIT();')
        post = fn not in ('lua_close',)
        if post:
            w(f'  bn = 0; P("#%lu < {fn}", seq);')
            if fn in ('lua_getfield', 'lua_gettable', 'lua_rawget', 'lua_rawgeti', 'lua_pushvalue', 'lua_getfenv'):
                w('  P(" -> "); IDX(L, -1);')
            if has_L:
                w('  P(" top=%d", p_lua_gettop(L));')
            elif fn == 'luaL_pushresult':
                w('  P(" -> "); IDX(L, -1); P(" top=%d", p_lua_gettop(L));')
            w('  EMIT();')
        w('}')
    else:
        w(f'  {ret} rv = p_{fn}({callargs});')
        if fn in ('lua_newstate', 'luaL_newstate'):
            w('  install_hook(rv);')
        w(f'  bn = 0; P("#%lu < {fn}", seq);')
        w('  ' + fmt_ret(fn, ret))
        for t, n in pa:
            if t.replace(' ', '') == 'size_t*':
                w(f'  if ({n}) P(" {n}=%llu", (unsigned long long)*{n});')
        if fn == 'lua_getinfo':
            w('  if (rv && what && strchr(what, \'S\')) { P(" src="); QS(ar->short_src, (size_t)-1); P(" what="); QS(ar->what, (size_t)-1); P(" linedefined=%d", ar->linedefined); }')
            w('  if (rv && what && strchr(what, \'l\')) P(" currentline=%d", ar->currentline);')
            w('  if (rv && what && strchr(what, \'n\')) { P(" name="); QS(ar->name, (size_t)-1); P(" namewhat="); QS(ar->namewhat, (size_t)-1); }')
            w('  if (rv && what && strchr(what, \'u\')) P(" nups=%d", ar->nups);')
        if has_L and fn not in ('lua_newthread',) or fn == 'luaL_newstate':
            if fn in ('luaL_newstate', 'lua_newstate'):
                pass
            else:
                w('  P(" top=%d", p_lua_gettop(L));')
        w('  EMIT();')
        w('  return rv;')
        w('}')
    w('')

header = r'''/* Generated by gen_tracer.py -- Lua 5.1 C API call tracer.
** Exports the Lua 5.1 API as lua5.1.dll; every call is logged and then
** forwarded to luacore.dll (the real implementation) in the same folder.
** Output: OutputDebugStringA (Sysinternals DebugView) and, if the
** environment variable LUATRACE_FILE is set, appended to that file.
** Set LUATRACE_OFF=1 to silence it. */
#include <windows.h>
#include <stdio.h>
#include <stdarg.h>
#include <string.h>
#include "lua.h"
#include "lauxlib.h"
#include "lualib.h"

#define TRBUF 2048
static unsigned long g_seq = 0;
static HMODULE g_core = NULL;
static FILE *g_file = NULL;
static int g_off = 0;

#define P(...) do { if (bn < TRBUF - 64) bn += _snprintf(buf + bn, TRBUF - 64 - bn, __VA_ARGS__); if (bn < 0 || bn > TRBUF - 64) bn = TRBUF - 64; } while (0)
#define EMIT() emit(buf, bn)
#define QS(s, n) bn = qs(buf, bn, (s), (n))
#define IDX(L, i) bn = tr_idx(buf, bn, (L), (i))
#define REGS(l) bn = regs(buf, bn, (l))
#define OPTS(l) bn = opts(buf, bn, (l))
#define SN(r) statusname(r)

static void emit(char *buf, int bn) {
  char line[TRBUF + 32];
  if (g_off) return;
  _snprintf(line, sizeof(line) - 2, "[luatrace] %.*s", bn, buf);
  line[sizeof(line) - 3] = 0;
  strcat(line, "\n");
  OutputDebugStringA(line);
  if (g_file) { fputs(line, g_file); fflush(g_file); }
}

static void tr_log(const char *fmt, ...) {
  char buf[TRBUF]; int bn;
  va_list ap; va_start(ap, fmt);
  bn = _vsnprintf(buf, TRBUF - 64, fmt, ap);
  va_end(ap);
  if (bn < 0 || bn > TRBUF - 64) bn = TRBUF - 64;
  emit(buf, bn);
}

static int qs(char *buf, int bn, const char *s, size_t n) {
  size_t i;
  if (s == NULL) { return bn + _snprintf(buf + bn, TRBUF - 64 - bn, "NULL"); }
  if (n == (size_t)-1) n = strlen(s);
  if (bn < TRBUF - 70) buf[bn++] = '"';
  for (i = 0; i < n && bn < TRBUF - 72; i++) {
    unsigned char c = (unsigned char)s[i];
    if (i >= 120) { memcpy(buf + bn, "...", 3); bn += 3; break; }
    if (c == '\n') { buf[bn++] = '\\'; buf[bn++] = 'n'; }
    else if (c == '"') { buf[bn++] = '\\'; buf[bn++] = '"'; }
    else if (c < 32 || c >= 127) bn += _snprintf(buf + bn, 8, "\\x%02x", c);
    else buf[bn++] = (char)c;
  }
  if (bn < TRBUF - 64) buf[bn++] = '"';
  return bn;
}
'''

ptrdecl = '\n'.join(f'static __typeof__(&{fn}) p_{fn};' for fn in ptrs)

helpers = r'''
static const char *statusname(int s) {
  switch (s) { case 0: return "OK"; case 1: return "YIELD"; case 2: return "ERRRUN";
    case 3: return "ERRSYNTAX"; case 4: return "ERRMEM"; case 5: return "ERRERR"; }
  return "?";
}

/* describe the value at an index: idx<type:value> */
static int tr_idx(char *buf, int bn, lua_State *L, int i) {
  int t;
  const char *pseudo = i == LUA_REGISTRYINDEX ? "REGISTRY" : i == LUA_GLOBALSINDEX ? "GLOBALS" :
                       i == LUA_ENVIRONINDEX ? "ENVIRON" : NULL;
  if (pseudo) bn += _snprintf(buf + bn, TRBUF - 64 - bn, "%s", pseudo);
  else if (i < LUA_GLOBALSINDEX) bn += _snprintf(buf + bn, TRBUF - 64 - bn, "upvalue(%d)", LUA_GLOBALSINDEX - i);
  else if (i != 0x7fffffff) bn += _snprintf(buf + bn, TRBUF - 64 - bn, "%d", i);
  if (i == 0x7fffffff) i = -1; /* value only, no index label */
  if (L == NULL || bn > TRBUF - 200) return bn;
  t = p_lua_type(L, i);
  switch (t) {
    case LUA_TNONE: bn += _snprintf(buf + bn, TRBUF - 64 - bn, "<none>"); break;
    case LUA_TNIL: bn += _snprintf(buf + bn, TRBUF - 64 - bn, "<nil>"); break;
    case LUA_TBOOLEAN: bn += _snprintf(buf + bn, TRBUF - 64 - bn, "<bool:%s>", p_lua_toboolean(L, i) ? "true" : "false"); break;
    case LUA_TNUMBER: bn += _snprintf(buf + bn, TRBUF - 64 - bn, "<number:%.14g>", p_lua_tonumber(L, i)); break;
    case LUA_TSTRING: {
      size_t n; const char *s = p_lua_tolstring(L, i, &n);
      bn += _snprintf(buf + bn, TRBUF - 64 - bn, "<string:");
      bn = qs(buf, bn, s, n);
      bn += _snprintf(buf + bn, TRBUF - 64 - bn, ">");
      break;
    }
    default: bn += _snprintf(buf + bn, TRBUF - 64 - bn, "<%s>", p_lua_typename(L, t)); break;
  }
  return bn;
}

static int regs(char *buf, int bn, const luaL_Reg *l) {
  int n = 0;
  if (l == NULL) return bn + _snprintf(buf + bn, TRBUF - 64 - bn, "NULL");
  bn += _snprintf(buf + bn, TRBUF - 64 - bn, "{");
  for (; l->name && bn < TRBUF - 200; l++, n++)
    bn += _snprintf(buf + bn, TRBUF - 64 - bn, "%s%s", n ? "," : "", l->name);
  return bn + _snprintf(buf + bn, TRBUF - 64 - bn, "}");
}

static int opts(char *buf, int bn, const char *const *l) {
  int n = 0;
  if (l == NULL) return bn + _snprintf(buf + bn, TRBUF - 64 - bn, "NULL");
  bn += _snprintf(buf + bn, TRBUF - 64 - bn, "{");
  for (; *l && bn < TRBUF - 200; l++, n++)
    bn += _snprintf(buf + bn, TRBUF - 64 - bn, "%s%s", n ? "," : "", *l);
  return bn + _snprintf(buf + bn, TRBUF - 64 - bn, "}");
}
'''

loader_lines = '\n'.join(f'  p_{fn} = (__typeof__(p_{fn}))resolve("{fn}");' for fn in ptrs)
loader = r'''
static FARPROC resolve(const char *name) {
  FARPROC f = GetProcAddress(g_core, name);
  if (f == NULL) tr_log("!!! luacore.dll does not export %s", name);
  return f;
}

static void load_core(void) {
  char path[MAX_PATH], *slash;
  HMODULE self = NULL;
  const char *env;
  if (g_core) return;
  env = getenv("LUATRACE_OFF");
  g_off = env && *env && *env != '0';
  env = getenv("LUATRACE_FILE");
  if (env && *env) g_file = fopen(env, "a");
  GetModuleHandleExA(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                     (LPCSTR)&load_core, &self);
  GetModuleFileNameA(self, path, MAX_PATH);
  slash = strrchr(path, '\\');
  strcpy(slash ? slash + 1 : path, "luacore.dll");
  g_core = LoadLibraryA(path);
  if (g_core == NULL) {
    char msg[MAX_PATH + 64];
    _snprintf(msg, sizeof(msg), "luatrace: cannot load %s (error %lu)", path, GetLastError());
    OutputDebugStringA(msg);
    MessageBoxA(NULL, msg, "lua5.1.dll (trace)", MB_ICONERROR);
    ExitProcess(3);
  }
  tr_log("==== luatrace: forwarding to %s (pid %lu) ====", path, GetCurrentProcessId());
''' + loader_lines + r'''
}

BOOL WINAPI DllMain(HINSTANCE h, DWORD reason, LPVOID r) {
  (void)h; (void)r;
  if (reason == DLL_PROCESS_DETACH) {
    tr_log("==== luatrace: lua5.1.dll unloading (process exit or FreeLibrary) ====");
    if (g_file) fflush(g_file);
  }
  return TRUE;
}


/* ---- optional Lua-level tracing (LUATRACE_LUA=1 calls, =2 calls+lines) ---- */
static int g_lua_level = -1;

static void tr_hook(lua_State *L, lua_Debug *ar) {
  char buf[TRBUF]; int bn = 0; unsigned long seq = ++g_seq;
  lua_Debug caller;
  if (ar->event == LUA_HOOKLINE) {
    if (!p_lua_getinfo(L, "Sl", ar)) return;
    P("#%lu LUA line %s:%d", seq, ar->short_src, ar->currentline);
    EMIT();
    return;
  }
  if (ar->event != LUA_HOOKCALL) return;
  if (!p_lua_getinfo(L, "nSl", ar)) return;
  P("#%lu LUA call %s", seq, ar->name ? ar->name : "?");
  if (ar->what && strcmp(ar->what, "C") == 0) {
    /* C function: show its first two arguments */
    int i;
    P("(");
    for (i = 1; i <= 2; i++) {
      const char *nm = p_lua_getlocal(L, ar, i);
      if (nm == NULL) break;
      if (i > 1) P(", ");
      IDX(L, 0x7fffffff);
      p_lua_settop(L, -2);
    }
    P(") [C]");
  } else {
    P(" [%s %s:%d]", ar->what ? ar->what : "?", ar->short_src, ar->linedefined);
  }
  if (p_lua_getstack(L, 1, &caller) && p_lua_getinfo(L, "Sl", &caller) && caller.currentline > 0)
    P(" from %s:%d", caller.short_src, caller.currentline);
  EMIT();
}

static void install_hook(lua_State *L) {
  const char *env;
  if (L == NULL) return;
  if (g_lua_level < 0) {
    env = getenv("LUATRACE_LUA");
    g_lua_level = (env && *env) ? atoi(env) : 0;
  }
  if (g_lua_level <= 0 || g_off) return;
  p_lua_sethook(L, tr_hook, g_lua_level >= 2 ? (LUA_MASKCALL | LUA_MASKLINE) : LUA_MASKCALL, 0);
  tr_log("==== luatrace: Lua-level tracing enabled (level %d) on L=%p ====", g_lua_level, (void *)L);
}

/* ---- hand-written wrappers (varargs / non-returning) ---- */

const char *tr_lua_pushvfstring(lua_State *L, const char *fmt, va_list argp) {
  char buf[TRBUF]; int bn = 0; unsigned long seq = ++g_seq;
  const char *r;
  load_core();
  P("#%lu > lua_pushvfstring(L=%p, fmt=", seq, (void*)L); QS(fmt, (size_t)-1); P(") top=%d", p_lua_gettop(L)); EMIT();
  r = p_lua_pushvfstring(L, fmt, argp);
  bn = 0; P("#%lu < lua_pushvfstring = ", seq); QS(r, (size_t)-1); P(" top=%d", p_lua_gettop(L)); EMIT();
  return r;
}

const char *tr_lua_pushfstring(lua_State *L, const char *fmt, ...) {
  char buf[TRBUF]; int bn = 0; unsigned long seq = ++g_seq;
  const char *r; va_list ap;
  load_core();
  P("#%lu > lua_pushfstring(L=%p, fmt=", seq, (void*)L); QS(fmt, (size_t)-1); P(") top=%d", p_lua_gettop(L)); EMIT();
  va_start(ap, fmt);
  r = p_lua_pushvfstring(L, fmt, ap);
  va_end(ap);
  bn = 0; P("#%lu < lua_pushfstring = ", seq); QS(r, (size_t)-1); P(" top=%d", p_lua_gettop(L)); EMIT();
  return r;
}

int tr_luaL_error(lua_State *L, const char *fmt, ...) {
  char buf[TRBUF]; int bn = 0; unsigned long seq = ++g_seq;
  va_list ap;
  load_core();
  /* same as lauxlib.c: luaL_where(L, 1) .. formatted message, then lua_error */
  p_luaL_where(L, 1);
  va_start(ap, fmt);
  p_lua_pushvfstring(L, fmt, ap);
  va_end(ap);
  p_lua_concat(L, 2);
  P("#%lu > luaL_error(L=%p) msg=", seq, (void*)L); IDX(L, -1); P(" top=%d  (raises, does not return)", p_lua_gettop(L)); EMIT();
  return p_lua_error(L);
}

int tr_lua_error(lua_State *L) {
  char buf[TRBUF]; int bn = 0; unsigned long seq = ++g_seq;
  load_core();
  P("#%lu > lua_error(L=%p) value=", seq, (void*)L); IDX(L, -1); P(" top=%d  (raises, does not return)", p_lua_gettop(L)); EMIT();
  return p_lua_error(L);
}
'''

import os
INTERNAL_DATA = open('internals_data.txt').read().split() if os.path.exists('internals_data.txt') else []
INTERNAL_FUNCS = [n for n in (open('internals.txt').read().split() if os.path.exists('internals.txt') else []) if n not in INTERNAL_DATA]

with open('internals.S', 'w') as f:
    f.write('    .text\n')
    for i, nm in enumerate(INTERNAL_FUNCS):
        f.write(f'''    .globl ix_{nm}
    .p2align 4
ix_{nm}:
    push %rcx
    push %rdx
    push %r8
    push %r9
    sub $104, %rsp
    movdqu %xmm0, 32(%rsp)
    movdqu %xmm1, 48(%rsp)
    movdqu %xmm2, 64(%rsp)
    movdqu %xmm3, 80(%rsp)
    mov ${i}, %ecx
    call tr_internal_hit
    movdqu 32(%rsp), %xmm0
    movdqu 48(%rsp), %xmm1
    movdqu 64(%rsp), %xmm2
    movdqu 80(%rsp), %xmm3
    add $104, %rsp
    pop %r9
    pop %r8
    pop %rdx
    pop %rcx
    jmp *%rax
''')

internal_c = '''
/* ---- PUC-Rio internal functions: logged, then forwarded to luacore.dll ---- */
static const char *g_internal_names[] = {''' + ','.join(f'"{n}"' for n in INTERNAL_FUNCS) + '''};
static void *g_internal_ptr[''' + str(max(1, len(INTERNAL_FUNCS))) + '''];
static unsigned g_internal_hits[''' + str(max(1, len(INTERNAL_FUNCS))) + '''];

void *tr_internal_hit(int i) {
  load_core();
  if (g_internal_ptr[i] == NULL) g_internal_ptr[i] = (void *)GetProcAddress(g_core, g_internal_names[i]);
  g_internal_hits[i]++;
  if (g_internal_hits[i] <= 5 || (g_internal_hits[i] & (g_internal_hits[i] - 1)) == 0)
    tr_log("#%lu !!! INTERNAL %s called (PUC-Rio private function, hit #%u) -> %s", ++g_seq,
           g_internal_names[i], g_internal_hits[i], g_internal_ptr[i] ? "forwarded to luacore.dll" : "MISSING in luacore.dll");
  if (g_internal_ptr[i] == NULL) {
    char msg[256];
    _snprintf(msg, sizeof(msg), "A module called the PUC-Rio internal function %s, which luacore.dll does not provide. The process will exit.", g_internal_names[i]);
    tr_log("!!! FATAL: %s", msg);
    MessageBoxA(NULL, msg, "lua5.1.dll (trace)", MB_ICONERROR);
    ExitProcess(4);
  }
  return g_internal_ptr[i];
}
'''

with open('luatrace.c', 'w') as f:
    f.write(header)
    f.write(ptrdecl + '\n')
    f.write(helpers)
    # forward declarations of generated wrappers not needed; loader first
    f.write(loader)
    f.write(internal_c)
    f.write('\n/* ---- generated wrappers ---- */\n\n')
    f.write('\n'.join(out))

with open('luatrace.def', 'w') as f:
    f.write('LIBRARY "lua5.1.dll"\nEXPORTS\n')
    for fn in ptrs:
        f.write(f'    {fn} = tr_{fn}\n')
    # PUC-Rio internals (exported by LuaBinaries-style builds). Functions go
    # through logged thunks (internals.S); data symbols are plain forwarders.
    for nm in INTERNAL_FUNCS:
        f.write(f'    {nm} = ix_{nm}\n')
    for nm in INTERNAL_DATA:
        f.write(f'    {nm} = luacore.{nm}\n')
print(len(ptrs), 'functions')
