//! V8's text for the early errors whose message needs no parser state, keyed
//! by the message this parser raises (`--node` only, issues #1484, #1523).

pub(super) const INVALID_TOKEN: &str = "Invalid or unexpected token";
const NON_STRICT_FUNCTION: &str = "In non-strict mode code, functions can only be declared at top level, inside a block, or as the body of an if statement.";

pub(super) const EXACT: &[(&str, &str)] = &[
    (
        "'let' is not allowed as a variable name in lexical declarations",
        "let is disallowed as a lexically bound name",
    ),
    (
        "Rest element may not have a default initializer",
        "Invalid destructuring assignment target",
    ),
    (
        "'let' is not allowed as a binding name in lexical declarations",
        "let is disallowed as a lexically bound name",
    ),
    (
        "#constructor is not a valid private name",
        "Private field '#constructor' must be declared in an enclosing class",
    ),
    ("Expected decorator expression after '@'", INVALID_TOKEN),
    ("Expected 'class' after decorator", INVALID_TOKEN),
    (
        "Illegal continue statement",
        "Illegal continue statement: no surrounding iteration statement",
    ),
    (
        "Delete of an unqualified identifier in strict mode",
        "Delete of an unqualified identifier in strict mode.",
    ),
    (
        "Assignment to 'eval' or 'arguments' in strict mode",
        "Unexpected eval or arguments in strict mode",
    ),
    (
        "Octal literals are not allowed in strict mode",
        "Octal literals are not allowed in strict mode.",
    ),
    (
        "Octal escape sequences are not allowed in strict mode",
        "Octal escape sequences are not allowed in strict mode.",
    ),
    (
        "In strict mode code, functions can only be declared at top level or inside a block",
        "In strict mode code, functions can only be declared at top level or inside a block.",
    ),
    (
        "Async function declaration cannot appear in a single-statement context",
        "Async functions can only be declared at the top level or inside a block.",
    ),
    (
        "Generators can only be declared at the top level or inside a block",
        "Generators can only be declared at the top level or inside a block.",
    ),
    (
        "The identifier 'async' is disallowed as a left-hand side expression of a for-of loop",
        "The left-hand side of a for-of loop may not be 'async'.",
    ),
    (
        "for-of loop variable declaration may not have an initializer",
        "for-of loop variable declaration may not have an initializer.",
    ),
    (
        "for-await-of loop variable declaration may not have an initializer",
        "for-await-of loop variable declaration may not have an initializer.",
    ),
    (
        "for-in loop variable declaration may not have an initializer",
        "for-in loop variable declaration may not have an initializer.",
    ),
    (
        "for await...of is only valid in async functions or at module top level",
        "Unexpected reserved word",
    ),
    (
        "Getter must not have any formal parameters",
        "Getter must not have any formal parameters.",
    ),
    (
        "Setter must have exactly one formal parameter",
        "Setter must have exactly one formal parameter.",
    ),
    (
        "Class fields and methods cannot be named '#constructor'",
        "Classes may not have a field named 'constructor'",
    ),
    (
        "Class field initializer cannot reference 'arguments'",
        "'arguments' is not allowed in class field initializer or static initialization block",
    ),
    (
        "'arguments' is not allowed in class static blocks",
        "'arguments' is not allowed in class field initializer or static initialization block",
    ),
    (
        "Applying the 'delete' operator to a private name is not allowed",
        "Private fields can not be deleted",
    ),
    (
        "Tagged template cannot be used in an optional chain",
        "Invalid tagged template on optional chain",
    ),
    (
        "Await expression is not allowed in formal parameters of an async function",
        "Illegal await-expression in formal parameters of async function",
    ),
    (
        "Yield expression is not allowed in formal parameters of a generator function",
        "Yield expression not allowed in formal parameter",
    ),
    (
        "'await' is not allowed in async arrow formal parameters",
        "'await' is not a valid identifier name in an async function",
    ),
    (
        "Octal escape sequences are not allowed in template literals",
        "Octal escape sequences are not allowed in template strings.",
    ),
    (
        "\\8 and \\9 are not allowed in template literals",
        "\\8 and \\9 are not allowed in template strings.",
    ),
    (
        "\\8 and \\9 are not allowed in strict mode",
        "\\8 and \\9 are not allowed in strict mode.",
    ),
    (
        "Unary operator used immediately before exponentiation expression. Parenthesization is required",
        "Unary operator used immediately before exponentiation expression. Parenthesis must be used to disambiguate operator precedence",
    ),
    (
        "Cannot use 'new' with import()",
        "Cannot use new with import",
    ),
    (
        "import.meta is only valid in module code",
        "Cannot use 'import.meta' outside a module",
    ),
    ("Unexpected token )", "Unexpected token ')'"),
    (
        "An imported name that is not a valid binding identifier requires 'as'",
        "Unexpected reserved word",
    ),
    (
        "Invalid destructuring target",
        "Invalid destructuring assignment target",
    ),
    (
        "for await...in is not valid; use for await...of",
        "Unexpected token 'in'",
    ),
    (
        "Private fields are not accessible on 'super'",
        "Unexpected private field",
    ),
    (
        "Function declaration not allowed in statement position",
        NON_STRICT_FUNCTION,
    ),
    (
        "Labelled function declaration not allowed in with statement",
        NON_STRICT_FUNCTION,
    ),
    (
        "In non-strict mode code, functions can only be declared at top level, inside a block, or as the body of an if statement",
        NON_STRICT_FUNCTION,
    ),
    (
        "A string literal cannot be used as an exported local name without 'from'",
        "String literal module export names must be followed by a 'from' clause",
    ),
    (
        "An export name cannot include a lone surrogate.",
        "Invalid module export name: contains unpaired surrogate",
    ),
    (
        "'await' is not allowed as an identifier in this context",
        "Unexpected reserved word",
    ),
    (
        "'yield' is not allowed as an identifier in this context",
        "Unexpected identifier 'yield'",
    ),
    (
        "Expected declaration after export",
        "Unexpected token 'export'",
    ),
    (
        "Unexpected 'import'",
        "Cannot use import statement outside a module",
    ),
    (
        "A switch statement may only have one default clause",
        "More than one default clause in switch statement",
    ),
    (
        "Expected '}' after template substitution",
        "Missing } in template expression",
    ),
    ("Invalid Unicode escape", "Invalid Unicode escape sequence"),
    (
        "Unterminated Unicode escape",
        "Invalid Unicode escape sequence",
    ),
    ("Invalid hex escape", "Invalid hexadecimal escape sequence"),
    (
        "Unicode code point out of range",
        "Undefined Unicode code-point",
    ),
    ("Unterminated template literal", "Unexpected end of input"),
    (
        "Unterminated regular expression",
        "Invalid regular expression: missing /",
    ),
    (
        "Numeric separator must be exactly one underscore",
        "Only one underscore is allowed as numeric separator",
    ),
    (
        "Trailing numeric separator",
        "Numeric separators are not allowed at the end of numeric literals",
    ),
    (
        "Numeric separator cannot appear after leading 0",
        "Numeric separator can not be used after leading 0.",
    ),
];
