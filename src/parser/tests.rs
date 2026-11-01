use super::*;

fn parse(src: &str) -> Program {
    Parser::new(src).unwrap().parse_program().unwrap()
}

/// Parses `source` on the engine stack — the stack `MAX_PARSE_DEPTH` is
/// calibrated against — discarding the `Program`, which is not `Send` and
/// so cannot cross the thread boundary. `ParseError` is just a `String`,
/// so the failure itself comes back for the caller to assert on.
fn parse_on_engine_stack(source: &str) -> Result<(), ParseError> {
    crate::run_on_engine_stack(move || {
        Parser::new(source)
            .and_then(|mut p| p.parse_program())
            .map(|_| ())
    })
}

#[test]
fn parse_empty() {
    let prog = parse("");
    assert!(prog.body.as_slice().is_empty());
}

#[test]
fn lookahead_reads_tokens_without_advancing_the_parser() {
    let mut parser = Parser::new("await\nusing value").expect("parser init");

    let observed = parser
        .lookahead(|tokens| -> Result<(bool, bool, bool), ParseError> {
            tokens.advance()?;
            let saw_using = matches!(tokens.current(), Token::Identifier(name) if name == "using");
            let saw_line_terminator = tokens.line_terminator_before();
            tokens.advance()?;
            let saw_value = matches!(tokens.current(), Token::Identifier(name) if name == "value");
            Ok((saw_using, saw_line_terminator, saw_value))
        })
        .expect("lookahead");

    assert_eq!(observed, (true, true, true));
    assert_eq!(parser.current, Token::Keyword(Keyword::Await));

    parser.advance().expect("advance to using");
    assert_eq!(parser.current, Token::Identifier("using".to_string()));
    assert!(parser.prev_line_terminator);

    parser.advance().expect("advance to value");
    assert_eq!(parser.current, Token::Identifier("value".to_string()));
}

#[test]
fn parse_var_declaration() {
    let prog = parse("var x = 42;");
    assert_eq!(prog.body.as_slice().len(), 1);
    assert!(matches!(&prog.body.as_slice()[0], Statement::Variable(_)));
}

#[test]
fn parse_if_statement() {
    let prog = parse("if (true) { x; } else { y; }");
    assert!(matches!(&prog.body.as_slice()[0], Statement::If(_)));
}

#[test]
fn parse_function_declaration() {
    let prog = parse("function foo(a, b) { return a + b; }");
    assert!(matches!(
        &prog.body.as_slice()[0],
        Statement::FunctionDeclaration(_)
    ));
}

#[test]
fn parse_expression_statement() {
    let prog = parse("1 + 2 * 3;");
    assert!(matches!(
        &prog.body.as_slice()[0],
        Statement::Expression(_, _)
    ));
}

#[test]
fn parse_for_loop() {
    let prog = parse("for (var i = 0; i < 10; i++) { x; }");
    assert!(matches!(&prog.body.as_slice()[0], Statement::For(_)));
}

#[test]
fn parse_arrow_function() {
    let prog = parse("var f = (a, b) => a + b;");
    assert!(matches!(&prog.body.as_slice()[0], Statement::Variable(_)));
}

#[test]
fn rejects_trailing_comma_after_object_rest_in_assignment_pattern() {
    for source in [
        "0, {...rest,} = {};",
        "({a: {...rest,}} = {});",
        "([{...rest,}] = []);",
    ] {
        assert!(Parser::new(source).unwrap().parse_program().is_err());
    }

    for source in [
        "0, {...rest} = {};",
        "0, {a, ...rest} = {};",
        "var object = {...rest,};",
        "({a, b,} = {});",
    ] {
        assert!(Parser::new(source).unwrap().parse_program().is_ok());
    }
}

#[test]
fn rejects_trailing_comma_after_object_rest_in_reinterpreted_patterns() {
    for source in [
        "for ({...rest,} in object) ;",
        "for ({...rest,} of object) ;",
        "({...rest,}) => {};",
    ] {
        assert!(Parser::new(source).unwrap().parse_program().is_err());
    }

    for source in [
        "for ({...rest} in object) ;",
        "for ({...rest} of object) ;",
        "({...rest}) => {};",
        "({...rest,});",
    ] {
        assert!(Parser::new(source).unwrap().parse_program().is_ok());
    }
}

#[test]
fn optional_chain_await_property_in_async_arrow_parameters() {
    assert!(
        Parser::new("var f = async (x = ({await: 1})?.await) => x;")
            .unwrap()
            .parse_program()
            .is_ok()
    );

    for source in [
        "var f = async (x = await) => x;",
        "var f = async (x = ({})?.[await]) => x;",
        "var f = async (x = (() => 1)?.(await)) => x;",
    ] {
        assert!(Parser::new(source).unwrap().parse_program().is_err());
    }
}

#[test]
fn parse_try_catch() {
    let prog = parse("try { x; } catch (e) { y; } finally { z; }");
    assert!(matches!(&prog.body.as_slice()[0], Statement::Try(_)));
}

#[test]
fn parse_class() {
    let prog = parse("class Foo extends Bar { constructor() {} }");
    assert!(matches!(
        &prog.body.as_slice()[0],
        Statement::ClassDeclaration(_)
    ));
}

/// Truncated source aborts the parse partway through constructs that
/// re-scope the context counters. Each such construct must put the saved
/// counters back on the failure path as well, or the enclosing construct
/// decrements a counter someone else zeroed — `parse_iteration_body` used
/// to underflow `in_iteration` that way, panicking under `overflow-checks`.
#[test]
fn truncated_source_restores_context_counters() {
    const SOURCES: &[&str] = &[
        "for (v;7; i) {function t(a) {\n t",
        "while (0) { function f() {",
        "for (;;) for (;;) { function f() {",
        "label: for (;;) { function f() {",
        "for (;;) { async function f() {",
        "for (;;) { function* g() {",
        "for (;;) { (function () {",
        "for (;;) { (() => {",
        "for (;;) { class C { m() {",
        "for (;;) { class C { static {",
        "for (;;) { class C { static { for (;;) {",
        "for (;;) { function f() { for (;;) {",
        "for (;;) { function f() { switch (x) { case 1:",
        "switch (x) { case 1:",
        "switch (x) { case 1: for (;;) { function f() {",
        "class C { static { for (;;) { function f() {",
        "try {",
        "try { } catch (e) {",
        "try { } catch {",
        "try { } finally {",
        "for (;;) { try {",
        "for (;;) { try { } catch (e) {",
        "for (;;) { try { } finally {",
    ];

    fn assert_counters_clean(parser: &Parser<'_>, source: &str) {
        assert_eq!(parser.in_iteration, 0, "in_iteration leaked for {source:?}");
        assert_eq!(parser.in_switch, 0, "in_switch leaked for {source:?}");
        assert_eq!(parser.in_function, 0, "in_function leaked for {source:?}");
        assert!(
            !parser.in_block_or_function,
            "in_block_or_function leaked for {source:?}"
        );
        assert!(
            !parser.in_switch_case,
            "in_switch_case leaked for {source:?}"
        );
    }

    for source in SOURCES {
        let mut parser = Parser::new(source).unwrap();
        assert!(
            parser.parse_program().is_err(),
            "expected a parse error for {source:?}"
        );
        assert_counters_clean(&parser, source);
    }

    // `in_non_arrow_function` is not asserted above. Eight of its nine
    // decrements sit after a `?`, so an aborted parse skips them and simply
    // leaves the counter elevated — untidy but harmless. The ninth,
    // `parse_function_body_with_context`, decrements unconditionally, and
    // `export default function` is the only way to reach it. A static block
    // inside such a body resets the counter, so before the restores below
    // were made unconditional that pairing underflowed — on a path
    // `parse_program` cannot reach at all.
    const MODULE_SOURCES: &[&str] = &[
        "export default function () { class C { static {",
        "export default function* () { class C { static {",
    ];

    for source in MODULE_SOURCES {
        let mut parser = Parser::new(source).unwrap();
        assert!(
            parser.parse_program_as_module().is_err(),
            "expected a parse error for {source:?}"
        );
        assert_counters_clean(&parser, source);
        assert_eq!(
            parser.in_non_arrow_function, 0,
            "in_non_arrow_function leaked for {source:?}"
        );
    }
}

/// The combinator itself, independent of any parser construct: `enter`
/// mutates a couple of block-scope fields, `body` returns `Err`
/// unconditionally, and the fields must be back to their pre-call values
/// once `with_block_scope` returns — this is the structural guarantee
/// issue #608's retarget asked for (a restore a future call site cannot
/// forget, since there is no separate restore call to omit).
#[test]
fn with_block_scope_restores_on_err() {
    let mut parser = Parser::new("").unwrap();
    parser.in_block_or_function = false;
    parser.in_switch_case = true;
    let result = parser.with_block_scope(
        |p| {
            p.in_block_or_function = true;
            p.in_switch_case = false;
        },
        |_p| {
            Err::<(), ParseError>(ParseError {
                message: "boom".into(),
            })
        },
    );
    assert!(result.is_err());
    assert!(!parser.in_block_or_function);
    assert!(parser.in_switch_case);
}

#[test]
fn with_block_scope_restores_on_ok() {
    let mut parser = Parser::new("").unwrap();
    parser.in_block_or_function = false;
    parser.in_switch_case = true;
    let result = parser.with_block_scope(
        |p| {
            p.in_block_or_function = true;
            p.in_switch_case = false;
        },
        |p| {
            assert!(p.in_block_or_function);
            assert!(!p.in_switch_case);
            Ok(42)
        },
    );
    assert_eq!(result.unwrap(), 42);
    assert!(!parser.in_block_or_function);
    assert!(parser.in_switch_case);
}

#[test]
fn with_function_context_restores_on_err() {
    let mut parser = Parser::new("").unwrap();
    parser.in_generator = false;
    parser.in_block_or_function = false;
    let result = parser.with_function_context(
        |p| {
            p.in_generator = true;
            p.in_block_or_function = true;
        },
        |_p| {
            Err::<(), ParseError>(ParseError {
                message: "boom".into(),
            })
        },
    );
    assert!(result.is_err());
    assert!(!parser.in_generator);
    assert!(!parser.in_block_or_function);
}

#[test]
fn with_function_context_restores_on_ok() {
    let mut parser = Parser::new("").unwrap();
    parser.in_generator = false;
    parser.in_block_or_function = false;
    let result = parser.with_function_context(
        |p| {
            p.in_generator = true;
            p.in_block_or_function = true;
        },
        |p| {
            assert!(p.in_generator);
            assert!(p.in_block_or_function);
            Ok(7)
        },
    );
    assert_eq!(result.unwrap(), 7);
    assert!(!parser.in_generator);
    assert!(!parser.in_block_or_function);
}

/// A `using`/`await using` declaration must be rejected directly in a
/// `switch` case body (test262
/// `language/statements/using/syntax/using-invalid-switchstatement-*`):
/// the `switch` `CaseBlock` consequent's `with_block_scope` call is the
/// one site whose `enter` closure must not force `in_block_or_function`
/// true, or this parses successfully instead — the regression a
/// same-value-no-op assumption in an earlier draft of this combinator
/// carried forward silently until the full test262 run caught it.
#[test]
fn using_declaration_rejected_directly_in_switch_case() {
    for src in [
        "switch (x) { case 1: using y = z; }",
        "switch (x) { default: using y = z; }",
    ] {
        assert!(
            Parser::new(src).unwrap().parse_program().is_err(),
            "expected a parse error for {src:?}"
        );
    }
}

/// Source nested past `MAX_PARSE_DEPTH` must raise the catchable depth
/// error rather than exhausting the native stack first. The first four
/// shapes are the stack-hungriest measured when calibrating the constant,
/// so they bound every other shape the guard covers — but only those:
/// source that eats native stack without advancing the counter (a long
/// `a.b.b…` member chain, a long left-associative `1+1+1…` chain) is
/// outside this guard entirely, in both profiles.
///
/// `tests/recursion-limit-parser.js` covers the same shapes from JS against
/// a release binary; keep the two lists in step.
///
/// Note this cannot fail politely: if `MAX_PARSE_DEPTH` is ever raised
/// above what the running profile's stack holds, the process aborts
/// (SIGABRT) instead of reporting a failed assertion — jsse#599. Runs on
/// the engine stack because that is the stack the limit is calibrated
/// against; the default test-harness stack is far smaller.
#[test]
fn deep_nesting_raises_error_before_native_overflow() {
    // Twice the limit, so even the shape advancing the counter slowest
    // (one unit per repetition) is guaranteed to cross it.
    let reps = MAX_PARSE_DEPTH as usize * 2;
    for (label, source) in [
        ("array literal", "[".repeat(reps)),
        ("object literal", "({a:".repeat(reps)),
        ("object/array mix", "({a:[".repeat(reps)),
        ("template substitution", "`${".repeat(reps)),
        ("spread element", "[...".repeat(reps)),
        ("block statement", "{".repeat(reps)),
    ] {
        let err =
            parse_on_engine_stack(&source).expect_err("deeply nested source should be rejected");
        assert!(
            err.message.contains("nested too deeply"),
            "{label} nested {reps} deep should hit the parse-depth guard, got: {}",
            err.message
        );
    }
}

/// The limit must not be so tight that it rejects ordinary code: test262's
/// deepest nesting (64 brackets, ~192 units) parses in every profile.
#[test]
fn nesting_below_the_limit_still_parses() {
    let source = format!("{}1{}", "[".repeat(64), "]".repeat(64));
    assert!(
        parse_on_engine_stack(&source).is_ok(),
        "64-deep array nesting should parse"
    );
}

/// §16.1.1: a script's LexicallyDeclaredNames must not occur in its
/// VarDeclaredNames, and at the top level of a script function
/// declarations are var-declared (TopLevelVarDeclaredNames). The full
/// matrix lives in `test262-extra/script-toplevel-*.js`.
#[test]
fn script_top_level_function_and_lexical_redeclaration_is_an_early_error() {
    let src = "function benchmark(){} class B{} const benchmark = new B();";
    let err = parse_on_engine_stack(src).expect_err(src);
    assert!(err.message.contains("has already been declared"), "{err:?}");
}
