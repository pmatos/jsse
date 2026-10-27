'use strict';

// The `serialization: 'advanced'` wire format. Node frames V8 ValueSerializer
// output behind a 4-byte big-endian length; jsse has no V8 serializer, so the
// frames (cut by the native channel) carry a structured-clone graph as tagged
// JSON text instead (the same value set, with shared references and cycles;
// not readable by Node).

const {
  ArrayBufferIsView,
  ArrayIsArray,
  DatePrototypeGetTime,
  JSONParse,
  JSONStringify,
  MapPrototypeEntries,
  ObjectKeys,
  SetPrototypeValues,
  TypedArrayPrototypeGetSymbolToStringTag,
} = primordials;

const { Buffer } = require('internal/host/buffer');
const {
  isArrayBuffer,
  isBoxedPrimitive,
  isDate,
  isMap,
  isRegExp,
  isSet,
} = require('internal/util/types');

const VIEWS = {
  __proto__: null,
  Buffer, Int8Array, Uint8Array, Uint8ClampedArray, Int16Array, Uint16Array,
  Int32Array, Uint32Array, Float32Array, Float64Array, BigInt64Array,
  BigUint64Array, DataView,
};
if (typeof Float16Array === 'function') VIEWS.Float16Array = Float16Array;
const ERRORS = {
  __proto__: null,
  Error, EvalError, RangeError, ReferenceError, SyntaxError, TypeError, URIError,
};

function cloneError(value) {
  const DOMException = globalThis.DOMException;
  const what = typeof value === 'symbol' ? value.toString() : `${value}`;
  return new DOMException(`${what} could not be cloned.`, 'DataCloneError');
}

function toBase64(buffer, offset, length) {
  return Buffer.from(buffer, offset, length).toString('base64');
}

function fromBase64(text) {
  const bytes = Buffer.from(text, 'base64');
  const out = new ArrayBuffer(bytes.length);
  new Uint8Array(out).set(bytes);
  return out;
}

function viewName(value) {
  if (Buffer.isBuffer(value)) return 'Buffer';
  if (value instanceof DataView) return 'DataView';
  return TypedArrayPrototypeGetSymbolToStringTag(value);
}

function encode(root) {
  const ids = new Map();

  function node(value) {
    switch (typeof value) {
      case 'undefined': return ['u'];
      case 'boolean':
      case 'string': return value;
      case 'number':
        return value - value === 0 && (value !== 0 || 1 / value > 0) ?
          value : ['n', value === 0 ? '-0' : `${value}`];
      case 'bigint': return ['b', `${value}`];
      case 'function':
      case 'symbol': throw cloneError(value);
    }
    if (value === null) return null;
    const seen = ids.get(value);
    if (seen !== undefined) return ['r', seen];
    const id = ids.size;
    ids.set(value, id);
    if (isDate(value)) return ['D', id, node(DatePrototypeGetTime(value))];
    if (isRegExp(value)) return ['R', id, value.source, value.flags];
    if (isArrayBuffer(value)) return ['A', id, toBase64(value, 0, value.byteLength)];
    if (ArrayBufferIsView(value)) {
      return ['V', id, viewName(value), toBase64(value.buffer, value.byteOffset, value.byteLength)];
    }
    if (isMap(value)) {
      const entries = [];
      for (const [k, v] of MapPrototypeEntries(value)) entries.push(node(k), node(v));
      return ['M', id, entries];
    }
    if (isSet(value)) {
      const items = [];
      for (const v of SetPrototypeValues(value)) items.push(node(v));
      return ['S', id, items];
    }
    if (value instanceof Error) {
      const out = ['E', id, value.name, value.message, value.stack ?? null, undefined];
      if ('cause' in value) out[5] = node(value.cause);
      return out;
    }
    if (isBoxedPrimitive(value)) return ['P', id, node(value.valueOf())];
    const keys = ObjectKeys(value);
    const members = [];
    for (let i = 0; i < keys.length; i++) members.push(keys[i], node(value[keys[i]]));
    return ArrayIsArray(value) ? ['a', id, value.length, members] : ['o', id, members];
  }

  return node(root);
}

function decode(root) {
  const objects = [];

  function fill(target, members) {
    for (let i = 0; i < members.length; i += 2) target[members[i]] = node(members[i + 1]);
    return target;
  }

  function node(value) {
    if (value === null || typeof value !== 'object') return value;
    const id = value[1];
    let out;
    switch (value[0]) {
      case 'u': return undefined;
      case 'n': return value[1] === '-0' ? -0 : +value[1];
      case 'b': return BigInt(value[1]);
      case 'r': return objects[id];
      case 'D': out = new Date(node(value[2])); break;
      case 'R': out = new RegExp(value[2], value[3]); break;
      case 'A': out = fromBase64(value[2]); break;
      case 'V': {
        const buffer = fromBase64(value[3]);
        out = value[2] === 'Buffer' ? Buffer.from(buffer) : new VIEWS[value[2]](buffer);
        break;
      }
      case 'M': {
        out = new Map();
        objects[id] = out;
        for (let i = 0; i < value[2].length; i += 2) out.set(node(value[2][i]), node(value[2][i + 1]));
        return out;
      }
      case 'S': {
        out = new Set();
        objects[id] = out;
        for (let i = 0; i < value[2].length; i++) out.add(node(value[2][i]));
        return out;
      }
      case 'E': {
        const Class = ERRORS[value[2]] ?? Error;
        out = new Class(value[3]);
        objects[id] = out;
        if (value[2] !== out.name) out.name = value[2];
        if (value[4] !== null) out.stack = value[4];
        if (value[5] !== undefined) out.cause = node(value[5]);
        return out;
      }
      case 'P': out = Object(node(value[2])); break;
      case 'a':
        out = [];
        out.length = value[2];
        objects[id] = out;
        return fill(out, value[3]);
      default:
        out = {};
        objects[id] = out;
        return fill(out, value[2]);
    }
    objects[id] = out;
    return out;
  }

  return node(root);
}

function serialize(message) {
  return JSONStringify(encode(message));
}

function deserialize(text) {
  return decode(JSONParse(text));
}

module.exports = { deserialize, serialize };
