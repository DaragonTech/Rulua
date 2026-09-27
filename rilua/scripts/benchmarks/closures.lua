-- Closure creation and upvalue access: stresses closure allocation.
-- Uses function-statement syntax for compatibility.
function counter()
    local n = 0
    return function()
        n = n + 1
        return n
    end
end

local c = counter()
local s = 0
for i = 1, 500000 do
    s = s + c()
end
print(s)
