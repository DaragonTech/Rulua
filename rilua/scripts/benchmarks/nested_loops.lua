-- Nested loops with local variables: stresses register allocation and dispatch
local s = 0
for i = 1, 1000 do
    for j = 1, 1000 do
        s = s + i * j
    end
end
print(s)
