#!/usr/bin/env bash
# Benchmark Lua implementations against PUC-Rio 5.1.1 test suite and
# micro-benchmarks.
#
# Usage: ./scripts/benchmark-implementations.sh [runs]
# Default: 10 runs per test, reports median in milliseconds.
#
# Implementations:
#   pucrio     - PUC-Rio Lua 5.1.1 (C reference)
#   rilua      - rilua (pure Rust, Lua 5.1.1)
#   mlua       - mlua with vendored Lua 5.1 (Rust FFI to C)
#   lua_rs     - lua-rs by CppCXY (pure Rust, Lua 5.5, nightly)
#   lua-in-rust - lua-in-rust by cjneidhart (pure Rust, Lua 5.1, incomplete)

set -euo pipefail

RUNS="${1:-10}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TESTDIR="$ROOT/lua-5.1-tests"
BENCHDIR="$ROOT/scripts/benchmarks"
CSVFILE="$ROOT/benchmark-results.csv"

# --- Implementation paths ---
PUCRIO_BIN="$ROOT/lua-5.1.1/src/lua"
RILUA_BIN="$ROOT/target/release/rilua"
MLUA_BIN="$ROOT/scripts/mlua-runner/target/release/mlua-runner"
LUARS_ROOT="/home/danielsreichenbach/Repos/github.com/CppCXY/lua-rs"
LUARS_BIN="$LUARS_ROOT/target/release/lua"
LIR_ROOT="/home/danielsreichenbach/Repos/github.com/cjneidhart/lua-in-rust"
LIR_BIN="$LIR_ROOT/target/release/lua"

# PUC-Rio test suite files (standalone, from all.lua)
SUITE_TESTS=(
    gc.lua db.lua calls.lua strings.lua literals.lua attrib.lua
    locals.lua constructs.lua code.lua nextvar.lua pm.lua api.lua
    events.lua vararg.lua closure.lua errors.lua math.lua sort.lua
    verybig.lua files.lua
)

# Micro-benchmarks (minimal stdlib requirements)
MICRO_TESTS=(fib.lua loop.lua tables.lua closures.lua nested_loops.lua)

# Implementation labels for display
declare -A IMPL_LABELS=(
    [pucrio]="PUC-Rio"
    [rilua]="rilua"
    [mlua]="mlua"
    [lua_rs]="lua-rs"
    [lua_in_rust]="lua-in-rust"
)

# ---- Utility functions ----

# Compute median from a list of integers (one per line on stdin)
median() {
    sort -n | awk '{a[NR]=$1} END {
        if (NR%2==1) printf "%d\n", a[(NR+1)/2]
        else printf "%d\n", (a[NR/2]+a[NR/2+1])/2
    }'
}

# Time a single invocation (ms)
time_lua() {
    local bin="$1"
    local script="$2"
    local env_prefix="${3:-}"
    local start end_t
    start=$(date +%s%N)
    if [ -n "$env_prefix" ]; then
        env $env_prefix "$bin" "$script" >/dev/null 2>&1 || true
    else
        "$bin" "$script" >/dev/null 2>&1 || true
    fi
    end_t=$(date +%s%N)
    echo $(( (end_t - start) / 1000000 ))
}

# Run benchmark: 1 warmup + N timed runs, write raw times to CSV,
# return median on stdout.
#   bench <impl_name> <suite_name> <bin> <script> [env_prefix]
bench() {
    local impl="$1"
    local suite="$2"
    local bin="$3"
    local script="$4"
    local env_prefix="${5:-}"
    local test_name
    test_name="$(basename "$script")"

    # Warmup (discard)
    time_lua "$bin" "$script" "$env_prefix" >/dev/null

    local times=()
    for ((i=1; i<=RUNS; i++)); do
        local t
        t="$(time_lua "$bin" "$script" "$env_prefix")"
        times+=("$t")
        echo "$impl,$suite,$test_name,$i,$t" >> "$CSVFILE"
    done
    printf '%s\n' "${times[@]}" | median
}

# Check if an implementation can run a script (60s timeout)
probe() {
    local bin="$1"
    local script="$2"
    local env_prefix="${3:-}"
    if [ -n "$env_prefix" ]; then
        timeout 60 env $env_prefix "$bin" "$script" >/dev/null 2>&1
    else
        timeout 60 "$bin" "$script" >/dev/null 2>&1
    fi
    return $?
}

# Get binary path for an implementation
impl_bin() {
    case "$1" in
        pucrio)      echo "$PUCRIO_BIN" ;;
        rilua)       echo "$RILUA_BIN" ;;
        mlua)        echo "$MLUA_BIN" ;;
        lua_rs)      echo "$LUARS_BIN" ;;
        lua_in_rust) echo "$LIR_BIN" ;;
    esac
}

# Get env vars for an implementation.
# Note: RILUA_TEST_LIB is NOT set here. Enabling it makes rilua run
# additional test paths (T module) that PUC-Rio skips unless compiled
# with -DLUA_USER_H, creating an unfair comparison.
impl_env() {
    echo ""
}

# ---- Phase 1: Build all implementations ----

echo "=== Building Implementations ==="
echo ""

build_ok=()

# PUC-Rio
if [ -f "$PUCRIO_BIN" ]; then
    echo "[pucrio]      Binary exists: $PUCRIO_BIN"
    build_ok+=(pucrio)
elif [ -d "$ROOT/lua-5.1.1" ]; then
    echo "[pucrio]      Building..."
    if make -C "$ROOT/lua-5.1.1" linux >/dev/null 2>&1; then
        echo "[pucrio]      Built"
        build_ok+=(pucrio)
    else
        echo "[pucrio]      BUILD FAILED"
    fi
else
    echo "[pucrio]      Source not found at $ROOT/lua-5.1.1/"
    echo "              Download: curl -R -O https://www.lua.org/ftp/lua-5.1.1.tar.gz"
fi

# rilua
echo "[rilua]       Building..."
if cargo build --release --manifest-path "$ROOT/Cargo.toml" 2>/dev/null; then
    echo "[rilua]       Built"
    build_ok+=(rilua)
else
    echo "[rilua]       BUILD FAILED"
fi

# mlua runner
echo "[mlua]        Building (vendored Lua 5.1)..."
if cargo build --release --manifest-path "$ROOT/scripts/mlua-runner/Cargo.toml" 2>/dev/null; then
    echo "[mlua]        Built"
    build_ok+=(mlua)
else
    echo "[mlua]        BUILD FAILED"
fi

# lua-rs (requires nightly)
if [ -d "$LUARS_ROOT" ]; then
    echo "[lua-rs]      Building (nightly)..."
    if rustup run nightly cargo build --release --manifest-path "$LUARS_ROOT/Cargo.toml" -p luars_interpreter 2>/dev/null; then
        echo "[lua-rs]      Built"
        build_ok+=(lua_rs)
    else
        echo "[lua-rs]      BUILD FAILED (requires nightly Rust)"
    fi
else
    echo "[lua-rs]      Source not found at $LUARS_ROOT"
fi

# lua-in-rust
if [ -d "$LIR_ROOT" ]; then
    echo "[lua-in-rust] Building..."
    if cargo build --release --manifest-path "$LIR_ROOT/Cargo.toml" 2>/dev/null; then
        echo "[lua-in-rust] Built"
        build_ok+=(lua_in_rust)
    else
        echo "[lua-in-rust] BUILD FAILED"
    fi
else
    echo "[lua-in-rust] Source not found at $LIR_ROOT"
fi

echo ""
echo "Implementations built: ${build_ok[*]}"
echo ""

if [ ${#build_ok[@]} -lt 2 ]; then
    echo "ERROR: Need at least 2 implementations to compare."
    exit 1
fi

# ---- Phase 2: System info ----

echo "=== System Information ==="
echo "Date:     $(date -u +%Y-%m-%dT%H:%M:%SZ)"
echo "CPU:      $(lscpu 2>/dev/null | grep 'Model name' | sed 's/.*: *//' || echo 'unknown')"
echo "OS:       $(uname -sr)"
echo "Rust:     $(rustc --version 2>/dev/null || echo 'unknown')"
echo "Runs:     $RUNS per test (plus 1 warmup)"
echo ""

# Initialize CSV
echo "implementation,test_suite,test,run,time_ms" > "$CSVFILE"

# ---- Phase 3: Compatibility probe ----

echo "=== Compatibility Probe ==="
echo "Testing which implementations can run each test (60s timeout)..."
echo ""

declare -A COMPAT

# Probe PUC-Rio test suite
if [ -d "$TESTDIR" ]; then
    pushd "$TESTDIR" >/dev/null

    for impl in "${build_ok[@]}"; do
        bin=$(impl_bin "$impl")
        env_prefix=$(impl_env "$impl")
        pass=0
        fail=0
        for test in "${SUITE_TESTS[@]}"; do
            if [ -f "$test" ] && probe "$bin" "$test" "$env_prefix"; then
                COMPAT["${impl}:suite:${test}"]="PASS"
                ((pass++)) || true
            else
                COMPAT["${impl}:suite:${test}"]="FAIL"
                ((fail++)) || true
            fi
        done
        label="${IMPL_LABELS[$impl]:-$impl}"
        echo "  $label: ${pass}/${#SUITE_TESTS[@]} suite tests pass"
    done

    # Probe combined runners
    for impl in "${build_ok[@]}"; do
        bin=$(impl_bin "$impl")
        env_prefix=$(impl_env "$impl")
        if [ -f "bench-all.lua" ] && probe "$bin" "bench-all.lua" "$env_prefix"; then
            COMPAT["${impl}:suite:bench-all.lua"]="PASS"
        elif [ -f "all.lua" ] && probe "$bin" "all.lua" "$env_prefix"; then
            COMPAT["${impl}:suite:all.lua"]="PASS"
        fi
    done

    popd >/dev/null
else
    echo "  WARNING: Test suite not found at $TESTDIR"
    echo "  Download: see AGENTS.md for setup instructions"
    echo "  Skipping PUC-Rio test suite benchmarks."
fi

echo ""

# Probe micro-benchmarks
for impl in "${build_ok[@]}"; do
    bin=$(impl_bin "$impl")
    env_prefix=$(impl_env "$impl")
    pass=0
    fail=0
    for test in "${MICRO_TESTS[@]}"; do
        if probe "$bin" "$BENCHDIR/$test" "$env_prefix"; then
            COMPAT["${impl}:micro:${test}"]="PASS"
            ((pass++)) || true
        else
            COMPAT["${impl}:micro:${test}"]="FAIL"
            ((fail++)) || true
        fi
    done
    label="${IMPL_LABELS[$impl]:-$impl}"
    echo "  $label: ${pass}/${#MICRO_TESTS[@]} micro-benchmarks pass"
done

echo ""

# ---- Print compatibility matrix ----

echo "=== Compatibility Matrix ==="
echo ""

# Header
printf "%-20s" "Test"
for impl in "${build_ok[@]}"; do
    printf " %12s" "${IMPL_LABELS[$impl]:-$impl}"
done
echo ""
printf "%-20s" "--------------------"
for impl in "${build_ok[@]}"; do
    printf " %12s" "------------"
done
echo ""

# Suite tests
if [ -d "$TESTDIR" ]; then
    for test in "${SUITE_TESTS[@]}"; do
        printf "%-20s" "$test"
        for impl in "${build_ok[@]}"; do
            status="${COMPAT["${impl}:suite:${test}"]:-SKIP}"
            printf " %12s" "$status"
        done
        echo ""
    done
fi

# Micro tests
echo ""
printf "%-20s" "--- Micro ---"
for impl in "${build_ok[@]}"; do
    printf " %12s" ""
done
echo ""

for test in "${MICRO_TESTS[@]}"; do
    printf "%-20s" "$test"
    for impl in "${build_ok[@]}"; do
        status="${COMPAT["${impl}:micro:${test}"]:-SKIP}"
        printf " %12s" "$status"
    done
    echo ""
done
echo ""

# ---- Phase 4: Benchmark PUC-Rio test suite ----

# Find implementations that pass at least one suite test
suite_impls=()
for impl in "${build_ok[@]}"; do
    for test in "${SUITE_TESTS[@]}"; do
        if [ "${COMPAT["${impl}:suite:${test}"]:-FAIL}" = "PASS" ]; then
            suite_impls+=("$impl")
            break
        fi
    done
done

if [ ${#suite_impls[@]} -ge 1 ] && [ -d "$TESTDIR" ]; then
    echo "=== PUC-Rio Test Suite Benchmark ==="
    echo ""

    # Header
    printf "%-20s" "Test"
    for impl in "${suite_impls[@]}"; do
        printf " %12s" "${IMPL_LABELS[$impl]} (ms)"
    done
    # Ratio columns vs first impl
    if [ ${#suite_impls[@]} -ge 2 ]; then
        for ((idx=1; idx<${#suite_impls[@]}; idx++)); do
            printf " %12s" "ratio"
        done
    fi
    echo ""

    printf "%-20s" "--------------------"
    for impl in "${suite_impls[@]}"; do
        printf " %12s" "------------"
    done
    if [ ${#suite_impls[@]} -ge 2 ]; then
        for ((idx=1; idx<${#suite_impls[@]}; idx++)); do
            printf " %12s" "------------"
        done
    fi
    echo ""

    pushd "$TESTDIR" >/dev/null

    # Totals
    declare -A SUITE_TOTALS
    for impl in "${suite_impls[@]}"; do
        SUITE_TOTALS[$impl]=0
    done

    for test in "${SUITE_TESTS[@]}"; do
        printf "%-20s" "$test"

        declare -A test_times
        for impl in "${suite_impls[@]}"; do
            if [ "${COMPAT["${impl}:suite:${test}"]:-FAIL}" = "PASS" ]; then
                bin=$(impl_bin "$impl")
                env_prefix=$(impl_env "$impl")
                t=$(bench "$impl" "suite" "$bin" "$test" "$env_prefix")
                test_times[$impl]=$t
                SUITE_TOTALS[$impl]=$(( ${SUITE_TOTALS[$impl]} + t ))
            else
                test_times[$impl]=-1
            fi
            if [ "${test_times[$impl]}" -ge 0 ]; then
                printf " %12d" "${test_times[$impl]}"
            else
                printf " %12s" "---"
            fi
        done

        # Ratios
        if [ ${#suite_impls[@]} -ge 2 ]; then
            base_t=${test_times[${suite_impls[0]}]}
            for ((idx=1; idx<${#suite_impls[@]}; idx++)); do
                other_t=${test_times[${suite_impls[$idx]}]}
                if [ "$base_t" -gt 0 ] && [ "$other_t" -ge 0 ]; then
                    ratio=$(awk "BEGIN {printf \"%.2fx\", $other_t/$base_t}")
                    printf " %12s" "$ratio"
                else
                    printf " %12s" "---"
                fi
            done
        fi
        unset test_times
        echo ""
    done

    echo ""

    # Combined runner
    combined_file=""
    if [ -f "bench-all.lua" ]; then
        combined_file="bench-all.lua"
    elif [ -f "all.lua" ]; then
        combined_file="all.lua"
    fi

    if [ -n "$combined_file" ]; then
        printf "%-20s" "$combined_file"
        declare -A combined_times
        for impl in "${suite_impls[@]}"; do
            if [ "${COMPAT["${impl}:suite:${combined_file}"]:-FAIL}" = "PASS" ]; then
                bin=$(impl_bin "$impl")
                env_prefix=$(impl_env "$impl")
                t=$(bench "$impl" "suite" "$bin" "$combined_file" "$env_prefix")
                combined_times[$impl]=$t
            else
                combined_times[$impl]=-1
            fi
            if [ "${combined_times[$impl]}" -ge 0 ]; then
                printf " %12d" "${combined_times[$impl]}"
            else
                printf " %12s" "---"
            fi
        done
        if [ ${#suite_impls[@]} -ge 2 ]; then
            base_t=${combined_times[${suite_impls[0]}]}
            for ((idx=1; idx<${#suite_impls[@]}; idx++)); do
                other_t=${combined_times[${suite_impls[$idx]}]}
                if [ "$base_t" -gt 0 ] && [ "$other_t" -ge 0 ]; then
                    ratio=$(awk "BEGIN {printf \"%.2fx\", $other_t/$base_t}")
                    printf " %12s" "$ratio"
                else
                    printf " %12s" "---"
                fi
            done
        fi
        unset combined_times
        echo ""
    fi

    echo ""

    # Sum row
    printf "%-20s" "Sum (individual)"
    for impl in "${suite_impls[@]}"; do
        printf " %12d" "${SUITE_TOTALS[$impl]}"
    done
    if [ ${#suite_impls[@]} -ge 2 ]; then
        base_total=${SUITE_TOTALS[${suite_impls[0]}]}
        for ((idx=1; idx<${#suite_impls[@]}; idx++)); do
            other_total=${SUITE_TOTALS[${suite_impls[$idx]}]}
            if [ "$base_total" -gt 0 ]; then
                ratio=$(awk "BEGIN {printf \"%.2fx\", $other_total/$base_total}")
                printf " %12s" "$ratio"
            else
                printf " %12s" "---"
            fi
        done
    fi
    echo ""
    echo ""

    popd >/dev/null
fi

# ---- Phase 5: Micro-benchmarks ----

# Find implementations that pass at least one micro-benchmark
micro_impls=()
for impl in "${build_ok[@]}"; do
    for test in "${MICRO_TESTS[@]}"; do
        if [ "${COMPAT["${impl}:micro:${test}"]:-FAIL}" = "PASS" ]; then
            micro_impls+=("$impl")
            break
        fi
    done
done

if [ ${#micro_impls[@]} -ge 1 ]; then
    echo "=== Micro-Benchmark Results ==="
    echo ""

    # Header
    printf "%-20s" "Test"
    for impl in "${micro_impls[@]}"; do
        printf " %12s" "${IMPL_LABELS[$impl]} (ms)"
    done
    if [ ${#micro_impls[@]} -ge 2 ]; then
        for ((idx=1; idx<${#micro_impls[@]}; idx++)); do
            printf " %12s" "ratio"
        done
    fi
    echo ""

    printf "%-20s" "--------------------"
    for impl in "${micro_impls[@]}"; do
        printf " %12s" "------------"
    done
    if [ ${#micro_impls[@]} -ge 2 ]; then
        for ((idx=1; idx<${#micro_impls[@]}; idx++)); do
            printf " %12s" "------------"
        done
    fi
    echo ""

    for test in "${MICRO_TESTS[@]}"; do
        printf "%-20s" "$test"

        declare -A micro_times
        for impl in "${micro_impls[@]}"; do
            if [ "${COMPAT["${impl}:micro:${test}"]:-FAIL}" = "PASS" ]; then
                bin=$(impl_bin "$impl")
                env_prefix=$(impl_env "$impl")
                t=$(bench "$impl" "micro" "$bin" "$BENCHDIR/$test" "$env_prefix")
                micro_times[$impl]=$t
            else
                micro_times[$impl]=-1
            fi
            if [ "${micro_times[$impl]}" -ge 0 ]; then
                printf " %12d" "${micro_times[$impl]}"
            else
                printf " %12s" "---"
            fi
        done

        # Ratios vs first impl
        if [ ${#micro_impls[@]} -ge 2 ]; then
            base_t=${micro_times[${micro_impls[0]}]}
            for ((idx=1; idx<${#micro_impls[@]}; idx++)); do
                other_t=${micro_times[${micro_impls[$idx]}]}
                if [ "$base_t" -gt 0 ] && [ "$other_t" -ge 0 ]; then
                    ratio=$(awk "BEGIN {printf \"%.2fx\", $other_t/$base_t}")
                    printf " %12s" "$ratio"
                else
                    printf " %12s" "---"
                fi
            done
        fi
        unset micro_times
        echo ""
    done

    echo ""
fi

echo "=== Done ==="
echo "Raw timing data: $CSVFILE"
echo "Ratio = implementation / ${IMPL_LABELS[${build_ok[0]}]} (lower is better, 1.00x = parity)"
