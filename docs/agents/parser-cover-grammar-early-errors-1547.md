# Parser early errors for cover grammars and module names (#1547)

Sources the parser used to accept although the grammar or an early-error rule rejects them. Each form has a negative `test262-extra/` test; `arrow-parameters-cover-grammar-valid-forms.js` and `module-var-declared-names-valid-forms.js` pin the neighbouring forms that must stay valid.

| Source | Rule | Where it is enforced |
| --- | --- | --- |
| `var x = (a, )`, `x = (a, ...b)` | the trailing comma and the rest element of `CoverParenthesizedExpressionAndArrowParameterList` need a following `=>` | `parse_primary_expression`, `Token::LeftParen` arm (`arrow_only`) |
| `(a.b) => 1`, `((a)) => 1`, `([a.b]) => 1`, `({a: b.c}) => 1`, `async ((a)) => 1`, `({...{a}}) => 1` | the cover list is refined to `ArrowFormalParameters`; members, parenthesised bindings and a pattern under an object rest are not `BindingElement`s | `src/parser/arrow_params.rs` |
| `try {} catch ([e]) { var e; }` | B.3.4: only a plain `BindingIdentifier` catch parameter may be redeclared by `var` | `src/parser/catch_param.rs` |
| `({...{a}} = {})`, `for ({...[a]} of [])` | `AssignmentRestProperty` target must not be an object or array literal | `validate_destructuring_pattern` (`expressions/targets.rs`) and `expr_to_pattern` (for-in/of heads) |
| `var {...{a}} = {}`, `function f({...{a}}) {}` | `BindingRestProperty : ... BindingIdentifier` | `parse_object_pattern` (`declarations/patterns.rs`) |
| module `let x; { var x; }` | 16.2.1.1: `VarDeclaredNames` of the `ModuleItemList` includes nested vars | `src/parser/module_early_errors.rs` (`collect_module_var_names` uses `collect_var_declared_names`) |

## Arrow parameter refinement

The parenthesised list is first parsed as expressions (it cannot be known to be arrow parameters until `)` `=>` is seen), then `expr_to_pattern` converts it. That conversion is permissive, so `arrow_params_from_cover` also rewinds to the opening parenthesis (`TokenMark`: a cloned lexer plus the `(` token, no allocation) and parses the same tokens as `FormalParameters`, which is what the specification does. The re-parse only validates; the converted patterns are kept and the re-parse's own error is reported. Two guards keep it cheap and side-effect free:

- a list of nothing but identifiers and commas (`(a, b) =>`) is not re-parsed (`is_plain_identifier_list`);
- `refining_arrow_params` stops nested arrows in defaults from re-parsing again, so nesting costs a constant factor per arrow, not an exponential one, and stops `infer_assigned_name` recording `--node` stack names a second time (`tests/node_arrow_param_name_inference_1547.rs`).

## Error text

Message text is implementation-defined. `eval`, `Function`, the `AsyncFunction`/`GeneratorFunction`/`AsyncGeneratorFunction` constructors, `$262.evalScript` and `ShadowRealm#evaluate` throw a `SyntaxError` whose `message` is the parser message without a `SyntaxError: ` prefix (`tests/syntax-error-message-prefix-1547.js`).
