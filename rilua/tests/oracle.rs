//! Oracle comparison tests: verify PUC-Rio Lua 5.1.1 reference binary
//! integration and establish baseline for future rilua comparisons.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::items_after_statements
)]

#[allow(dead_code, unreachable_pub)]
mod helpers;

use helpers::oracle;

#[test]
fn reference_binary_exists() {
    if !oracle::reference_available() {
        eprintln!("skipping: reference Lua binary not available");
        return;
    }
    let bin = oracle::reference_bin();
    assert!(
        bin.exists(),
        "reference binary should exist at {}",
        bin.display()
    );
}

#[test]
fn reference_print_hello() {
    let Some(result) = oracle::run_reference("print('hello')") else {
        eprintln!("skipping: reference Lua binary not available");
        return;
    };
    assert_eq!(result.exit_code, 0);
    assert_eq!(result.stdout, "hello\n");
    assert!(result.stderr.is_empty());
}

#[test]
fn reference_arithmetic() {
    oracle::assert_reference_output("print(1 + 2)", "3\n");
}

#[test]
fn reference_string_concat() {
    oracle::assert_reference_output("print('foo' .. 'bar')", "foobar\n");
}

#[test]
fn reference_multiple_values() {
    oracle::assert_reference_output("print(1, 2, 3)", "1\t2\t3\n");
}

#[test]
fn reference_tostring_coercion() {
    oracle::assert_reference_output("print(tostring(42))", "42\n");
}

#[test]
fn reference_type_function() {
    oracle::assert_reference_output("print(type(nil))", "nil\n");
}

#[test]
fn reference_syntax_error() {
    let Some(result) = oracle::run_reference("if then") else {
        eprintln!("skipping: reference Lua binary not available");
        return;
    };
    assert_ne!(
        result.exit_code, 0,
        "syntax error should produce non-zero exit"
    );
    assert!(
        result.stderr.contains("'then'") || result.stderr.contains("syntax"),
        "stderr should mention the syntax error: {}",
        result.stderr
    );
}

#[test]
fn reference_runtime_error() {
    let Some(result) = oracle::run_reference("error('boom')") else {
        eprintln!("skipping: reference Lua binary not available");
        return;
    };
    assert_ne!(
        result.exit_code, 0,
        "runtime error should produce non-zero exit"
    );
    assert!(
        result.stderr.contains("boom"),
        "stderr should contain error message: {}",
        result.stderr
    );
}

#[test]
fn reference_version_string() {
    let Some(result) = oracle::run_reference("print(_VERSION)") else {
        eprintln!("skipping: reference Lua binary not available");
        return;
    };
    assert_eq!(result.exit_code, 0);
    assert_eq!(result.stdout, "Lua 5.1\n");
}

// ---------------------------------------------------------------------------
// Oracle comparison tests: rilua vs PUC-Rio
// ---------------------------------------------------------------------------

#[test]
fn oracle_print_hello() {
    oracle::assert_matches_reference("print('hello')");
}

#[test]
fn oracle_arithmetic() {
    oracle::assert_matches_reference("print(1 + 2)");
}

#[test]
fn oracle_multiple_values() {
    oracle::assert_matches_reference("print(1, 2, 3)");
}

#[test]
fn oracle_print_nil() {
    oracle::assert_matches_reference("print(nil)");
}

#[test]
fn oracle_print_bool() {
    oracle::assert_matches_reference("print(true, false)");
}

#[test]
fn oracle_print_no_args() {
    oracle::assert_matches_reference("print()");
}

#[test]
fn oracle_variable_assignment() {
    oracle::assert_matches_reference("x = 42 print(x)");
}

#[test]
fn oracle_print_negative() {
    oracle::assert_matches_reference("print(-5)");
}

#[test]
fn oracle_print_float() {
    oracle::assert_matches_reference("print(3.14)");
}

// ---------------------------------------------------------------------------
// Phase 4 oracle comparison tests
// ---------------------------------------------------------------------------

#[test]
fn oracle_type_function() {
    oracle::assert_matches_reference(
        "print(type(1), type('s'), type(nil), type(true), type({}), type(print))",
    );
}

#[test]
fn oracle_tostring() {
    oracle::assert_matches_reference("print(tostring(42), tostring(nil), tostring(true))");
}

#[test]
fn oracle_tonumber() {
    oracle::assert_matches_reference("print(tonumber('42'), tonumber('ff', 16))");
}

#[test]
fn oracle_pcall_success() {
    oracle::assert_matches_reference("print(pcall(function() return 1, 2, 3 end))");
}

#[test]
fn oracle_pcall_error() {
    oracle::assert_matches_reference("print(pcall(function() error('boom') end))");
}

#[test]
fn oracle_xpcall() {
    oracle::assert_matches_reference(
        "print(xpcall(function() error('boom') end, function(e) return 'caught: ' .. e end))",
    );
}

#[test]
fn oracle_select_count() {
    oracle::assert_matches_reference("print(select('#', 1, 2, 3))");
}

#[test]
fn oracle_select_range() {
    oracle::assert_matches_reference("print(select(2, 'a', 'b', 'c'))");
}

#[test]
fn oracle_unpack() {
    oracle::assert_matches_reference("print(unpack({10, 20, 30}))");
}

#[test]
fn oracle_rawequal() {
    oracle::assert_matches_reference("print(rawequal(1, 1), rawequal(1, 2))");
}

#[test]
fn oracle_assert_success() {
    oracle::assert_matches_reference("print(assert(42, 'msg'))");
}

#[test]
fn oracle_metamethod_add() {
    oracle::assert_matches_reference(
        "local t = setmetatable({}, {__add = function(a,b) return 42 end}); print(t + 1)",
    );
}

#[test]
fn oracle_metamethod_index() {
    oracle::assert_matches_reference(
        "local t = setmetatable({}, {__index = function(t,k) return k end}); print(t.hello)",
    );
}

#[test]
fn oracle_metamethod_call() {
    oracle::assert_matches_reference(
        "local t = setmetatable({}, {__call = function(self, a, b) return a + b end}); print(t(3, 4))",
    );
}

#[test]
fn oracle_metamethod_len_table_ignores() {
    // In Lua 5.1.1, __len is NOT called for tables (only for userdata).
    oracle::assert_matches_reference(
        "local t = setmetatable({1, 2, 3}, {__len = function(self) return 99 end}); print(#t)",
    );
}

#[test]
fn oracle_metamethod_unm() {
    oracle::assert_matches_reference(
        "local t = setmetatable({}, {__unm = function(self) return 'negated' end}); print(-t)",
    );
}

#[test]
fn oracle_metamethod_concat() {
    oracle::assert_matches_reference(
        "local t = setmetatable({}, {__concat = function(a,b) return 'joined' end}); print(t .. 'x')",
    );
}

#[test]
fn oracle_nested_pcall() {
    oracle::assert_matches_reference(
        "print(pcall(function() return pcall(function() error('inner') end) end))",
    );
}

#[test]
fn oracle_error_non_string() {
    oracle::assert_matches_reference("print(pcall(function() error(42) end))");
}

#[test]
fn oracle_pcall_no_error() {
    oracle::assert_matches_reference("print(pcall(print, 'hello'))");
}

// ---------------------------------------------------------------------------
// Phase 5a oracle comparison tests
// ---------------------------------------------------------------------------

#[test]
fn oracle_version() {
    oracle::assert_matches_reference("print(_VERSION)");
}

#[test]
fn oracle_g_type() {
    oracle::assert_matches_reference("print(type(_G))");
}

#[test]
fn oracle_g_self_ref() {
    oracle::assert_matches_reference("print(_G == _G)");
}

#[test]
fn oracle_next_basic() {
    // next on single-element table.
    oracle::assert_matches_reference("local k, v = next({x=1}) print(k, v)");
}

#[test]
fn oracle_next_empty() {
    oracle::assert_matches_reference("print(next({}))");
}

#[test]
fn oracle_ipairs() {
    oracle::assert_matches_reference("for i, v in ipairs({10, 20, 30}) do print(i, v) end");
}

#[test]
fn oracle_ipairs_stops_at_nil() {
    oracle::assert_matches_reference(
        "local t = {1, 2, nil, 4} local c = 0 for i, v in ipairs(t) do c = c + 1 end print(c)",
    );
}

#[test]
fn oracle_pairs_sum() {
    oracle::assert_matches_reference(
        "local s = 0 for k, v in pairs({a=1, b=2, c=3}) do s = s + v end print(s)",
    );
}

#[test]
fn oracle_loadstring_success() {
    oracle::assert_matches_reference("local f = loadstring('return 1+2') print(f())");
}

#[test]
fn oracle_loadstring_nil_on_error() {
    oracle::assert_matches_reference("local f, err = loadstring('if then') print(f, type(err))");
}

#[test]
fn oracle_collectgarbage_step() {
    oracle::assert_matches_reference("print(collectgarbage('step'))");
}

#[test]
fn oracle_getfenv_zero() {
    oracle::assert_matches_reference("print(getfenv(0) == _G)");
}

#[test]
fn oracle_setfenv_function() {
    oracle::assert_matches_reference(
        "local f = function() return x end local env = {x = 42} setfenv(f, env) print(f())",
    );
}

#[test]
fn oracle_load_function() {
    oracle::assert_matches_reference(
        "local i = 0 local chunks = {'ret', 'urn ', '42'} local f = load(function() i = i + 1 return chunks[i] end) print(f())",
    );
}

// ---------------------------------------------------------------------------
// String library oracle tests
// ---------------------------------------------------------------------------

#[test]
fn oracle_string_len() {
    oracle::assert_matches_reference("print(string.len('hello'))");
}

#[test]
fn oracle_string_len_empty() {
    oracle::assert_matches_reference("print(string.len(''))");
}

#[test]
fn oracle_string_byte() {
    oracle::assert_matches_reference("print(string.byte('A'))");
}

#[test]
fn oracle_string_byte_range() {
    oracle::assert_matches_reference("print(string.byte('abc', 1, 3))");
}

#[test]
fn oracle_string_char() {
    oracle::assert_matches_reference("print(string.char(72, 101, 108, 108, 111))");
}

#[test]
fn oracle_string_sub() {
    oracle::assert_matches_reference("print(string.sub('hello', 2, 4))");
}

#[test]
fn oracle_string_sub_negative() {
    oracle::assert_matches_reference("print(string.sub('hello', -3))");
}

#[test]
fn oracle_string_rep() {
    oracle::assert_matches_reference("print(string.rep('ab', 3))");
}

#[test]
fn oracle_string_rep_zero() {
    oracle::assert_matches_reference("print(string.rep('xy', 0))");
}

#[test]
fn oracle_string_reverse() {
    oracle::assert_matches_reference("print(string.reverse('hello'))");
}

#[test]
fn oracle_string_lower() {
    oracle::assert_matches_reference("print(string.lower('Hello World'))");
}

#[test]
fn oracle_string_upper() {
    oracle::assert_matches_reference("print(string.upper('Hello World'))");
}

#[test]
fn oracle_string_format_d() {
    oracle::assert_matches_reference("print(string.format('%d %s', 42, 'hi'))");
}

#[test]
fn oracle_string_format_x() {
    oracle::assert_matches_reference("print(string.format('%x', 255))");
}

#[test]
fn oracle_string_format_f() {
    oracle::assert_matches_reference("print(string.format('%f', 3.14))");
}

#[test]
fn oracle_string_format_g() {
    oracle::assert_matches_reference("print(string.format('%g', 100000))");
}

#[test]
fn oracle_string_format_g_sci() {
    oracle::assert_matches_reference("print(string.format('%g', 1e10))");
}

#[test]
fn oracle_string_format_q() {
    oracle::assert_matches_reference(r#"print(string.format('%q', 'hello "world"'))"#);
}

#[test]
fn oracle_string_format_percent() {
    oracle::assert_matches_reference("print(string.format('100%%'))");
}

#[test]
fn oracle_string_find_plain() {
    oracle::assert_matches_reference("print(string.find('hello world', 'world'))");
}

#[test]
fn oracle_string_find_pattern() {
    oracle::assert_matches_reference("print(string.find('hello123', '%d+'))");
}

#[test]
fn oracle_string_find_not_found() {
    oracle::assert_matches_reference("print(string.find('hello', 'xyz'))");
}

#[test]
fn oracle_string_find_captures() {
    oracle::assert_matches_reference("print(string.find('hello world', '(w%a+)'))");
}

#[test]
fn oracle_string_find_anchor() {
    oracle::assert_matches_reference("print(string.find('hello', '^hel'))");
}

#[test]
fn oracle_string_match_captures() {
    oracle::assert_matches_reference("print(string.match('2024-01-15', '(%d+)-(%d+)-(%d+)'))");
}

#[test]
fn oracle_string_match_no_capture() {
    oracle::assert_matches_reference("print(string.match('hello', '%a+'))");
}

#[test]
fn oracle_string_match_empty_capture() {
    oracle::assert_matches_reference("print(string.match('hello', '()h'))");
}

#[test]
fn oracle_string_gmatch() {
    oracle::assert_matches_reference(
        "local t = {} for w in string.gmatch('hello world foo', '%a+') do t[#t+1] = w end print(t[1], t[2], t[3])",
    );
}

#[test]
fn oracle_string_gsub_basic() {
    oracle::assert_matches_reference("print(string.gsub('hello', 'l', 'L'))");
}

#[test]
fn oracle_string_gsub_limit() {
    oracle::assert_matches_reference("print(string.gsub('hello', 'l', 'L', 1))");
}

#[test]
fn oracle_string_gsub_pattern() {
    oracle::assert_matches_reference("print(string.gsub('abc123', '%d', '*'))");
}

#[test]
fn oracle_string_gsub_function() {
    oracle::assert_matches_reference(
        "print(string.gsub('abc', '%a', function(c) return string.upper(c) end))",
    );
}

#[test]
fn oracle_string_gsub_table() {
    oracle::assert_matches_reference("local t = {a='A', b='B'} print(string.gsub('aXb', '%a', t))");
}

#[test]
fn oracle_string_method_upper() {
    oracle::assert_matches_reference("print(('hello'):upper())");
}

#[test]
fn oracle_string_method_len() {
    oracle::assert_matches_reference("print(('abc'):len())");
}

#[test]
fn oracle_string_method_sub() {
    oracle::assert_matches_reference("print(('hello'):sub(1, 3))");
}

#[test]
fn oracle_string_method_find() {
    oracle::assert_matches_reference("print(('hello world'):find('world'))");
}

#[test]
fn oracle_string_method_gsub() {
    oracle::assert_matches_reference("print(('aaa'):gsub('a', 'b'))");
}

#[test]
fn oracle_string_type() {
    oracle::assert_matches_reference("print(type(string))");
}

#[test]
fn oracle_string_format_leading_zero() {
    oracle::assert_matches_reference("print(string.format('%05d', 42))");
}

#[test]
fn oracle_string_format_left_align() {
    oracle::assert_matches_reference("print(string.format('%-10s|', 'hi'))");
}

#[test]
fn oracle_string_find_plain_flag() {
    oracle::assert_matches_reference("print(string.find('hello%world', '%', 1, true))");
}

// ---------------------------------------------------------------------------
// Table library oracle tests
// ---------------------------------------------------------------------------

#[test]
fn oracle_table_concat_basic() {
    oracle::assert_matches_reference("print(table.concat({1, 2, 3}, ', '))");
}

#[test]
fn oracle_table_concat_default_sep() {
    oracle::assert_matches_reference("print(table.concat({'a', 'b', 'c'}))");
}

#[test]
fn oracle_table_concat_range() {
    oracle::assert_matches_reference("print(table.concat({'a', 'b', 'c', 'd'}, '-', 2, 3))");
}

#[test]
fn oracle_table_concat_empty() {
    oracle::assert_matches_reference("print(table.concat({}, ', '))");
}

#[test]
fn oracle_table_insert_append() {
    oracle::assert_matches_reference(
        "local t = {1, 2, 3}; table.insert(t, 4); print(t[1], t[2], t[3], t[4])",
    );
}

#[test]
fn oracle_table_insert_at_position() {
    oracle::assert_matches_reference(
        "local t = {1, 2, 3}; table.insert(t, 2, 99); print(t[1], t[2], t[3], t[4])",
    );
}

#[test]
fn oracle_table_remove_last() {
    oracle::assert_matches_reference(
        "local t = {1, 2, 3}; local v = table.remove(t); print(v, #t)",
    );
}

#[test]
fn oracle_table_remove_at_position() {
    oracle::assert_matches_reference(
        "local t = {1, 2, 3}; local v = table.remove(t, 1); print(v, t[1], t[2])",
    );
}

#[test]
fn oracle_table_sort_numbers() {
    oracle::assert_matches_reference(
        "local t = {3, 1, 4, 1, 5, 9, 2, 6}; table.sort(t); print(table.concat(t, ', '))",
    );
}

#[test]
fn oracle_table_sort_strings() {
    oracle::assert_matches_reference(
        "local t = {'banana', 'apple', 'cherry'}; table.sort(t); print(table.concat(t, ', '))",
    );
}

#[test]
fn oracle_table_sort_custom() {
    oracle::assert_matches_reference(
        "local t = {3, 1, 4}; table.sort(t, function(a, b) return a > b end); print(table.concat(t, ', '))",
    );
}

#[test]
fn oracle_table_maxn() {
    oracle::assert_matches_reference("print(table.maxn({1, 2, 3}))");
}

#[test]
fn oracle_table_maxn_empty() {
    oracle::assert_matches_reference("print(table.maxn({}))");
}

#[test]
fn oracle_table_maxn_float_keys() {
    oracle::assert_matches_reference(
        "local t = {}; t[1] = 'a'; t[3.5] = 'b'; t[100] = 'c'; print(table.maxn(t))",
    );
}

#[test]
fn oracle_table_getn() {
    oracle::assert_matches_reference("print(table.getn({10, 20, 30}))");
}

#[test]
fn oracle_table_foreachi() {
    oracle::assert_matches_reference(
        "local r = '' table.foreachi({10, 20, 30}, function(i, v) r = r .. i .. '=' .. v .. ' ' end) print(r)",
    );
}

#[test]
fn oracle_table_foreachi_early_return() {
    oracle::assert_matches_reference(
        "local v = table.foreachi({10, 20, 30}, function(i, v) if v == 20 then return 'found' end end) print(v)",
    );
}

#[test]
fn oracle_table_sort_empty() {
    oracle::assert_matches_reference("local t = {}; table.sort(t); print(#t)");
}

#[test]
fn oracle_table_sort_single() {
    oracle::assert_matches_reference("local t = {42}; table.sort(t); print(t[1])");
}

#[test]
fn oracle_table_insert_remove_sequence() {
    oracle::assert_matches_reference(
        "local t = {1, 2, 3}; table.insert(t, 2, 99); table.remove(t, 3); print(table.concat(t, ', '))",
    );
}

// ---------------------------------------------------------------------------
// Math library oracle tests
// ---------------------------------------------------------------------------

#[test]
fn oracle_math_pi() {
    oracle::assert_matches_reference("print(math.pi)");
}

#[test]
fn oracle_math_huge() {
    oracle::assert_matches_reference("print(math.huge)");
}

#[test]
fn oracle_math_huge_negative() {
    oracle::assert_matches_reference("print(-math.huge)");
}

#[test]
fn oracle_math_abs() {
    oracle::assert_matches_reference("print(math.abs(-10), math.abs(5), math.abs(0))");
}

#[test]
fn oracle_math_floor() {
    oracle::assert_matches_reference("print(math.floor(4.5), math.floor(-2.3), math.floor(3))");
}

#[test]
fn oracle_math_ceil() {
    oracle::assert_matches_reference("print(math.ceil(4.5), math.ceil(-2.3), math.ceil(3))");
}

#[test]
fn oracle_math_sin_cos() {
    oracle::assert_matches_reference("print(math.sin(0), math.cos(0), math.sin(math.pi/2))");
}

#[test]
fn oracle_math_tan() {
    oracle::assert_matches_reference("print(math.sin(0), math.tan(0))");
}

#[test]
fn oracle_math_asin_acos_atan() {
    oracle::assert_matches_reference("print(math.asin(0), math.acos(1), math.atan(0))");
}

#[test]
fn oracle_math_atan2() {
    oracle::assert_matches_reference("print(math.atan2(1, 0), math.atan2(0, 1))");
}

#[test]
fn oracle_math_sinh_cosh_tanh() {
    oracle::assert_matches_reference("print(math.sinh(0), math.cosh(0), math.tanh(0))");
}

#[test]
fn oracle_math_exp_log() {
    oracle::assert_matches_reference("print(math.exp(0), math.exp(1), math.log(1))");
}

#[test]
fn oracle_math_log10() {
    oracle::assert_matches_reference("print(math.log10(1), math.log10(10), math.log10(100))");
}

#[test]
fn oracle_math_sqrt() {
    oracle::assert_matches_reference(
        "print(math.sqrt(0), math.sqrt(1), math.sqrt(4), math.sqrt(16))",
    );
}

#[test]
fn oracle_math_pow() {
    oracle::assert_matches_reference("print(math.pow(2, 0), math.pow(2, 10), math.pow(3, 3))");
}

#[test]
fn oracle_math_deg_rad() {
    oracle::assert_matches_reference("print(math.deg(math.pi), math.rad(180))");
}

#[test]
fn oracle_math_fmod() {
    oracle::assert_matches_reference("print(math.fmod(10, 3), math.fmod(7, 2))");
}

#[test]
fn oracle_math_mod_alias() {
    oracle::assert_matches_reference("print(math.mod(10, 3))");
}

#[test]
fn oracle_math_modf() {
    oracle::assert_matches_reference("print(math.modf(3.5))");
}

#[test]
fn oracle_math_modf_negative() {
    oracle::assert_matches_reference("print(math.modf(-3.5))");
}

#[test]
fn oracle_math_frexp() {
    oracle::assert_matches_reference("print(math.frexp(8))");
}

#[test]
fn oracle_math_frexp_pi() {
    oracle::assert_matches_reference("local v, e = math.frexp(math.pi); print(v, e)");
}

#[test]
fn oracle_math_ldexp() {
    oracle::assert_matches_reference("print(math.ldexp(0.5, 4))");
}

#[test]
fn oracle_math_frexp_ldexp_roundtrip() {
    oracle::assert_matches_reference("local v, e = math.frexp(math.pi); print(math.ldexp(v, e))");
}

#[test]
fn oracle_math_min() {
    oracle::assert_matches_reference("print(math.min(3, 1, 4, 1, 5))");
}

#[test]
fn oracle_math_max() {
    oracle::assert_matches_reference("print(math.max(3, 1, 4, 1, 5))");
}

#[test]
fn oracle_math_min_single() {
    oracle::assert_matches_reference("print(math.min(42))");
}

#[test]
fn oracle_math_max_negative() {
    oracle::assert_matches_reference("print(math.max(-5, -2, -8))");
}

#[test]
fn oracle_math_pythagorean() {
    oracle::assert_matches_reference(
        "function eq(a,b) return math.abs(a-b) < 1e-10 end; print(eq(math.sin(-9.8)^2 + math.cos(-9.8)^2, 1))",
    );
}

#[test]
fn oracle_math_tanh_identity() {
    oracle::assert_matches_reference(
        "function eq(a,b) return math.abs(a-b) < 1e-10 end; print(eq(math.tanh(3.5), math.sinh(3.5)/math.cosh(3.5)))",
    );
}

#[test]
fn oracle_math_type() {
    oracle::assert_matches_reference("print(type(math))");
}

#[test]
fn oracle_math_function_types() {
    oracle::assert_matches_reference("print(type(math.sin), type(math.random))");
}

// =========================================================================
// OS library oracle tests
// =========================================================================

#[test]
fn oracle_os_type() {
    oracle::assert_matches_reference("print(type(os))");
}

#[test]
fn oracle_os_function_types() {
    oracle::assert_matches_reference("print(type(os.clock), type(os.date), type(os.difftime))");
}

#[test]
fn oracle_os_function_types_2() {
    oracle::assert_matches_reference("print(type(os.execute), type(os.exit), type(os.getenv))");
}

#[test]
fn oracle_os_function_types_3() {
    oracle::assert_matches_reference("print(type(os.remove), type(os.rename), type(os.setlocale))");
}

#[test]
fn oracle_os_function_types_4() {
    oracle::assert_matches_reference("print(type(os.time), type(os.tmpname))");
}

#[test]
fn oracle_os_clock_type() {
    oracle::assert_matches_reference("print(type(os.clock()))");
}

#[test]
fn oracle_os_time_type() {
    oracle::assert_matches_reference("print(type(os.time()))");
}

#[test]
fn oracle_os_difftime() {
    oracle::assert_matches_reference("print(os.difftime(100, 50))");
}

#[test]
fn oracle_os_difftime_default() {
    oracle::assert_matches_reference("print(os.difftime(100))");
}

#[test]
fn oracle_os_date_utc_epoch() {
    oracle::assert_matches_reference("print(os.date('!%Y-%m-%d %H:%M:%S', 0))");
}

#[test]
fn oracle_os_date_utc_star_t() {
    oracle::assert_matches_reference(
        "local d = os.date('!*t', 0) \
         print(d.year, d.month, d.day, d.hour, d.min, d.sec, d.wday, d.yday)",
    );
}

#[test]
fn oracle_os_date_utc_format() {
    oracle::assert_matches_reference("print(os.date('!%H:%M:%S', 3661))");
}

#[test]
fn oracle_os_date_utc_known_timestamp() {
    // 2001-09-09 01:46:40 UTC (1 billion seconds)
    oracle::assert_matches_reference("print(os.date('!%Y-%m-%d %H:%M:%S', 1000000000))");
}

#[test]
fn oracle_os_date_star_t_roundtrip() {
    oracle::assert_matches_reference(
        "local t = os.time({year=2000, month=6, day=15, hour=12}) \
         local d = os.date('*t', t) \
         local t2 = os.time(d) \
         print(t == t2)",
    );
}

#[test]
fn oracle_os_time_table() {
    oracle::assert_matches_reference("print(type(os.time({year=2000, month=1, day=1})))");
}

#[test]
fn oracle_os_time_table_missing_field() {
    oracle::assert_matches_reference(
        "local ok, err = pcall(os.time, {year=2000, month=1}) print(ok, err)",
    );
}

#[test]
fn oracle_os_getenv_path() {
    oracle::assert_matches_reference("print(os.getenv('PATH') ~= nil)");
}

#[test]
fn oracle_os_getenv_nonexistent() {
    oracle::assert_matches_reference("print(os.getenv('RILUA_NONEXISTENT_12345'))");
}

#[test]
fn oracle_os_execute_no_args() {
    oracle::assert_matches_reference("print(os.execute() ~= 0)");
}

#[test]
fn oracle_os_execute_true() {
    oracle::assert_matches_reference("print(os.execute('true'))");
}

#[test]
fn oracle_os_remove_nonexistent() {
    oracle::assert_matches_reference(
        "local a, b, c = os.remove('/tmp/rilua_nonexistent_xyz') \
         print(type(a), type(b), type(c))",
    );
}

#[test]
fn oracle_os_rename_nonexistent() {
    oracle::assert_matches_reference(
        "local a, b, c = os.rename('/tmp/rilua_no_1', '/tmp/rilua_no_2') \
         print(type(a), type(b), type(c))",
    );
}

#[test]
fn oracle_os_setlocale_c() {
    oracle::assert_matches_reference("print(os.setlocale('C'))");
}

#[test]
fn oracle_os_setlocale_query() {
    oracle::assert_matches_reference("print(type(os.setlocale(nil)))");
}

#[test]
fn oracle_os_setlocale_invalid() {
    oracle::assert_matches_reference("print(os.setlocale('invalid_locale_xyz'))");
}

#[test]
fn oracle_os_setlocale_category() {
    oracle::assert_matches_reference("print(os.setlocale('C', 'numeric'))");
}

#[test]
fn oracle_os_tmpname_type() {
    oracle::assert_matches_reference("local n = os.tmpname() print(type(n)) os.remove(n)");
}

// ---------------------------------------------------------------------------
// Userdata / newproxy
// ---------------------------------------------------------------------------

#[test]
fn oracle_newproxy_type() {
    oracle::assert_matches_reference("print(type(newproxy()))");
}

#[test]
fn oracle_newproxy_true_type() {
    oracle::assert_matches_reference("print(type(newproxy(true)))");
}

#[test]
fn oracle_newproxy_false_type() {
    oracle::assert_matches_reference("print(type(newproxy(false)))");
}

#[test]
fn oracle_newproxy_metatable() {
    oracle::assert_matches_reference("local p = newproxy(true) print(type(getmetatable(p)))");
}

#[test]
fn oracle_newproxy_no_metatable() {
    oracle::assert_matches_reference("print(getmetatable(newproxy()))");
}

// ---------------------------------------------------------------------------
// I/O library
// ---------------------------------------------------------------------------

#[test]
fn oracle_io_type() {
    oracle::assert_matches_reference("print(type(io))");
}

#[test]
fn oracle_io_type_stdin() {
    oracle::assert_matches_reference("print(io.type(io.stdin))");
}

#[test]
fn oracle_io_type_stdout() {
    oracle::assert_matches_reference("print(io.type(io.stdout))");
}

#[test]
fn oracle_io_type_stderr() {
    oracle::assert_matches_reference("print(io.type(io.stderr))");
}

#[test]
fn oracle_io_type_table() {
    oracle::assert_matches_reference("print(io.type({}))");
}

#[test]
fn oracle_io_type_nil() {
    oracle::assert_matches_reference("print(io.type(nil))");
}

#[test]
fn oracle_io_type_number() {
    oracle::assert_matches_reference("print(io.type(42))");
}

#[test]
fn oracle_io_function_types() {
    oracle::assert_matches_reference(
        "print(type(io.close), type(io.flush), type(io.input), type(io.lines))",
    );
}

#[test]
fn oracle_io_function_types_2() {
    oracle::assert_matches_reference("print(type(io.open), type(io.output), type(io.popen))");
}

#[test]
fn oracle_io_function_types_3() {
    oracle::assert_matches_reference(
        "print(type(io.read), type(io.tmpfile), type(io.type), type(io.write))",
    );
}

#[test]
fn oracle_io_write() {
    oracle::assert_matches_reference("io.write('hello world\\n')");
}

#[test]
fn oracle_io_write_number() {
    oracle::assert_matches_reference("io.write(42) io.write('\\n')");
}

#[test]
fn oracle_io_write_multi() {
    oracle::assert_matches_reference("io.write('a', 'b', 'c', '\\n')");
}

#[test]
fn oracle_io_tmpfile_type() {
    oracle::assert_matches_reference("print(io.type(io.tmpfile()))");
}

#[test]
fn oracle_io_open_nonexistent() {
    oracle::assert_matches_reference(
        "local f, msg, code = io.open('/tmp/__rilua_oracle_no__', 'r') \
         print(f, type(msg), type(code))",
    );
}

#[test]
fn oracle_io_file_read_write() {
    oracle::assert_matches_reference(
        "local name = os.tmpname() \
         local f = io.open(name, 'w') \
         f:write('hello\\n') \
         f:close() \
         local f = io.open(name, 'r') \
         print(f:read('*l')) \
         f:close() \
         os.remove(name)",
    );
}

#[test]
fn oracle_io_file_read_all() {
    oracle::assert_matches_reference(
        "local name = os.tmpname() \
         local f = io.open(name, 'w') \
         f:write('abc\\ndef\\n') \
         f:close() \
         local f = io.open(name, 'r') \
         local s = f:read('*a') \
         print(#s) \
         f:close() \
         os.remove(name)",
    );
}

#[test]
fn oracle_io_file_read_number() {
    oracle::assert_matches_reference(
        "local name = os.tmpname() \
         local f = io.open(name, 'w') \
         f:write('3.14 42\\n') \
         f:close() \
         local f = io.open(name, 'r') \
         print(f:read('*n'), f:read('*n')) \
         f:close() \
         os.remove(name)",
    );
}

#[test]
fn oracle_io_file_read_bytes() {
    oracle::assert_matches_reference(
        "local name = os.tmpname() \
         local f = io.open(name, 'w') \
         f:write('hello world') \
         f:close() \
         local f = io.open(name, 'r') \
         print(f:read(5), f:read(6)) \
         f:close() \
         os.remove(name)",
    );
}

#[test]
fn oracle_io_lines() {
    oracle::assert_matches_reference(
        "local name = os.tmpname() \
         local f = io.open(name, 'w') \
         f:write('a\\nb\\nc\\n') \
         f:close() \
         for line in io.lines(name) do print(line) end",
    );
}

#[test]
fn oracle_io_popen_echo() {
    oracle::assert_matches_reference(
        "local f = io.popen('echo hello') \
         print(f:read('*l')) \
         f:close()",
    );
}

#[test]
fn oracle_io_seek() {
    oracle::assert_matches_reference(
        "local f = io.tmpfile() \
         f:write('hello') \
         print(f:seek('cur')) \
         f:seek('set', 0) \
         print(f:read('*a')) \
         f:close()",
    );
}

#[test]
fn oracle_io_type_closed() {
    oracle::assert_matches_reference(
        "local f = io.tmpfile() \
         f:close() \
         print(io.type(f))",
    );
}

#[test]
fn oracle_io_input_output_type() {
    oracle::assert_matches_reference("print(io.type(io.input()), io.type(io.output()))");
}

// ---------------------------------------------------------------------------
// Package library oracle tests
// ---------------------------------------------------------------------------

#[test]
fn oracle_package_loaded_type() {
    oracle::assert_matches_reference("print(type(package.loaded))");
}

#[test]
fn oracle_package_preload_type() {
    oracle::assert_matches_reference("print(type(package.preload))");
}

#[test]
fn oracle_package_loaders_type() {
    oracle::assert_matches_reference("print(type(package.loaders))");
}

#[test]
fn oracle_require_string_identity() {
    oracle::assert_matches_reference("print(require('string') == string)");
}

#[test]
fn oracle_require_math_identity() {
    oracle::assert_matches_reference("print(require('math') == math)");
}

#[test]
fn oracle_require_table_identity() {
    oracle::assert_matches_reference("print(require('table') == table)");
}

#[test]
fn oracle_package_config_lines() {
    oracle::assert_matches_reference(
        "local lines = {} \
         for line in package.config:gmatch('[^\\n]+') do \
           lines[#lines + 1] = line \
         end \
         print(#lines) \
         print(lines[1]) \
         print(lines[2]) \
         print(lines[3])",
    );
}

#[test]
fn oracle_require_not_found_message() {
    // Match error behavior: both should fail with similar message format.
    oracle::assert_matches_reference(
        "local ok, err = pcall(require, 'nonexistent_xyz_module') \
         print(ok) \
         print(err:find('not found') ~= nil)",
    );
}

#[test]
fn oracle_require_caching() {
    oracle::assert_matches_reference(
        "local a = require('string') \
         local b = require('string') \
         print(a == b)",
    );
}

#[test]
fn oracle_package_loadlib_returns_three_values() {
    // PUC-Rio returns (nil, errormsg, "open") because it has dlopen.
    // rilua returns (nil, errormsg, "absent") because it lacks C loading.
    // Only test that 3 values are returned and first is nil.
    oracle::assert_matches_reference(
        "local f, err, kind = package.loadlib('foo', 'bar') \
         print(type(f), type(err), type(kind))",
    );
}

#[test]
fn oracle_require_type() {
    oracle::assert_matches_reference("print(type(require))");
}

#[test]
fn oracle_module_type() {
    oracle::assert_matches_reference("print(type(module))");
}

// ---------------------------------------------------------------------------
// Coroutine library oracle tests
// ---------------------------------------------------------------------------

#[test]
fn oracle_coroutine_table_type() {
    oracle::assert_matches_reference("print(type(coroutine))");
}

#[test]
fn oracle_coroutine_create_type() {
    oracle::assert_matches_reference("print(type(coroutine.create))");
}

#[test]
fn oracle_coroutine_resume_return() {
    oracle::assert_matches_reference(
        "local co = coroutine.create(function() return 42 end) \
         print(coroutine.resume(co))",
    );
}

#[test]
fn oracle_coroutine_yield_values() {
    oracle::assert_matches_reference(
        "local co = coroutine.create(function() coroutine.yield(1,2,3) end) \
         print(coroutine.resume(co))",
    );
}

#[test]
fn oracle_coroutine_dead_resume() {
    oracle::assert_matches_reference(
        "local co = coroutine.create(function() end) \
         coroutine.resume(co) \
         print(coroutine.resume(co))",
    );
}

#[test]
fn oracle_coroutine_status_cycle() {
    oracle::assert_matches_reference(
        "local co = coroutine.create(function() coroutine.yield() end) \
         print(coroutine.status(co)) \
         coroutine.resume(co) \
         print(coroutine.status(co)) \
         coroutine.resume(co) \
         print(coroutine.status(co))",
    );
}

#[test]
fn oracle_coroutine_running_main() {
    oracle::assert_matches_reference("print(coroutine.running())");
}

#[test]
fn oracle_coroutine_wrap_basic() {
    oracle::assert_matches_reference(
        "local f = coroutine.wrap(function() \
           coroutine.yield(10) \
           coroutine.yield(20) \
           return 30 \
         end) \
         print(f()) print(f()) print(f())",
    );
}

#[test]
fn oracle_coroutine_resume_passes_args() {
    oracle::assert_matches_reference(
        "local co = coroutine.create(function(a,b) return a+b end) \
         print(coroutine.resume(co, 10, 20))",
    );
}

#[test]
fn oracle_coroutine_resume_to_yield() {
    oracle::assert_matches_reference(
        "local co = coroutine.create(function() \
           local x = coroutine.yield() \
           return x * 2 \
         end) \
         coroutine.resume(co) \
         print(coroutine.resume(co, 21))",
    );
}

#[test]
fn oracle_coroutine_multiple_return() {
    oracle::assert_matches_reference(
        "local co = coroutine.create(function() return 1,2,3 end) \
         print(coroutine.resume(co))",
    );
}

#[test]
fn oracle_coroutine_error_in_body() {
    oracle::assert_matches_reference(
        "local co = coroutine.create(function() error('oops') end) \
         local ok, msg = coroutine.resume(co) \
         print(ok, type(msg))",
    );
}

#[test]
fn oracle_coroutine_require_loaded() {
    oracle::assert_matches_reference("print(require('coroutine') == coroutine)");
}

// ---------------------------------------------------------------------------
// Debug library oracle tests
// ---------------------------------------------------------------------------

#[test]
fn oracle_debug_table_type() {
    oracle::assert_matches_reference("print(type(debug))");
}

#[test]
fn oracle_debug_getregistry_type() {
    oracle::assert_matches_reference("print(type(debug.getregistry()))");
}

#[test]
fn oracle_debug_getmetatable_raw() {
    oracle::assert_matches_reference(
        "local t = {} \
         local mt = {__metatable = 'hidden'} \
         setmetatable(t, mt) \
         print(getmetatable(t)) \
         print(debug.getmetatable(t) == mt)",
    );
}

#[test]
fn oracle_debug_getmetatable_nil() {
    oracle::assert_matches_reference("print(debug.getmetatable({}))");
}

#[test]
fn oracle_debug_setmetatable_returns_true() {
    oracle::assert_matches_reference(
        "local t = {} \
         print(debug.setmetatable(t, {}))",
    );
}

#[test]
fn oracle_debug_getinfo_what_s() {
    oracle::assert_matches_reference(
        "local info = debug.getinfo(1, 'S') \
         print(info.what)",
    );
}

#[test]
fn oracle_debug_getinfo_c_function() {
    oracle::assert_matches_reference(
        "local info = debug.getinfo(print, 'S') \
         print(info.what, info.source)",
    );
}

#[test]
fn oracle_debug_getinfo_invalid_level() {
    oracle::assert_matches_reference("print(debug.getinfo(100))");
}

#[test]
fn oracle_debug_getinfo_nups() {
    oracle::assert_matches_reference(
        "local x = 1 \
         local function f() return x end \
         print(debug.getinfo(f, 'u').nups)",
    );
}

#[test]
fn oracle_debug_getlocal_basic() {
    oracle::assert_matches_reference(
        "local x = 42 \
         local name, val = debug.getlocal(1, 1) \
         print(name, val)",
    );
}

#[test]
fn oracle_debug_getlocal_out_of_range() {
    oracle::assert_matches_reference(
        "local x = 1 \
         print(debug.getlocal(1, 99))",
    );
}

#[test]
fn oracle_debug_getupvalue_basic() {
    oracle::assert_matches_reference(
        "local x = 42 \
         local function f() return x end \
         print(debug.getupvalue(f, 1))",
    );
}

#[test]
fn oracle_debug_getupvalue_out_of_range() {
    oracle::assert_matches_reference(
        "local function f() end \
         print(debug.getupvalue(f, 99))",
    );
}

#[test]
fn oracle_debug_gethook_stub() {
    oracle::assert_matches_reference(
        "local a, b, c = debug.gethook() \
         print(a, b, c)",
    );
}

#[test]
fn oracle_debug_traceback_type() {
    oracle::assert_matches_reference("print(type(debug.traceback()))");
}

#[test]
fn oracle_debug_traceback_non_string() {
    // Numbers are treated as string messages in both. Our CLI has one fewer
    // C frame at the bottom (no lua_pcall wrapper), so compare just the
    // message prefix, not the full traceback.
    let rilua = oracle::run_rilua("print(debug.traceback(42))");
    assert!(
        rilua.stdout.starts_with("42\nstack traceback:"),
        "rilua output should start with '42\\nstack traceback:': {:?}",
        rilua.stdout,
    );
}

#[test]
fn oracle_debug_traceback_nil() {
    oracle::assert_matches_reference("print(debug.traceback(nil))");
}

#[test]
fn oracle_debug_require_loaded() {
    oracle::assert_matches_reference("print(require('debug') == debug)");
}

#[test]
fn oracle_debug_getinfo_func_field() {
    oracle::assert_matches_reference(
        "local function foo() end \
         local info = debug.getinfo(foo, 'f') \
         print(info.func == foo)",
    );
}

#[test]
fn oracle_debug_setmetatable_number() {
    oracle::assert_matches_reference(
        "debug.setmetatable(0, {__tostring = function(n) return 'num:' .. n end}) \
         print(tostring(42)) \
         debug.setmetatable(0, nil)",
    );
}

// ---------------------------------------------------------------------------
// CLI behavior oracle tests (Phase 8d)
// ---------------------------------------------------------------------------

#[test]
fn oracle_cli_script_arg_table() {
    // Compare arg table behavior between rilua and PUC-Rio.
    if !oracle::reference_available() {
        eprintln!("skipping: reference Lua binary not available");
        return;
    }

    let script = std::env::temp_dir().join("oracle_cli_args.lua");
    std::fs::write(&script, "print(arg[-1], arg[0], arg[1], arg[2])").ok();
    let script_path = script.to_str().unwrap_or("");

    let rilua = oracle::run_rilua_args(&[script_path, "foo", "bar"]);
    let reference = oracle::run_reference_args(&[script_path, "foo", "bar"]);

    if let Some(ref_out) = reference {
        // The binary name (arg[-1]) will differ, but arg[0], arg[1], arg[2]
        // should be identical.
        assert_eq!(rilua.exit_code, ref_out.exit_code, "exit code mismatch");
        // Both should print the script path, "foo", and "bar".
        assert!(
            rilua.stdout.contains("foo") && rilua.stdout.contains("bar"),
            "rilua missing args: {}",
            rilua.stdout
        );
        assert!(
            ref_out.stdout.contains("foo") && ref_out.stdout.contains("bar"),
            "ref missing args: {}",
            ref_out.stdout
        );
    }

    std::fs::remove_file(&script).ok();
}

#[test]
fn oracle_cli_script_varargs() {
    if !oracle::reference_available() {
        eprintln!("skipping: reference Lua binary not available");
        return;
    }

    let script = std::env::temp_dir().join("oracle_cli_varargs.lua");
    std::fs::write(&script, "print(...)").ok();
    let script_path = script.to_str().unwrap_or("");

    let rilua = oracle::run_rilua_args(&[script_path, "a", "b", "c"]);
    let reference = oracle::run_reference_args(&[script_path, "a", "b", "c"]);

    if let Some(ref_out) = reference {
        assert_eq!(rilua.stdout, ref_out.stdout, "varargs output mismatch");
        assert_eq!(rilua.exit_code, ref_out.exit_code, "exit code mismatch");
    }

    std::fs::remove_file(&script).ok();
}

#[test]
fn oracle_cli_e_flag_error() {
    if !oracle::reference_available() {
        eprintln!("skipping: reference Lua binary not available");
        return;
    }

    let rilua = oracle::run_rilua_args(&["-e", "error('test error')"]);
    let reference = oracle::run_reference_args(&["-e", "error('test error')"]);

    if let Some(ref_out) = reference {
        assert_eq!(rilua.exit_code, ref_out.exit_code, "exit code mismatch");
        // Both should contain the error message.
        assert!(
            rilua.stderr.contains("test error"),
            "rilua stderr: {}",
            rilua.stderr
        );
        assert!(
            ref_out.stderr.contains("test error"),
            "ref stderr: {}",
            ref_out.stderr
        );
    }
}

#[test]
fn oracle_cli_multiple_e() {
    oracle::assert_matches_reference("x=10 print(x)");
}

#[test]
fn oracle_cli_stdin_dash() {
    if !oracle::reference_available() {
        eprintln!("skipping: reference Lua binary not available");
        return;
    }

    use std::io::Write;
    use std::process::{Command, Stdio};

    let rilua = oracle::run_rilua_stdin(&["-"], "print(99)\n");

    // PUC-Rio equivalent.
    let bin = oracle::reference_bin();
    if !bin.exists() {
        return;
    }
    let mut child = Command::new(&bin)
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn reference");
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(b"print(99)\n").ok();
    }
    let output = child.wait_with_output().expect("wait failed");
    let ref_stdout = String::from_utf8_lossy(&output.stdout).into_owned();

    assert_eq!(rilua.stdout, ref_stdout, "stdin dash output mismatch");
}

// ---------------------------------------------------------------------------
// string.dump / binary chunk oracle tests (Phase 9b)
// ---------------------------------------------------------------------------

#[test]
fn oracle_string_dump_roundtrip() {
    oracle::assert_matches_reference(
        "local f = loadstring('return 42'); print(loadstring(string.dump(f))())",
    );
}

#[test]
fn oracle_string_dump_constants() {
    oracle::assert_matches_reference(
        r#"local f = loadstring("return nil, true, false, 3.14, 'hello'"); print(loadstring(string.dump(f))())"#,
    );
}

#[test]
fn oracle_string_dump_nested() {
    oracle::assert_matches_reference(
        r#"local f = function() local g = function() return "inner" end; return g() end; print(loadstring(string.dump(f))())"#,
    );
}

#[test]
fn oracle_string_dump_error_nonfunc() {
    // Both should return false + error message; exact wording may differ.
    let rilua = oracle::run_rilua("print(pcall(string.dump, 42))");
    assert!(
        rilua.stdout.starts_with("false"),
        "rilua should fail: {}",
        rilua.stdout
    );
    if let Some(reference) = oracle::run_reference("print(pcall(string.dump, 42))") {
        assert!(
            reference.stdout.starts_with("false"),
            "reference should fail: {}",
            reference.stdout
        );
    }
}

#[test]
fn oracle_string_dump_error_cfunc() {
    // Both should return false + "unable to dump" error; exact wording may differ.
    let rilua = oracle::run_rilua("print(pcall(string.dump, print))");
    assert!(
        rilua.stdout.starts_with("false"),
        "rilua should fail: {}",
        rilua.stdout
    );
    if let Some(reference) = oracle::run_reference("print(pcall(string.dump, print))") {
        assert!(
            reference.stdout.starts_with("false"),
            "reference should fail: {}",
            reference.stdout
        );
    }
}

#[test]
fn oracle_string_dump_type_check() {
    oracle::assert_matches_reference("local f = function() end; print(type(string.dump(f)))");
}

#[test]
fn oracle_string_dump_signature() {
    oracle::assert_matches_reference(
        "local f = function() end; local s = string.dump(f); print(string.byte(s,1), string.byte(s,2), string.byte(s,3), string.byte(s,4))",
    );
}

#[test]
fn oracle_dump_load_exec() {
    oracle::assert_matches_reference(
        r#"local f = loadstring("local x = 10; for i=1,3 do x = x + i end; return x"); local g = loadstring(string.dump(f)); print(g())"#,
    );
}

#[test]
fn oracle_dump_vararg() {
    oracle::assert_matches_reference(
        r"local f = function(...) return select('#', ...) end; print(loadstring(string.dump(f))(1,2,3))",
    );
}

#[test]
fn oracle_dump_upvalue_reset() {
    oracle::assert_matches_reference(
        "local x = 10; local f = function() return x end; local g = loadstring(string.dump(f)); print(type(g()))",
    );
}

// ---------------------------------------------------------------------------
// Error message formatting (Phase 9c)
// ---------------------------------------------------------------------------

#[test]
fn oracle_error_msg_call_local() {
    oracle::assert_matches_reference(
        "local x = 1; local ok, msg = pcall(function() x() end); print(msg)",
    );
}

#[test]
fn oracle_error_msg_call_global() {
    oracle::assert_matches_reference("local ok, msg = pcall(function() foo() end); print(msg)");
}

#[test]
fn oracle_error_msg_arith_local() {
    oracle::assert_matches_reference(
        "local x = 'hello'; local ok, msg = pcall(function() return x + 1 end); print(msg)",
    );
}

#[test]
fn oracle_error_msg_index_local() {
    oracle::assert_matches_reference(
        "local x = nil; local ok, msg = pcall(function() return x.y end); print(msg)",
    );
}

#[test]
fn oracle_error_msg_concat_local() {
    oracle::assert_matches_reference(
        "local x = {}; local ok, msg = pcall(function() return 'a' .. x end); print(msg)",
    );
}

#[test]
fn oracle_error_msg_len_local() {
    oracle::assert_matches_reference(
        "local x = true; local ok, msg = pcall(function() return #x end); print(msg)",
    );
}

#[test]
fn oracle_error_msg_call_upvalue() {
    oracle::assert_matches_reference(
        "local x = 1; local ok, msg = pcall(function() return (function() x() end)() end); print(msg)",
    );
}

#[test]
fn oracle_error_msg_call_field() {
    oracle::assert_matches_reference(
        "local t = {x = 1}; local ok, msg = pcall(function() t.x() end); print(msg)",
    );
}

#[test]
fn oracle_traceback_error() {
    oracle::assert_matches_reference_stderr("error('boom')");
}

#[test]
fn oracle_traceback_nested() {
    oracle::assert_matches_reference_stderr(
        "function a() error('nested') end; function b() a() end; function c() b() end; c()",
    );
}

#[test]
fn oracle_traceback_type_error() {
    oracle::assert_matches_reference_stderr(
        "function foo() local x = nil; return x + 1 end; foo()",
    );
}

#[test]
fn oracle_traceback_debug_traceback() {
    oracle::assert_matches_reference(
        "function foo() return debug.traceback('msg', 1) end; print(foo())",
    );
}

#[test]
fn oracle_traceback_debug_traceback_no_args() {
    oracle::assert_matches_reference("function foo() return debug.traceback() end; print(foo())");
}

#[test]
fn oracle_error_msg_compare() {
    oracle::assert_matches_reference(
        "local ok, msg = pcall(function() return {} < 1 end); print(msg)",
    );
}
