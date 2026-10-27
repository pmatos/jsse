'use strict';

const { ArrayPrototypeJoin, ArrayPrototypePush, ObjectDefineProperty } = primordials;

// require(esm) error codes (#1241), kept apart so internal/errors stays under the file budget.
module.exports = function defineRequireEsmErrors(E) {
  E('ERR_REQUIRE_ASYNC_MODULE', Error, function(filename, parent) {
    let message = 'require() cannot be used on an ESM graph with top-level await. Use import() instead. ' +
      'To see where the top-level await comes from, use --experimental-print-required-tla.';
    if (filename) message += `\nRequired module: ${filename}`;
    const stack = [];
    for (let cursor = parent; cursor; cursor = cursor.parent) {
      ArrayPrototypePush(stack, cursor.filename || cursor.id);
    }
    if (stack.length > 0) message += `\nRequire stack:\n- ${ArrayPrototypeJoin(stack, '\n- ')}`;
    ObjectDefineProperty(this, 'requireStack', {
      value: stack, writable: true, enumerable: false, configurable: true,
    });
    return message;
  });
  E('ERR_REQUIRE_CYCLE_MODULE', Error, (id, parent) => {
    return `Cannot require() ES Module ${id} in a cycle.${parent ? ` (from ${parent.filename})` : ''} ` +
      'A cycle involving require(esm) is not allowed to maintain invariants mandated by the ' +
      'ECMAScript specification. Try making at least part of the dependency in the graph lazily loaded.';
  });
};
