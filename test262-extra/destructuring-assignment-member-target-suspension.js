/*---
description: Member targets preserve generator suspension during destructuring assignment
esid: sec-runtime-semantics-destructuringassignmentevaluation
features: [generators, destructuring-binding]
info: |
  MemberExpression : MemberExpression [ Expression ]
    1. Let baseReference be ? Evaluation of MemberExpression.
    2. Let baseValue be ? GetValue(baseReference).
    3. Return ? EvaluatePropertyAccessWithExpressionKey(baseValue, Expression, strict).

  AssignmentElement : DestructuringAssignmentTarget Initializer?
    1. If DestructuringAssignmentTarget is not a pattern, let lRef be
       ? Evaluation of DestructuringAssignmentTarget.

  AssignmentRestElement : ... DestructuringAssignmentTarget
    1. If DestructuringAssignmentTarget is not a pattern, let lRef be
       ? Evaluation of DestructuringAssignmentTarget.

  KeyedDestructuringAssignmentEvaluation evaluates its non-pattern target
  reference before reading the source property.
---*/

var arrayElementSource = [41];
var arrayElementTarget = {};
function* assignArrayElementBase() {
  return [(yield "array element base").slot] = arrayElementSource;
}

var iter = assignArrayElementBase();
var step = iter.next();
assert.sameValue(step.value, "array element base");
assert.sameValue(step.done, false);
step = iter.next(arrayElementTarget);
assert.sameValue(step.value, arrayElementSource);
assert.sameValue(step.done, true);
assert.sameValue(arrayElementTarget.slot, 41);

var arrayRestSource = [42, 43];
var arrayRestTarget = {};
function* assignArrayRestBase() {
  return [...(yield "array rest base").slot] = arrayRestSource;
}

iter = assignArrayRestBase();
step = iter.next();
assert.sameValue(step.value, "array rest base");
assert.sameValue(step.done, false);
step = iter.next(arrayRestTarget);
assert.sameValue(step.value, arrayRestSource);
assert.sameValue(step.done, true);
assert.compareArray(arrayRestTarget.slot, [42, 43]);

var objectSource = { value: 44 };
var objectTarget = {};
function* assignObjectPropertyBase() {
  return { value: (yield "object property base").slot } = objectSource;
}

iter = assignObjectPropertyBase();
step = iter.next();
assert.sameValue(step.value, "object property base");
assert.sameValue(step.done, false);
step = iter.next(objectTarget);
assert.sameValue(step.value, objectSource);
assert.sameValue(step.done, true);
assert.sameValue(objectTarget.slot, 44);

class Parent {}
class Child extends Parent {
  *assign(source) {
    return [super[yield "super key"]] = source;
  }
}

var child = new Child();
var superSource = [45];
iter = child.assign(superSource);
step = iter.next();
assert.sameValue(step.value, "super key");
assert.sameValue(step.done, false);
step = iter.next("slot");
assert.sameValue(step.value, superSource);
assert.sameValue(step.done, true);
assert.sameValue(child.slot, 45);
