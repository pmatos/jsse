// Host console.error/warn/info/debug (WHATWG Console Standard): each is a
// zero-arity function that never throws and always returns undefined,
// regardless of argument count. Stream routing (stderr vs stdout) is covered
// by tests/console_stderr_routing.rs since it isn't observable from JS.

function sameValue(actual, expected, label) {
  if (!Object.is(actual, expected)) {
    throw new Error(label + ": expected " + String(expected) +
      ", got " + String(actual));
  }
}

for (const name of ["error", "warn", "info", "debug"]) {
  const fn = console[name];
  sameValue(typeof fn, "function", "typeof console." + name);
  sameValue(fn.length, 0, "console." + name + ".length");
  sameValue(fn.call(console), undefined, "console." + name + "() with no args");
  sameValue(fn.call(console, "a"), undefined, "console." + name + "() with one arg");
  sameValue(fn.call(console, "a", 1, { toString() { return "obj"; } }), undefined,
    "console." + name + "() with multiple args");
}
