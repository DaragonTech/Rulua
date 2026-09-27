-- compare.lua -- compare two bench.lua result files.
-- Usage: lua compare.lua reference.txt candidate.txt
-- (e.g. reference = original lua5.1.dll, candidate = rilua lua5.1.dll)

local function load(path)
  local r, order = {}, {}
  local f = assert(io.open(path, "r"), "cannot open " .. path)
  for line in f:lines() do
    local name, secs, sum = line:match("^RESULT%s+(%S+)%s+(%S+)%s*(.*)$")
    if name then
      r[name] = { secs = tonumber(secs), sum = sum }
      order[#order + 1] = name
    end
  end
  f:close()
  return r, order
end

local a_path, b_path = arg[1], arg[2]
if not (a_path and b_path) then
  print("usage: lua compare.lua reference.txt candidate.txt")
  return
end
local a, order = load(a_path)
local b = load(b_path)

print(string.format("%-20s %10s %10s %9s  %s", "test", "ref(s)", "cand(s)", "cand/ref", "checksums"))
print(string.rep("-", 66))
local mismatches = 0
for _, name in ipairs(order) do
  local x, y = a[name], b[name]
  if y then
    local ratio = (x.secs and y.secs and x.secs > 0) and string.format("%8.2fx", y.secs / x.secs) or "       -"
    local same
    if name == "TOTAL" then same = "" elseif x.sum == y.sum then same = "same" else same = "DIFFERENT"; mismatches = mismatches + 1 end
    print(string.format("%-20s %10s %10s %9s  %s", name,
      x.secs and string.format("%.3f", x.secs) or "-",
      y.secs and string.format("%.3f", y.secs) or "-", ratio, same))
  end
end
print(string.rep("-", 66))
print(mismatches == 0 and "All checksums match." or (mismatches .. " test(s) produced different results!"))
