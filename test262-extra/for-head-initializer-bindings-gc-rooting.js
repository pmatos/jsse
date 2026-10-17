// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-for-statement-runtime-semantics-forloopevaluation
description: >
  The bindings a `for (let|const|using ...; ...)` head has already initialised
  stay reachable while a later declarator's initializer runs arbitrary code
  (and so can trigger a garbage collection), including a `using` head's
  pending resources that are disposed when the loop exits.
info: |
  ForStatement : for ( LexicalDeclaration Expression_opt ; Expression_opt ) Statement

  2. Let oldEnv be the running execution context's LexicalEnvironment.
  3. Let loopEnv be NewDeclarativeEnvironment(oldEnv).
  [...]
  7. Let forDcl be Completion(Evaluation of LexicalDeclaration).
  [...]
  Each later BindingList declarator is evaluated in loopEnv after the earlier
  ones were initialised, and loopEnv is otherwise referenced only by the
  running execution context.
features: [explicit-resource-management, host-gc-required]
---*/

function collect() {
  $262.gc();
  var churn = [];
  for (var i = 0; i < 500; i++) {
    churn.push({ churn: i });
  }
  return churn.length;
}

var log = [];
function disposer(name) {
  return { [Symbol.dispose]() { log.push(name); } };
}

for (using a = disposer('A'), b = (collect(), disposer('B')); false; ) {}
assert.sameValue(log.join(), 'B,A', 'using head: resources initialised before a collection are disposed in reverse order');

log = [];
for (using a = disposer('A'), b = disposer('B'), c = (collect(), disposer('C')); false; ) {}
assert.sameValue(log.join(), 'C,B,A', 'using head: three declarators');

var seen;
for (let p = { v: [1, 2, 3] }, q = collect(); ; ) {
  seen = p.v.join();
  break;
}
assert.sameValue(seen, '1,2,3', 'let head: an earlier declarator survives a collection in a later initializer');

for (const p = { v: [4, 5, 6] }, q = collect(); ; ) {
  seen = p.v.join();
  break;
}
assert.sameValue(seen, '4,5,6', 'const head: an earlier declarator survives a collection in a later initializer');

function inFunction() {
  var captured;
  for (let p = { v: [7, 8, 9] }, f = (collect(), function () { return p.v.length; }); ; ) {
    captured = f();
    break;
  }
  return captured;
}
assert.sameValue(inFunction(), 3, 'let head: a closure created after a collection sees the earlier declarator');

log = [];
try {
  for (using a = disposer('A'), b = (collect(), (function () { throw new Error('boom'); })()); false; ) {}
} catch (e) {
  log.push('caught-' + e.message);
}
assert.sameValue(log.join(), 'A,caught-boom', 'using head: a throwing later initializer disposes the earlier resource');
