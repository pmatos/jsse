// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-functiondeclarationinstantiation
description: >
  A parameter default that names a later parameter hits that parameter's
  temporal dead zone, even when an outer binding of the same name exists.
info: |
  10.2.11 FunctionDeclarationInstantiation
    21. For each String paramName of parameterNames, do
        a. Let alreadyDeclared be ! env.HasBinding(paramName).
        b. If alreadyDeclared is false, then
           i. Perform ! env.CreateMutableBinding(paramName, false).
           ii. If hasDuplicates is true, then
               1. Perform ! env.InitializeBinding(paramName, undefined).
    26. If hasParameterExpressions is false, then ... Else,
        a. Perform ? IteratorBindingInitialization of formals with arguments
           iteratorRecord and environment env.
---*/

var b = "outer";

assert.throws(ReferenceError, function () {
  (function (a = b, b) { return a; })();
}, "plain later parameter");

assert.throws(ReferenceError, function () {
  (function (a = b, b = 1) { return a; })();
}, "later parameter with a default");

assert.throws(ReferenceError, function () {
  (function (a = b, ...b) { return a; })();
}, "later rest parameter");

assert.throws(ReferenceError, function () {
  ((a = b, b) => a)();
}, "arrow function");

assert.throws(ReferenceError, function () {
  ({ m(a = b, b) { return a; } }).m();
}, "method");

assert.throws(ReferenceError, function () {
  (function* (a = b, b) {})();
}, "generator");

assert.throws(ReferenceError, function () {
  (function (a = eval("b"), b) { return a; })();
}, "direct eval in the default");

assert.throws(ReferenceError, function () {
  (function ({ x = b }, b) { return x; })({});
}, "destructuring parameter default naming a later parameter");

assert.throws(ReferenceError, function () {
  (function (a, { x = b }, [b]) { return x; })(1, {}, [2]);
}, "later name bound by an array pattern");

assert.sameValue(
  (function (a, b = a) { return b; })(7),
  7,
  "an earlier parameter is initialized"
);

assert.sameValue(
  (function (a = function () { return b; }, b = 3) { return a(); })(),
  3,
  "a closure in a default reads the parameter once initialized"
);

assert.sameValue(
  (function (a = b, b) { return a; }).length,
  0,
  "length counts only the parameters before the first default"
);

var supplied = (function (a = b, b) { return a; })(5);
assert.sameValue(supplied, 5, "a supplied argument skips the default");
