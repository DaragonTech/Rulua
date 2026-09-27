-- A short script that exercises basic Lua features.

local greeting = "Hello from rilua!"
print(greeting)

local function factorial(n)
    if n <= 1 then return 1 end
    return n * factorial(n - 1)
end

for i = 1, 5 do
    print(string.format("  %d! = %d", i, factorial(i)))
end
