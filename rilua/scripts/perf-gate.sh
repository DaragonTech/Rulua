#!/bin/bash
# Performance regression gate for rilua.
#
# Compares the current build against a known baseline median time.
# Fails if the current median exceeds the baseline by more than
# the allowed regression threshold.
#
# Usage:
#   ./scripts/perf-gate.sh [baseline_ms] [threshold_pct]
#
# Arguments:
#   baseline_ms    Baseline median in milliseconds (default: read from .perf-baseline)
#   threshold_pct  Allowed regression percentage (default: 5)
#
# The baseline file (.perf-baseline) contains a single integer: the median
# time in milliseconds from the last accepted benchmark run.
#
# Exit code 0 if within threshold, 1 if regression detected.

set -euo pipefail

BASELINE_FILE=".perf-baseline"
THRESHOLD="${2:-5}"

if [ -n "${1:-}" ]; then
    BASELINE="$1"
elif [ -f "$BASELINE_FILE" ]; then
    BASELINE=$(cat "$BASELINE_FILE")
else
    echo "Error: no baseline provided and $BASELINE_FILE not found." >&2
    echo "Run: ./scripts/bench-puc-rio.sh > $BASELINE_FILE" >&2
    exit 1
fi

echo "Baseline: ${BASELINE}ms  Threshold: ${THRESHOLD}%"

# Build release
cargo build --release 2>&1 | tail -1

# Run benchmark
CURRENT=$(./scripts/bench-puc-rio.sh target/release/rilua 5)
echo "Current median: ${CURRENT}ms"

# Calculate regression
MAX_ALLOWED=$(( BASELINE + BASELINE * THRESHOLD / 100 ))
if [ "$CURRENT" -gt "$MAX_ALLOWED" ]; then
    REGRESSION=$(( (CURRENT - BASELINE) * 100 / BASELINE ))
    echo "FAIL: ${REGRESSION}% regression detected (${CURRENT}ms > ${MAX_ALLOWED}ms limit)" >&2
    exit 1
fi

IMPROVEMENT=$(( (BASELINE - CURRENT) * 100 / BASELINE ))
if [ "$IMPROVEMENT" -gt 0 ]; then
    echo "PASS: ${IMPROVEMENT}% faster than baseline"
else
    echo "PASS: within ${THRESHOLD}% threshold"
fi
