-- Table construction and traversal: stresses table array path and GC.
-- Avoids # length operator for compatibility with incomplete
-- implementations.
local t = {}
for i = 1, 100000 do
    t[i] = i
end
local s = 0
for i = 1, 100000 do
    s = s + t[i]
end
print(s)
