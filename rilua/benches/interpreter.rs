//! Criterion benchmarks for rilua interpreter hot paths.
//!
//! Run with: `cargo bench`

#![allow(clippy::expect_used)]

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use rilua::compiler::codegen;
use rilua::{Lua, LuaApiMut, StdLib, Val};

// ---------------------------------------------------------------------------
// State creation
// ---------------------------------------------------------------------------

fn bench_state_creation(c: &mut Criterion) {
    let mut group = c.benchmark_group("state_creation");

    group.bench_function("new_empty", |b| {
        b.iter(|| {
            let lua = Lua::new_empty();
            black_box(lua);
        });
    });

    group.bench_function("new_with_base", |b| {
        b.iter(|| {
            let lua = Lua::new_with(StdLib::BASE).expect("new_with failed");
            black_box(lua);
        });
    });

    group.bench_function("new_full", |b| {
        b.iter(|| {
            let lua = Lua::new().expect("new failed");
            black_box(lua);
        });
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// Compilation
// ---------------------------------------------------------------------------

fn bench_compilation(c: &mut Criterion) {
    let mut group = c.benchmark_group("compilation");

    // Minimal script.
    group.bench_function("compile_minimal", |b| {
        b.iter(|| {
            let proto = codegen::compile(black_box(b"return 1"), "bench");
            black_box(proto).expect("compile failed");
        });
    });

    // Arithmetic loop.
    let loop_src = b"local s = 0; for i = 1, 1000 do s = s + i end; return s";
    group.bench_function("compile_loop", |b| {
        b.iter(|| {
            let proto = codegen::compile(black_box(loop_src), "bench");
            black_box(proto).expect("compile failed");
        });
    });

    // Function definitions.
    let funcs_src = br"
        local function fib(n)
            if n < 2 then return n end
            return fib(n - 1) + fib(n - 2)
        end
        local function fact(n)
            if n <= 1 then return 1 end
            return n * fact(n - 1)
        end
        return fib(10) + fact(10)
    ";
    group.bench_function("compile_functions", |b| {
        b.iter(|| {
            let proto = codegen::compile(black_box(funcs_src), "bench");
            black_box(proto).expect("compile failed");
        });
    });

    // Table-heavy code.
    let table_src = br#"
        local t = {}
        for i = 1, 100 do
            t[i] = { x = i, y = i * 2, name = "item" .. i }
        end
        local s = 0
        for i = 1, #t do s = s + t[i].x + t[i].y end
        return s
    "#;
    group.bench_function("compile_tables", |b| {
        b.iter(|| {
            let proto = codegen::compile(black_box(table_src), "bench");
            black_box(proto).expect("compile failed");
        });
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// VM execution
// ---------------------------------------------------------------------------

fn bench_vm_execution(c: &mut Criterion) {
    let mut group = c.benchmark_group("vm_execution");

    // Arithmetic loop.
    group.bench_function("loop_sum_1k", |b| {
        let mut lua = Lua::new_with(StdLib::BASE).expect("new failed");
        b.iter(|| {
            lua.exec(black_box("local s = 0; for i = 1, 1000 do s = s + i end"))
                .expect("exec failed");
        });
    });

    // Recursive fibonacci.
    group.bench_function("fib_20", |b| {
        let mut lua = Lua::new_with(StdLib::BASE).expect("new failed");
        lua.exec("function fib(n) if n < 2 then return n end return fib(n-1) + fib(n-2) end")
            .expect("define fib failed");
        b.iter(|| {
            lua.exec(black_box("fib(20)")).expect("fib failed");
        });
    });

    // String concatenation.
    group.bench_function("string_concat_100", |b| {
        let mut lua = Lua::new_with(StdLib::BASE | StdLib::STRING).expect("new failed");
        b.iter(|| {
            lua.exec(black_box(
                "local s = ''; for i = 1, 100 do s = s .. 'x' end",
            ))
            .expect("concat failed");
        });
    });

    // Table construction and access.
    group.bench_function("table_build_1k", |b| {
        let mut lua =
            Lua::new_with(StdLib::BASE | StdLib::TABLE).expect("new failed");
        b.iter(|| {
            lua.exec(black_box(
                "local t = {}; for i = 1, 1000 do t[i] = i end; local s = 0; for i = 1, 1000 do s = s + t[i] end",
            ))
            .expect("table failed");
        });
    });

    // Function calls (closure creation + upvalue access).
    group.bench_function("closures_100", |b| {
        let mut lua = Lua::new_with(StdLib::BASE).expect("new failed");
        b.iter(|| {
            lua.exec(black_box(
                r"
                local fns = {}
                for i = 1, 100 do
                    local x = i
                    fns[i] = function() return x end
                end
                local s = 0
                for i = 1, 100 do s = s + fns[i]() end
                ",
            ))
            .expect("closures failed");
        });
    });

    // Method dispatch (metatables).
    group.bench_function("metatable_index_1k", |b| {
        let mut lua = Lua::new_with(StdLib::BASE).expect("new failed");
        lua.exec(
            r"
            local mt = { __index = function(t, k) return k end }
            G_proxy = setmetatable({}, mt)
            ",
        )
        .expect("setup failed");
        b.iter(|| {
            lua.exec(black_box(
                "local p = G_proxy; local s = 0; for i = 1, 1000 do s = s + p[i] end",
            ))
            .expect("meta failed");
        });
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// Garbage collection
// ---------------------------------------------------------------------------

fn bench_gc(c: &mut Criterion) {
    let mut group = c.benchmark_group("gc");

    // Full GC cycle on a state with many allocations.
    group.bench_function("collect_10k_tables", |b| {
        let mut lua = Lua::new_with(StdLib::BASE).expect("new failed");
        lua.gc_stop();
        // Pre-allocate many tables.
        lua.exec("G_tables = {}; for i = 1, 10000 do G_tables[i] = {i, i+1, i+2} end")
            .expect("alloc failed");
        b.iter(|| {
            lua.gc_collect().expect("gc failed");
        });
    });

    // GC with dead objects (allocation churn).
    group.bench_function("churn_alloc_collect", |b| {
        let mut lua = Lua::new_with(StdLib::BASE).expect("new failed");
        b.iter(|| {
            lua.gc_stop();
            lua.exec(black_box(
                "for i = 1, 1000 do local t = {i, i+1}; local s = 'str' .. i end",
            ))
            .expect("churn failed");
            lua.gc_collect().expect("gc failed");
        });
    });

    // Incremental GC stepping.
    group.bench_function("step_incremental", |b| {
        let mut lua = Lua::new_with(StdLib::BASE).expect("new failed");
        lua.gc_stop();
        lua.exec("G_data = {}; for i = 1, 5000 do G_data[i] = {x=i} end")
            .expect("setup failed");
        b.iter(|| {
            let _ = lua.gc_step(black_box(1024));
        });
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// String interning
// ---------------------------------------------------------------------------

fn bench_string_interning(c: &mut Criterion) {
    let mut group = c.benchmark_group("string_interning");

    // Intern unique strings.
    group.bench_function("intern_unique_1k", |b| {
        let mut lua = Lua::new_empty();
        b.iter(|| {
            for i in 0..1000 {
                let s = format!("unique_string_{i}");
                black_box(lua.create_string(s.as_bytes()));
            }
        });
    });

    // Intern duplicate strings (dedup hit).
    group.bench_function("intern_dedup_1k", |b| {
        let mut lua = Lua::new_empty();
        // Pre-intern the strings.
        for i in 0..100 {
            let s = format!("dedup_{i}");
            lua.create_string(s.as_bytes());
        }
        let keys: Vec<String> = (0..100).map(|i| format!("dedup_{i}")).collect();
        b.iter(|| {
            for key in &keys {
                black_box(lua.create_string(key.as_bytes()));
            }
        });
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// Table operations (via Lua API)
// ---------------------------------------------------------------------------

fn bench_table_ops(c: &mut Criterion) {
    let mut group = c.benchmark_group("table_ops");

    // Integer key insert via raw_set.
    group.bench_function("raw_set_int_1k", |b| {
        let mut lua = Lua::new_empty();
        b.iter(|| {
            let table = lua.create_table();
            for i in 1..=1000 {
                lua.table_raw_set(&table, Val::Num(f64::from(i)), Val::Num(f64::from(i)))
                    .expect("raw_set failed");
            }
            black_box(table);
        });
    });

    // String key insert via raw_set.
    group.bench_function("raw_set_str_1k", |b| {
        let mut lua = Lua::new_empty();
        let keys: Vec<Val> = (0..1000)
            .map(|i| {
                let s = format!("key_{i}");
                lua.create_string(s.as_bytes())
            })
            .collect();
        b.iter(|| {
            let table = lua.create_table();
            for (i, key) in keys.iter().enumerate() {
                lua.table_raw_set(&table, *key, Val::Num(i as f64))
                    .expect("raw_set failed");
            }
            black_box(table);
        });
    });

    // Mixed table operations in Lua.
    group.bench_function("mixed_ops_lua", |b| {
        let mut lua = Lua::new_with(StdLib::BASE | StdLib::TABLE).expect("new failed");
        b.iter(|| {
            lua.exec(black_box(
                r#"
                local t = {}
                for i = 1, 500 do t[i] = i end
                for i = 1, 500 do t["k" .. i] = i end
                local s = 0
                for i = 1, 500 do s = s + t[i] end
                for i = 1, 500 do s = s + t["k" .. i] end
                "#,
            ))
            .expect("mixed ops failed");
        });
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// End-to-end
// ---------------------------------------------------------------------------

fn bench_end_to_end(c: &mut Criterion) {
    let mut group = c.benchmark_group("end_to_end");

    // Compile + execute a realistic script.
    group.bench_function("compile_and_run", |b| {
        b.iter(|| {
            let mut lua = Lua::new().expect("new failed");
            lua.exec(black_box(
                r"
                local function map(t, f)
                    local r = {}
                    for i = 1, #t do r[i] = f(t[i]) end
                    return r
                end
                local data = {}
                for i = 1, 100 do data[i] = i end
                local doubled = map(data, function(x) return x * 2 end)
                local sum = 0
                for i = 1, #doubled do sum = sum + doubled[i] end
                ",
            ))
            .expect("exec failed");
        });
    });

    // Coroutine create/resume/yield cycle.
    group.bench_function("coroutine_cycle", |b| {
        let mut lua = Lua::new_with(StdLib::BASE | StdLib::COROUTINE).expect("new failed");
        b.iter(|| {
            lua.exec(black_box(
                r#"
                local co = coroutine.create(function()
                    for i = 1, 50 do
                        coroutine.yield(i)
                    end
                    return 0
                end)
                local s = 0
                while coroutine.status(co) ~= "dead" do
                    local ok, v = coroutine.resume(co)
                    if ok then s = s + v end
                end
                "#,
            ))
            .expect("coroutine failed");
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_state_creation,
    bench_compilation,
    bench_vm_execution,
    bench_gc,
    bench_string_interning,
    bench_table_ops,
    bench_end_to_end,
);
criterion_main!(benches);
