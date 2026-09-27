#!/usr/bin/env bash
#
# compare.sh - Run PUC-Rio Lua 5.1 test suite files through both
# PUC-Rio Lua and rilua, producing a markdown comparison table.
#
# Usage: scripts/compare.sh <lua-path> <rilua-path>
#
# Paths can be relative (resolved to absolute before cd).
# PUC-Rio test files are run from within lua-5.1-tests/ since
# they use relative dofile() calls internally.
#
# Always exits 0 (informational only).

set -euo pipefail

if [ $# -ne 2 ]; then
    echo "Usage: $0 <lua-path> <rilua-path>" >&2
    exit 1
fi

# Resolve to absolute paths before cd-ing into test directory.
LUA="$(readlink -f "$1")"
RILUA="$(readlink -f "$2")"

if [ ! -x "$LUA" ]; then
    echo "Error: Lua binary not found or not executable: $LUA" >&2
    exit 1
fi

if [ ! -x "$RILUA" ]; then
    echo "Error: rilua binary not found or not executable: $RILUA" >&2
    exit 1
fi

TEST_DIR="lua-5.1-tests"
if [ ! -d "$TEST_DIR" ]; then
    echo "Error: test suite directory not found: $TEST_DIR" >&2
    echo "See AGENTS.md for download instructions." >&2
    exit 1
fi

# Run a single test file with an interpreter, return status string.
# Must be called from within the test suite directory.
# Arguments: <interpreter-path> <test-file-basename>
# Output: "PASS", "FAIL", or "TIMEOUT"
run_test() {
    local interp="$1"
    local test_file="$2"

    if timeout 10s "$interp" "$test_file" >/dev/null 2>&1; then
        echo "PASS"
    else
        local rc=$?
        if [ "$rc" -eq 124 ]; then
            echo "TIMEOUT"
        else
            echo "FAIL"
        fi
    fi
}

# Print a markdown table of results.
# Arguments: <heading> <test-files...>
# Uses LUA and RILUA from outer scope.
print_table() {
    local heading="$1"
    shift
    local files=("$@")

    echo "### $heading"
    echo ""
    echo "| Test | PUC-Rio Lua | rilua | Match |"
    echo "|------|-------------|-------|-------|"

    local pass_count=0
    local total=0

    for test_file in "${files[@]}"; do
        local name
        name="$(basename "$test_file" .lua)"

        local lua_result
        lua_result="$(run_test "$LUA" "$test_file")"

        local rilua_result
        rilua_result="$(run_test "$RILUA" "$test_file")"

        local match="no"
        if [ "$lua_result" = "$rilua_result" ]; then
            match="yes"
        fi

        # Count rilua passes
        if [ "$rilua_result" = "PASS" ]; then
            pass_count=$((pass_count + 1))
        fi
        total=$((total + 1))

        echo "| $name | $lua_result | $rilua_result | $match |"
    done

    echo ""

    # Return counts via global variables (bash workaround)
    _pass_count=$pass_count
    _total_count=$total
}

# cd into test suite directory so relative dofile() calls work.
cd "$TEST_DIR"

# Collect test files (skip all.lua which is the runner, not a test).
lua51_tests=()
for f in *.lua; do
    [ "$f" = "all.lua" ] && continue
    [ -f "$f" ] && lua51_tests+=("$f")
done

echo "## Compatibility: rilua vs PUC-Rio Lua 5.1.1"
echo ""

# PUC-Rio official tests
_pass_count=0
_total_count=0

print_table "PUC-Rio Lua 5.1.1 Official Tests" "${lua51_tests[@]}"
lua51_pass=$_pass_count
lua51_total=$_total_count

# Summary
echo "### Summary"
echo ""
echo "- PUC-Rio tests: $lua51_pass/$lua51_total passing (rilua)"

exit 0
