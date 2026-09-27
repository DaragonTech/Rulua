-- Tight arithmetic loop: stresses VM dispatch and numeric for-loop
local s = 0
for i = 1, 1000000 do
    s = s + i
end
print(s)
