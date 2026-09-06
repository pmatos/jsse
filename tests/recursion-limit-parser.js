// Deeply nested *source* must raise a catchable error at parse time, not
// overflow the native parser stack (SIGABRT). eval() parses at runtime and
// surfaces a parse failure as a catchable SyntaxError, so this is observable
// from within JS.

function nestedArray(d) {
  return "[".repeat(d) + "1" + "]".repeat(d);
}

// Expression nesting well beyond the parser depth limit -> catchable SyntaxError.
var threw = false;
var err = null;
try {
  eval(nestedArray(50000));
} catch (e) {
  threw = true;
  err = e;
}
if (!threw) {
  throw new Error("expected deeply nested array literal to throw at parse time");
}
if (!(err instanceof SyntaxError)) {
  throw new Error("expected a SyntaxError, got " + err.name + ": " + err.message);
}

// Reasonable nesting must still parse AND evaluate — the limit must not reject
// ordinary (if deep) code. This depth assumes the release limit, which is what
// the custom-test runner uses; a debug build's MAX_PARSE_DEPTH is ~10x lower
// because its stack frames are that much larger (jsse#599).
var arr = eval(nestedArray(1000));
if (!Array.isArray(arr)) {
  throw new Error("moderately nested array literal should parse and evaluate");
}

// Statement nesting (nested blocks) must be catchable too, not crash.
var threw2 = false;
try {
  eval("{".repeat(50000) + "}".repeat(50000));
} catch (e) {
  threw2 = true;
}
if (!threw2) {
  throw new Error("expected deeply nested blocks to throw at parse time");
}

// Recursive expression productions that bypass parse_assignment_expression
// (prefix unary, right-associative **, and `new new …`) must be bounded too —
// otherwise a single assignment expression recurses natively and aborts.
function mustThrow(label, src) {
  var threw = false;
  try {
    eval(src);
  } catch (e) {
    threw = true;
  }
  if (!threw) {
    throw new Error("expected " + label + " to throw at parse time");
  }
}
mustThrow("prefix unary chain", "!".repeat(200000) + "0");
mustThrow("unary minus chain", "- ".repeat(200000) + "0");
mustThrow("exponentiation chain", "2" + "**2".repeat(200000));
mustThrow("new-expression chain", "new ".repeat(200000) + "X");

// The shapes above all reach the guard cheaply. These spend the most native
// stack per unit of parse depth (kept in step with the same list in
// `parser::tests::deep_nesting_raises_error_before_native_overflow`), so they
// are the ones that reach the native limit first if the guard is set too high
// (jsse#599) — object literal and template forms cost several units per level.
mustThrow("object literal nesting", "({a:".repeat(50000));
mustThrow("object/array mix", "({a:[".repeat(50000));
mustThrow("template substitution nesting", "`${".repeat(50000));
mustThrow("spread nesting", "[...".repeat(50000));
