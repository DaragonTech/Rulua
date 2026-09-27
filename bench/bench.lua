-- bench.lua -- Lua 5.1 performance comparison suite.
--
-- Usage:  lua bench.lua [scale] [filter]
--   scale  : multiplies the work of every test (default 1; use 0.2 for a
--            quick run, 3 for a long, more stable one)
--   filter : only run tests whose name contains this text
--
-- Every test computes a checksum so both implementations can be checked for
-- identical results, not just speed. Output lines:
--   RESULT <name> <seconds> <checksum>
-- Save the output of two runs to files and compare them with compare.lua.

local scale = tonumber(arg and arg[1]) or 1
local filter = arg and arg[2]
local clock = os.clock
local floor, sqrt, sin = math.floor, math.sqrt, math.sin
local fmt, byte, char, sub, rep = string.format, string.byte, string.char, string.sub, string.rep
local concat, insert, sort = table.concat, table.insert, table.sort

local function n(x) return floor(x * scale + 0.5) end

local tests = {}
local function test(name, desc, fn) tests[#tests + 1] = { name = name, desc = desc, fn = fn } end

-------------------------------------------------------------------------------
-- Core VM
-------------------------------------------------------------------------------

test("loop_arith", "numeric for loop with arithmetic", function()
  local s = 0
  for i = 1, n(20000000) do s = s + i % 7 * 2 - 1 end
  return s
end)

test("while_loop", "while loop, locals, comparisons", function()
  local i, s = 0, 0
  local lim = n(10000000)
  while i < lim do
    i = i + 1
    if i % 3 == 0 then s = s + 1 elseif i % 5 == 0 then s = s + 2 else s = s - 1 end
  end
  return s
end)

test("fib_recursive", "recursive Lua function calls", function()
  local function fib(k) if k < 2 then return k end return fib(k - 1) + fib(k - 2) end
  local s = 0
  for _ = 1, n(20) do s = s + fib(27) end
  return s
end)

test("closures", "creating and calling closures / upvalues", function()
  local s = 0
  for i = 1, n(2000000) do
    local f = function(x) return x + i end
    s = s + f(1)
  end
  local function counter() local c = 0; return function() c = c + 1; return c end end
  local c = counter()
  for _ = 1, n(3000000) do c() end
  return s + c()
end)

test("varargs", "vararg functions, select, unpack", function()
  local function sum(...)
    local t = 0
    for i = 1, select("#", ...) do t = t + (select(i, ...)) end
    return t
  end
  local s = 0
  local args = { 1, 2, 3, 4, 5 }
  for _ = 1, n(500000) do s = s + sum(unpack(args)) end
  return s
end)

-------------------------------------------------------------------------------
-- Tables
-------------------------------------------------------------------------------

test("table_array", "array fill + read (t[i] = ...)", function()
  local s = 0
  for _ = 1, n(20) do
    local t = {}
    for i = 1, 200000 do t[i] = i * 2 end
    for i = 1, #t do s = s + t[i] end
  end
  return s
end)

test("table_hash", "string-keyed hash insert + lookup", function()
  local keys = {}
  for i = 1, 20000 do keys[i] = "key" .. i end
  local s = 0
  for _ = 1, n(80) do
    local t = {}
    for i = 1, #keys do t[keys[i]] = i end
    for i = 1, #keys do s = s + t[keys[i]] end
  end
  return s
end)

test("table_insert_remove", "table.insert / table.remove", function()
  local s = 0
  for _ = 1, n(20) do
    local t = {}
    for i = 1, 50000 do insert(t, i) end
    for _ = 1, 25000 do s = s + table.remove(t) end
    insert(t, 1, 0); table.remove(t, 1)
    s = s + #t
  end
  return s
end)

test("table_pairs", "pairs / ipairs / next iteration", function()
  local t = {}
  for i = 1, 100000 do t[i] = i; t["k" .. i] = i end
  local s = 0
  for _ = 1, n(10) do
    for _, v in pairs(t) do s = s + v end
    for _, v in ipairs(t) do s = s + v end
  end
  return s
end)

test("table_sort", "table.sort numbers and strings", function()
  local s = 0
  for r = 1, n(5) do
    local t, u = {}, {}
    local x = r
    for i = 1, 50000 do
      x = (x * 1103515245 + 12345) % 2147483648
      t[i] = x
      u[i] = fmt("%08d", x % 99999999)
    end
    sort(t)
    sort(u)
    sort(t, function(a, b) return a > b end)
    s = s + t[1] % 1000 + tonumber(u[1]) % 1000
  end
  return s
end)

test("table_concat", "table.concat of many strings", function()
  local s = 0
  for _ = 1, n(20) do
    local t = {}
    for i = 1, 50000 do t[i] = tostring(i) end
    s = s + #concat(t, ",")
  end
  return s
end)

-------------------------------------------------------------------------------
-- Strings
-------------------------------------------------------------------------------

test("string_concat", "string building with .. in a loop", function()
  local s = 0
  for _ = 1, n(30) do
    local str = ""
    for i = 1, 5000 do str = str .. "x" end
    s = s + #str
  end
  return s
end)

test("string_format", "string.format with numbers and strings", function()
  local s = 0
  for i = 1, n(300000) do
    s = s + #fmt("%d:%s:%5.2f:%x", i, "abc", i / 3, i)
  end
  return s
end)

test("string_ops", "sub / byte / char / upper / rep / len", function()
  local str = rep("Hello, World! ", 20)
  local s = 0
  for i = 1, n(1500000) do
    local k = i % #str + 1
    s = s + byte(str, k) + #sub(str, k, k + 5) + #char(65 + i % 26)
    if i % 10 == 0 then s = s + #str:upper() end
  end
  return s
end)

test("string_find", "string.find / match (patterns and plain)", function()
  local text = rep("lorem ipsum dolor sit amet 12345 consectetur ", 50)
  local s = 0
  for _ = 1, n(100000) do
    s = s + (text:find("amet", 1, true) or 0)
    s = s + (text:find("%d+") or 0)
    local w = text:match("(%a+) (%a+) %d+")
    s = s + (w and #w or 0)
  end
  return s
end)

test("string_gsub", "string.gsub with string, table and function", function()
  local text = rep("The quick brown fox jumps over the lazy dog. ", 40)
  local map = { quick = "slow", brown = "red", lazy = "busy" }
  local s = 0
  for _ = 1, n(4000) do
    s = s + #text:gsub("o", "0")
    s = s + #text:gsub("%a+", map)
    s = s + #text:gsub("%a+", function(w) return #w > 4 and w:upper() or nil end)
  end
  return s
end)

test("string_gmatch", "string.gmatch tokenizing", function()
  local text = rep("alpha=1, beta=22, gamma=333; ", 400)
  local s = 0
  for _ = 1, n(600) do
    for k, v in text:gmatch("(%a+)=(%d+)") do s = s + #k + tonumber(v) end
  end
  return s
end)

test("string_large", "sub / byte / find walking a 1 MB string", function()
  local big = rep("<td>value 1234</td>\n", 50000)  -- ~1 MB
  local s = 0
  for i = 1, n(200000) do
    local k = (i * 37) % (#big - 10) + 1
    s = s + byte(big, k) + #sub(big, k, k + 4)
  end
  local pos, c = 1, 0
  for _ = 1, n(4) do
    pos = 1
    while true do
      local a, b = big:find("</td>", pos, true)
      if not a then break end
      c = c + 1; pos = b + 1
    end
  end
  return s + c
end)

test("tostring_tonumber", "number <-> string conversions", function()
  local s = 0
  for i = 1, n(500000) do
    s = s + tonumber(tostring(i * 1.5))
  end
  return s
end)

-------------------------------------------------------------------------------
-- OOP / metatables
-------------------------------------------------------------------------------

test("oop_methods", "class-style objects, method calls via __index", function()
  local Point = {}
  Point.__index = Point
  function Point.new(x, y) return setmetatable({ x = x, y = y }, Point) end
  function Point:add(o) return Point.new(self.x + o.x, self.y + o.y) end
  function Point:len2() return self.x * self.x + self.y * self.y end
  local acc = Point.new(0, 0)
  local one = Point.new(1, 2)
  for _ = 1, n(1000000) do acc = acc:add(one) end
  return acc:len2()
end)

test("metamethods", "__add / __eq / __lt / __index function / __newindex", function()
  local V = {}
  V.__add = function(a, b) return setmetatable({ v = a.v + b.v }, V) end
  V.__eq = function(a, b) return a.v == b.v end
  V.__lt = function(a, b) return a.v < b.v end
  local a, b = setmetatable({ v = 1 }, V), setmetatable({ v = 2 }, V)
  local s = 0
  for _ = 1, n(1000000) do
    local c = a + b
    if c == b then s = s + 1 end
    if a < c then s = s + 1 end
  end
  local proxy = setmetatable({}, { __index = function(_, k) return k * 2 end,
                                   __newindex = function(t, k, v) rawset(t, k, v) end })
  for i = 1, n(300000) do s = s + proxy[i] end
  for i = 1, n(100000) do proxy[i] = i end
  return s
end)

-------------------------------------------------------------------------------
-- Library calls, coroutines, errors, GC
-------------------------------------------------------------------------------

test("math_lib", "math library calls", function()
  local s = 0
  for i = 1, n(2000000) do
    s = s + floor(sqrt(i)) + sin(i) + math.max(i % 10, 5) + math.abs(-i % 3)
  end
  return floor(s)
end)

test("coroutines", "coroutine create / resume / yield", function()
  local s = 0
  local gen = coroutine.wrap(function()
    for i = 1, n(2000000) do coroutine.yield(i) end
    return 0
  end)
  for _ = 1, n(2000000) do s = s + gen() end
  for _ = 1, n(100000) do
    local co = coroutine.create(function(a) local b = coroutine.yield(a + 1); return b * 2 end)
    local _, x = coroutine.resume(co, 1)
    local _, y = coroutine.resume(co, x)
    s = s + y
  end
  return s
end)

test("pcall_error", "pcall with and without errors", function()
  local s = 0
  local function ok(x) return x + 1 end
  local function bad(x) error("fail " .. x) end
  for i = 1, n(1500000) do
    local st, v = pcall(ok, i)
    if st then s = s + v end
  end
  for i = 1, n(200000) do
    local st = pcall(bad, i)
    if not st then s = s + 1 end
  end
  return s
end)

test("gc_churn", "allocating many short-lived tables and strings", function()
  local s = 0
  for i = 1, n(500000) do
    local t = { i, i + 1, name = "obj" .. (i % 1000) }
    s = s + #t + #t.name
  end
  collectgarbage()
  return s
end)

test("loadstring", "compiling Lua source at runtime", function()
  local s = 0
  for i = 1, n(30000) do
    local f = loadstring("local a = " .. i .. " local t = {} for k = 1, 10 do t[k] = a * k end return t[10]")
    s = s + f()
  end
  return s
end)

-------------------------------------------------------------------------------
-- Runner
-------------------------------------------------------------------------------

io.write(fmt("# Lua benchmark  version=%s  scale=%s\n", _VERSION, tostring(scale)))
io.write(fmt("# %-20s %10s  %s\n", "test", "seconds", "checksum"))
local total = 0
for _, t in ipairs(tests) do
  if not filter or t.name:find(filter, 1, true) then
    collectgarbage()
    local t0 = clock()
    local ok, res = pcall(t.fn)
    local dt = clock() - t0
    total = total + dt
    if ok then
      io.write(fmt("RESULT %-20s %10.3f  %s\n", t.name, dt, fmt("%.17g", res)))
    else
      io.write(fmt("RESULT %-20s %10s  ERROR: %s\n", t.name, "-", tostring(res)))
    end
    io.flush()
  end
end
io.write(fmt("RESULT %-20s %10.3f\n", "TOTAL", total))
