//! Property-based fuzz tests for the rilua pipeline.
//!
//! These tests feed arbitrary byte sequences (and structured Lua fragments)
//! through the lexer, parser, compiler, and VM. The property under test is
//! that no stage panics — every input must produce either `Ok(_)` or
//! `Err(LuaError)`.

use proptest::prelude::*;
use rilua::Lua;
use rilua::compiler::codegen;
use rilua::compiler::lexer::Lexer;
use rilua::compiler::parser;
use rilua::compiler::token::Token;

// --- Raw byte fuzzing ---

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    #[test]
    fn prop_lexer_no_panic(data in proptest::collection::vec(any::<u8>(), 0..4096)) {
        let mut lexer = Lexer::new(&data, "proptest");
        loop {
            match lexer.next() {
                Ok((Token::Eos, _)) | Err(_) => break,
                Ok(_) => {}
            }
        }
    }

    #[test]
    fn prop_parser_no_panic(data in proptest::collection::vec(any::<u8>(), 0..4096)) {
        let _ = parser::parse(&data, "proptest");
    }

    #[test]
    fn prop_compiler_no_panic(data in proptest::collection::vec(any::<u8>(), 0..4096)) {
        let _ = codegen::compile(&data, "proptest");
    }

    #[test]
    fn prop_vm_no_panic(data in proptest::collection::vec(any::<u8>(), 0..4096)) {
        let Ok(mut lua) = Lua::new() else { return Ok(()) };
        let _ = lua.exec_bytes(&data, "proptest");
    }
}

// --- Structured Lua generation ---

/// Generates syntactically plausible (but semantically random) Lua statements.
fn arb_lua_program() -> impl Strategy<Value = String> {
    proptest::collection::vec(arb_lua_statement(), 1..10).prop_map(|stmts| stmts.join("\n"))
}

fn arb_lua_statement() -> impl Strategy<Value = String> {
    prop_oneof![
        // Assignment: local x = <expr>
        (arb_identifier(), arb_lua_expr()).prop_map(|(id, expr)| format!("local {id} = {expr}")),
        // Function call: print(<expr>)
        arb_lua_expr().prop_map(|expr| format!("print({expr})")),
        // If statement
        (arb_lua_expr(), arb_lua_expr())
            .prop_map(|(cond, body)| { format!("if {cond} then local _ = {body} end") }),
        // While loop (with a false condition to avoid hangs)
        arb_lua_expr().prop_map(|body| format!("while false do local _ = {body} end")),
        // For loop (bounded)
        (arb_identifier(), arb_lua_expr())
            .prop_map(|(id, body)| { format!("for {id} = 1, 3 do local _ = {body} end") }),
        // Do block
        arb_lua_expr().prop_map(|expr| format!("do local _ = {expr} end")),
        // Repeat-until (with true condition to run once)
        arb_lua_expr().prop_map(|body| format!("repeat local _ = {body} until true")),
        // Table constructor
        arb_lua_expr().prop_map(|expr| format!("local _ = {{ {expr}, {expr} }}")),
        // String operations
        arb_string_literal().prop_map(|s| format!("local _ = {s}")),
        // Numeric operations
        (arb_lua_number(), arb_lua_number(), arb_binop())
            .prop_map(|(a, b, op)| format!("local _ = {a} {op} {b}")),
    ]
}

fn arb_lua_expr() -> impl Strategy<Value = String> {
    prop_oneof![
        arb_lua_number(),
        arb_string_literal(),
        Just("nil".to_string()),
        Just("true".to_string()),
        Just("false".to_string()),
        arb_identifier(),
        // Unary operators
        arb_lua_number().prop_map(|n| format!("-{n}")),
        Just("not true".to_string()),
        arb_lua_number().prop_map(|n| format!("#{n}")),
        // Table constructor
        Just("{}".to_string()),
    ]
}

fn arb_identifier() -> impl Strategy<Value = String> {
    // Avoid Lua reserved words by prefixing with underscore
    "[a-z]{1,6}".prop_map(|s| format!("_{s}"))
}

fn arb_lua_number() -> impl Strategy<Value = String> {
    prop_oneof![
        // Integers
        (-1000i64..1000).prop_map(|n| n.to_string()),
        // Floats
        (-1000.0f64..1000.0)
            .prop_filter("must be finite", |f| f.is_finite())
            .prop_map(|f| format!("{f:.2}")),
        // Hex literals
        (0u32..0xFFFF).prop_map(|n| format!("0x{n:X}")),
    ]
}

fn arb_string_literal() -> impl Strategy<Value = String> {
    prop_oneof![
        // Short strings with safe ASCII content
        "[a-zA-Z0-9 ]{0,20}".prop_map(|s| format!("\"{s}\"")),
        // Long strings
        "[a-zA-Z0-9 ]{0,20}".prop_map(|s| format!("[[{s}]]")),
        // Escape sequences
        Just(r#""\n\t\\\"""#.to_string()),
        // Numeric escapes
        (0u8..=255).prop_map(|b| format!("\"\\{b}\"")),
    ]
}

fn arb_binop() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("+".to_string()),
        Just("-".to_string()),
        Just("*".to_string()),
        Just("/".to_string()),
        Just("%".to_string()),
        Just("^".to_string()),
        Just("..".to_string()),
        Just("==".to_string()),
        Just("~=".to_string()),
        Just("<".to_string()),
        Just(">".to_string()),
        Just("<=".to_string()),
        Just(">=".to_string()),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn prop_structured_lua_no_panic(program in arb_lua_program()) {
        let Ok(mut lua) = Lua::new() else { return Ok(()) };
        let _ = lua.exec_bytes(program.as_bytes(), "proptest");
    }
}
