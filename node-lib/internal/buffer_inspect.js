'use strict';

// Buffer.prototype[util.inspect.custom] from Node's lib/buffer.js: the hex
// dump plus any extra own properties, installed by the builtin loader.

const {
  ArrayPrototypeForEach,
  MathMin,
  NumberPrototypeToString,
  ObjectAssign,
  StringPrototypeSlice,
} = primordials;
const { ALL_PROPERTIES, ONLY_ENUMERABLE } = require('internal/binding/util').constants;
const { getOwnNonIndexProperties } = require('internal/binding/util');

const INSPECT_MAX_BYTES = 50;

function inspect(recurseTimes, ctx, utilInspect) {
  const actualMax = MathMin(INSPECT_MAX_BYTES, this.length);
  const remaining = this.length - INSPECT_MAX_BYTES;
  let str = '';
  for (let i = 0; i < actualMax; i++) {
    str += (i > 0 ? ' ' : '') + (this[i] < 16 ? '0' : '') + NumberPrototypeToString(this[i], 16);
  }
  if (remaining > 0) {
    str += ` ... ${remaining} more byte${remaining > 1 ? 's' : ''}`;
  }
  if (ctx) {
    let extras = false;
    const filter = ctx.showHidden ? ALL_PROPERTIES : ONLY_ENUMERABLE;
    const obj = { __proto__: null };
    ArrayPrototypeForEach(getOwnNonIndexProperties(this, filter), (key) => {
      extras = true;
      obj[key] = this[key];
    });
    if (extras) {
      if (this.length !== 0) str += ', ';
      // '[Object: null prototype] {'.length === 26
      str += StringPrototypeSlice(utilInspect(obj, ObjectAssign({}, ctx, {
        breakLength: Infinity,
        compact: true,
      })), 27, -2);
    }
  }
  return `<${this.constructor.name} ${str}>`;
}

module.exports = { inspect };
