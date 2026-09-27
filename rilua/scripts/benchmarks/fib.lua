-- Recursive fibonacci: stresses function call overhead
-- Uses function-statement syntax (not "local function") for compatibility.
function fib(n)
    if n < 2 then return n end
    return fib(n - 1) + fib(n - 2)
end
print(fib(35))
