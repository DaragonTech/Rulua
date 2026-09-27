#!/usr/bin/env bash
# Benchmark PUC-Rio Lua 5.1.1 and rilua against the PUC-Rio test suite.
# Usage: ./scripts/benchmark-tests.sh [runs]
# Default: 10 runs per test, reports median in milliseconds.

set -euo pipefail

RUNS="${1:-10}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PUCRIO="$ROOT/lua-5.1.1/src/lua"
RILUA="$ROOT/target/release/rilua"
TESTDIR="$ROOT/lua-5.1-tests"

# Tests from all.lua that can run standalone.
# Excluded: main.lua (CLI tests, needs special invocation),
#           big.lua (requires coroutine wrapper from all.lua).
TESTS=(
    gc.lua
    db.lua
    calls.lua
    strings.lua
    literals.lua
    attrib.lua
    locals.lua
    constructs.lua
    code.lua
    nextvar.lua
    pm.lua
    api.lua
    events.lua
    vararg.lua
    closure.lua
    errors.lua
    math.lua
    sort.lua
    verybig.lua
    files.lua
)

# Compute median from a list of integers (one per line)
median() {
    sort -n | awk '{a[NR]=$1} END {
        if (NR%2==1) printf "%d\n", a[(NR+1)/2]
        else printf "%d\n", (a[NR/2]+a[NR/2+1])/2
    }'
}

# Time a single lua invocation (ms), returns time on stdout
time_lua() {
    local lua="$1"
    local script="$2"
    local start end_t
    start=$(date +%s%N)
    "$lua" "$script" >/dev/null 2>&1 || true
    end_t=$(date +%s%N)
    echo $(( (end_t - start) / 1000000 ))
}

# Run benchmark for one interpreter + one script, N times, return median
bench() {
    local lua="$1"
    local script="$2"
    local times=()
    for ((i=1; i<=RUNS; i++)); do
        times+=("$(time_lua "$lua" "$script")")
    done
    printf '%s\n' "${times[@]}" | median
}

# Header
echo "=== Lua Test Suite Benchmark ==="
echo "Runs per test: $RUNS"
echo "PUC-Rio Lua:   $($PUCRIO -v 2>&1)"
echo "rilua:         $($RILUA -v 2>&1 || echo 'rilua')"
echo "Date:          $(date -u +%Y-%m-%dT%H:%M:%SZ)"
echo "CPU:           $(lscpu | grep 'Model name' | sed 's/.*: *//')"
echo ""

# Print table header
printf "%-20s %12s %12s %10s\n" "Test" "PUC-Rio (ms)" "rilua (ms)" "Ratio"
printf "%-20s %12s %12s %10s\n" "--------------------" "------------" "----------" "-----"

cd "$TESTDIR"

pucrio_total=0
rilua_total=0

# Benchmark individual tests
for test in "${TESTS[@]}"; do
    puc=$(bench "$PUCRIO" "$test")
    ri=$(bench "$RILUA" "$test")
    if [ "$puc" -gt 0 ]; then
        ratio=$(awk "BEGIN {printf \"%.2fx\", $ri/$puc}")
    else
        ratio="N/A"
    fi
    printf "%-20s %12d %12d %10s\n" "$test" "$puc" "$ri" "$ratio"
    pucrio_total=$((pucrio_total + puc))
    rilua_total=$((rilua_total + ri))
done

echo ""

# Benchmark bench-all.lua (combined runner)
if [ -f bench-all.lua ]; then
    puc=$(bench "$PUCRIO" "bench-all.lua")
    ri=$(bench "$RILUA" "bench-all.lua")
    if [ "$puc" -gt 0 ]; then
        ratio=$(awk "BEGIN {printf \"%.2fx\", $ri/$puc}")
    else
        ratio="N/A"
    fi
    printf "%-20s %12d %12d %10s\n" "bench-all.lua" "$puc" "$ri" "$ratio"
fi

echo ""
if [ "$pucrio_total" -gt 0 ]; then
    sum_ratio=$(awk "BEGIN {printf \"%.2fx\", $rilua_total/$pucrio_total}")
else
    sum_ratio="N/A"
fi
printf "%-20s %12d %12d %10s\n" "Sum (individual)" "$pucrio_total" "$rilua_total" "$sum_ratio"
echo ""
echo "Ratio = rilua / PUC-Rio (lower is better, 1.00x = parity)"
