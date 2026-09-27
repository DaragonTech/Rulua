#!/bin/bash
# Benchmark rilua against the PUC-Rio Lua 5.1 test suite.
#
# Usage:
#   ./scripts/bench-puc-rio.sh [binary] [runs]
#
# Arguments:
#   binary  Path to the rilua binary (default: target/release/rilua)
#   runs    Number of runs (default: 5)
#
# Output:
#   Median wall-clock time in milliseconds.
#   Exit code 0 on success, 1 if the test suite fails.

set -euo pipefail

BINARY="${1:-target/release/rilua}"
RUNS="${2:-5}"
TESTDIR="lua-5.1-tests"

if [ ! -x "$BINARY" ]; then
    echo "Error: binary not found or not executable: $BINARY" >&2
    echo "Build with: cargo build --release" >&2
    exit 1
fi

if [ ! -f "$TESTDIR/all.lua" ]; then
    echo "Error: test suite not found at $TESTDIR/all.lua" >&2
    exit 1
fi

times=()
for i in $(seq 1 "$RUNS"); do
    start=$(date +%s%N)
    if ! (cd "$TESTDIR" && RILUA_TEST_LIB=1 "../$BINARY" all.lua > /dev/null 2>&1); then
        echo "Error: test suite failed on run $i" >&2
        exit 1
    fi
    end=$(date +%s%N)
    elapsed_ms=$(( (end - start) / 1000000 ))
    times+=("$elapsed_ms")
    echo "Run $i: ${elapsed_ms}ms" >&2
done

# Sort and pick median
IFS=$'\n' sorted=($(sort -n <<<"${times[*]}")); unset IFS
median_idx=$(( RUNS / 2 ))
median=${sorted[$median_idx]}
min=${sorted[0]}
max=${sorted[$((RUNS - 1))]}

echo "Min: ${min}ms  Median: ${median}ms  Max: ${max}ms" >&2
echo "$median"
