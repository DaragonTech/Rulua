-- advanced.lua: Companion script for the advanced_embedding example.
--
-- This script exercises features set up by the Rust host:
--   - Vec2 userdata with arithmetic metamethods
--   - A registered Rust function (rust_add)
--   - Coroutine interaction

-- 1. Vec2 userdata (created and configured by the Rust host)
print("--- Vec2 userdata ---")
local a = vec2_new(3, 4)
local b = vec2_new(1, 2)
print("a =", vec2_tostring(a))
print("b =", vec2_tostring(b))

-- Arithmetic metamethods (__add, __mul by scalar)
local c = a + b
print("a + b =", vec2_tostring(c))

local d = a * 2
print("a * 2 =", vec2_tostring(d))

-- Length via __len metamethod
print("#a =", #a)

-- Equality via __eq metamethod
print("a == a?", a == vec2_new(3, 4))
print("a == b?", a == b)

-- 2. Calling a Rust function
print("\n--- Rust function ---")
print("rust_add(10, 32) =", rust_add(10, 32))

-- 3. Coroutine that yields values for the Rust host to consume
print("\n--- Coroutine (Lua side) ---")
function make_counter(start, stop)
    return coroutine.create(function()
        for i = start, stop do
            coroutine.yield(i)
        end
        return "done"
    end)
end

-- 4. Error handling via pcall
print("\n--- Error handling ---")
local ok, err = pcall(function()
    error("intentional error from Lua")
end)
print("pcall caught error:", ok, err)

-- 5. Table returned to Rust
print("\n--- Table for Rust ---")
function make_config()
    return {
        name = "rilua",
        version = 1,
        features = {"embedding", "userdata", "coroutines"},
    }
end
