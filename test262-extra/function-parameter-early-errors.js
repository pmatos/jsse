/*---
description: Parameter early errors agree across function and method grammar forms
esid: sec-function-definitions-static-semantics-early-errors
info: |
  FunctionBodyContainsUseStrict forbids non-simple parameter lists.
  UniqueFormalParameters forbids duplicate bound names in methods, arrows,
  generators and async functions. Sloppy ordinary simple lists allow them.
flags: [noStrict]
features: [generators, async-functions, async-iteration, arrow-function, destructuring-binding]
---*/

var forms = [
  function (params, body) { return 'function f(' + params + ') {' + body + '}'; },
  function (params, body) { return '(function f(' + params + ') {' + body + '});'; },
  function (params, body) { return 'function* f(' + params + ') {' + body + '}'; },
  function (params, body) { return '(function* f(' + params + ') {' + body + '});'; },
  function (params, body) { return 'async function f(' + params + ') {' + body + '}'; },
  function (params, body) { return '(async function f(' + params + ') {' + body + '});'; },
  function (params, body) { return 'async function* f(' + params + ') {' + body + '}'; },
  function (params, body) { return '(async function* f(' + params + ') {' + body + '});'; },
  function (params, body) { return '({m(' + params + ') {' + body + '}});'; },
  function (params, body) { return '({*m(' + params + ') {' + body + '}});'; },
  function (params, body) { return '({async m(' + params + ') {' + body + '}});'; },
  function (params, body) { return '({async *m(' + params + ') {' + body + '}});'; },
  function (params, body) { return '(class {m(' + params + ') {' + body + '}});'; },
  function (params, body) { return '(class {*m(' + params + ') {' + body + '}});'; },
  function (params, body) { return '(class {async m(' + params + ') {' + body + '}});'; },
  function (params, body) { return '(class {async *m(' + params + ') {' + body + '}});'; },
  function (params, body) { return '((' + params + ') => {' + body + '});'; },
  function (params, body) { return '(async (' + params + ') => {' + body + '});'; },
  function (params, body) { return '({set m(' + params + ') {' + body + '}});'; }
];

forms.forEach(function (form, index) {
  ['a = 1', '...a', '{a}', '[a]'].forEach(function (params) {
    var source = form(params, '"use strict";');
    assert.throws(SyntaxError, function () { eval(source); }, source);
    // A rest setter has an independent arity restriction.
    if (index !== forms.length - 1 || params !== '...a') {
      eval(form(params, ''));
    }
  });

  var duplicate = form('a, a', '');
  if (index < 2) {
    eval(duplicate);
  } else {
    assert.throws(SyntaxError, function () { eval(duplicate); }, duplicate);
  }
  var strictDuplicate = '"use strict"; ' + duplicate;
  assert.throws(SyntaxError, function () { eval(strictDuplicate); }, strictDuplicate);
  eval(form('a', '"use strict";'));
});

// Dynamic constructors still parse parameter and body fragments independently.
var constructors = [
  Function,
  Object.getPrototypeOf(function* () {}).constructor,
  Object.getPrototypeOf(async function () {}).constructor,
  Object.getPrototypeOf(async function* () {}).constructor
];
constructors.forEach(function (Constructor) {
  assert.throws(SyntaxError, function () { Constructor('a = 1', '"use strict";'); });
  assert.throws(SyntaxError, function () { Constructor('a', 'a', '"use strict";'); });
  assert.throws(SyntaxError, function () { Constructor('a) { /*', '*/ }'); });
  Constructor('a = 1', 'return a;');
});
