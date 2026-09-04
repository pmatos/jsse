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
// ordinary (if deep) code.
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
