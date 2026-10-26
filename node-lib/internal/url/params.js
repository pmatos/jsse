'use strict';

// URLSearchParams and its iterator, from Node v26.8.2 lib/internal/url.js
// (MIT). URL (internal/url/url.js) reaches the private list through `hooks`.

const {
  Array,
  ArrayIsArray,
  ArrayPrototypeJoin,
  ArrayPrototypeMap,
  ArrayPrototypePush,
  Int8Array,
  ObjectDefineProperties,
  ObjectSetPrototypeOf,
  ReflectGetOwnPropertyDescriptor,
  ReflectOwnKeys,
  StringPrototypeCharCodeAt,
  StringPrototypeIncludes,
  StringPrototypeToWellFormed,
  SymbolIterator,
  SymbolToStringTag,
  decodeURIComponent,
} = primordials;

const { inspect } = require('internal/util/inspect');
const { encodeStr, hexTable, isHexTable } = require('internal/querystring');
const {
  codes: {
    ERR_ARG_NOT_ITERABLE,
    ERR_INVALID_THIS,
    ERR_INVALID_TUPLE,
    ERR_MISSING_ARGS,
  },
} = require('internal/errors');
const {
  IteratorPrototype,
  hooks,
  kEnumerableProperty,
  kInspect,
  removeColors,
  validateFunction,
} = require('internal/url/util');
const querystring = require('querystring');

class URLSearchParamsIterator {
  #target;
  #kind;
  #index;

  // https://heycam.github.io/webidl/#dfn-default-iterator-object
  constructor(target, kind) {
    this.#target = target;
    this.#kind = kind;
    this.#index = 0;
  }

  next() {
    if (typeof this !== 'object' || this === null || !(#target in this))
      throw new ERR_INVALID_THIS('URLSearchParamsIterator');

    const index = this.#index;
    const values = hooks.getURLSearchParamsList(this.#target);
    const len = values.length;
    if (index >= len) {
      return {
        done: true,
        value: undefined,
      };
    }

    const name = values[index];
    const value = values[index + 1];
    this.#index = index + 2;

    let result;
    if (this.#kind === 'key') {
      result = name;
    } else if (this.#kind === 'value') {
      result = value;
    } else {
      result = [name, value];
    }

    return {
      done: false,
      value: result,
    };
  }

  [kInspect](recurseTimes, ctx) {
    if (!this || typeof this !== 'object' || !(#target in this))
      throw new ERR_INVALID_THIS('URLSearchParamsIterator');

    if (typeof recurseTimes === 'number' && recurseTimes < 0)
      return ctx.stylize('[Object]', 'special');

    const innerOpts = { ...ctx };
    if (recurseTimes !== null) {
      innerOpts.depth = recurseTimes - 1;
    }
    const index = this.#index;
    const values = hooks.getURLSearchParamsList(this.#target);
    const output = [];
    for (let i = index; i < values.length; i++) {
      const isKey = ((i - index) % 2) === 0;
      if (this.#kind === 'key') {
        if (isKey) ArrayPrototypePush(output, values[i]);
      } else if (!isKey) {
        ArrayPrototypePush(output, this.#kind === 'value' ? values[i] : [values[i - 1], values[i]]);
      }
    }
    const hasBreak = StringPrototypeIncludes(inspect(output, innerOpts), '\n');
    const outputStrs = ArrayPrototypeMap(output, (p) => inspect(p, innerOpts));
    let outputStr;
    if (hasBreak) {
      outputStr = `\n  ${ArrayPrototypeJoin(outputStrs, ',\n  ')}`;
    } else {
      outputStr = ` ${ArrayPrototypeJoin(outputStrs, ', ')}`;
    }
    return `${this[SymbolToStringTag]} {${outputStr} }`;
  }
}

// https://heycam.github.io/webidl/#dfn-iterator-prototype-object
delete URLSearchParamsIterator.prototype.constructor;
ObjectSetPrototypeOf(URLSearchParamsIterator.prototype, IteratorPrototype);

ObjectDefineProperties(URLSearchParamsIterator.prototype, {
  [SymbolToStringTag]: { __proto__: null, configurable: true, value: 'URLSearchParams Iterator' },
  next: kEnumerableProperty,
});

class URLSearchParams {
  #searchParams = [];

  #context;

  #serialized;

  static {
    hooks.setURLSearchParamsContext = (obj, ctx) => {
      obj.#context = ctx;
    };
    hooks.getURLSearchParamsList = (obj) => obj.#searchParams;
    hooks.setURLSearchParams = (obj, query) => {
      obj.#serialized = undefined;
      if (query === undefined) {
        obj.#searchParams = [];
      } else {
        obj.#searchParams = parseParams(query);
      }
    };
  }

  #markUpdated() {
    this.#serialized = undefined;
    if (this.#context) {
      hooks.setURLSearchParamsModified(this.#context);
    }
  }

  constructor(init = undefined) {
    if (init === undefined) {
    } else if (init !== null && (typeof init === 'object' || typeof init === 'function')) {
      const method = init[SymbolIterator];
      if (method === this[SymbolIterator] && #searchParams in init) {
        const childParams = init.#searchParams;
        this.#searchParams = childParams.slice();
        this.#serialized = init.#serialized;
      } else if (method != null) {
        if (typeof method !== 'function') {
          throw new ERR_ARG_NOT_ITERABLE('Query pairs');
        }

        for (const pair of init) {
          if (pair == null) {
            throw new ERR_INVALID_TUPLE('Each query pair', '[name, value]');
          } else if (ArrayIsArray(pair)) {
            if (pair.length !== 2) {
              throw new ERR_INVALID_TUPLE('Each query pair', '[name, value]');
            }
            ArrayPrototypePush(
              this.#searchParams,
              toUSVString(pair[0]),
              toUSVString(pair[1]),
            );
          } else {
            if (((typeof pair !== 'object' && typeof pair !== 'function') ||
                typeof pair[SymbolIterator] !== 'function')) {
              throw new ERR_INVALID_TUPLE('Each query pair', '[name, value]');
            }

            let length = 0;

            for (const element of pair) {
              length++;
              ArrayPrototypePush(this.#searchParams, toUSVString(element));
            }

            if (length !== 2) {
              throw new ERR_INVALID_TUPLE('Each query pair', '[name, value]');
            }
          }
        }
      } else {
        const visited = new Map();
        const keys = ReflectOwnKeys(init);
        for (let i = 0; i < keys.length; i++) {
          const key = keys[i];
          const desc = ReflectGetOwnPropertyDescriptor(init, key);
          if (desc !== undefined && desc.enumerable) {
            const typedKey = toUSVString(key);
            const typedValue = toUSVString(init[key]);

            const keyIdx = visited.get(typedKey);
            if (keyIdx !== undefined) {
              this.#searchParams[keyIdx] = typedValue;
            } else {
              visited.set(typedKey, ArrayPrototypePush(this.#searchParams,
                                                       typedKey,
                                                       typedValue) - 1);
            }
          }
        }
      }
    } else {
      // https://url.spec.whatwg.org/#dom-urlsearchparams-urlsearchparams
      init = toUSVString(init);
      this.#searchParams = init ? parseParams(init) : [];
    }
  }

  [kInspect](recurseTimes, ctx) {
    if (typeof this !== 'object' || this === null || !(#searchParams in this))
      throw new ERR_INVALID_THIS('URLSearchParams');

    if (typeof recurseTimes === 'number' && recurseTimes < 0)
      return ctx.stylize('[Object]', 'special');

    const separator = ', ';
    const innerOpts = { ...ctx };
    if (recurseTimes !== null) {
      innerOpts.depth = recurseTimes - 1;
    }
    const innerInspect = (v) => inspect(v, innerOpts);

    const list = this.#searchParams;
    const output = [];
    for (let i = 0; i < list.length; i += 2)
      ArrayPrototypePush(
        output,
        `${innerInspect(list[i])} => ${innerInspect(list[i + 1])}`);

    let length = -separator.length;
    for (let i = 0; i < output.length; i++) {
      length += removeColors(output[i]).length + separator.length;
    }
    if (length > ctx.breakLength) {
      return `${this.constructor.name} {\n` +
      `  ${ArrayPrototypeJoin(output, ',\n  ')} }`;
    } else if (output.length) {
      return `${this.constructor.name} { ` +
      `${ArrayPrototypeJoin(output, separator)} }`;
    }
    return `${this.constructor.name} {}`;
  }

  get size() {
    if (typeof this !== 'object' || this === null || !(#searchParams in this))
      throw new ERR_INVALID_THIS('URLSearchParams');
    return this.#searchParams.length / 2;
  }

  append(name, value) {
    if (typeof this !== 'object' || this === null || !(#searchParams in this))
      throw new ERR_INVALID_THIS('URLSearchParams');

    if (arguments.length < 2) {
      throw new ERR_MISSING_ARGS('name', 'value');
    }

    name = toUSVString(name);
    value = toUSVString(value);
    ArrayPrototypePush(this.#searchParams, name, value);
    this.#markUpdated();
  }

  delete(name, value = undefined) {
    if (typeof this !== 'object' || this === null || !(#searchParams in this))
      throw new ERR_INVALID_THIS('URLSearchParams');

    if (arguments.length < 1) {
      throw new ERR_MISSING_ARGS('name');
    }

    const list = this.#searchParams;
    name = toUSVString(name);
    const { length } = list;
    let write = 0;

    if (value !== undefined) {
      value = toUSVString(value);
      for (let i = 0; i < length; i += 2) {
        if (list[i] === name && list[i + 1] === value) {
          continue;
        }
        if (write !== i) {
          list[write] = list[i];
          list[write + 1] = list[i + 1];
        }
        write += 2;
      }
    } else {
      for (let i = 0; i < length; i += 2) {
        if (list[i] === name) {
          continue;
        }
        if (write !== i) {
          list[write] = list[i];
          list[write + 1] = list[i + 1];
        }
        write += 2;
      }
    }

    if (write !== length) {
      list.length = write;
      this.#serialized = undefined;
    }

    if (this.#context) {
      hooks.setURLSearchParamsModified(this.#context);
    }
  }

  get(name) {
    if (typeof this !== 'object' || this === null || !(#searchParams in this))
      throw new ERR_INVALID_THIS('URLSearchParams');

    if (arguments.length < 1) {
      throw new ERR_MISSING_ARGS('name');
    }

    const list = this.#searchParams;
    name = toUSVString(name);
    const { length } = list;
    for (let i = 0; i < length; i += 2) {
      if (list[i] === name) {
        return list[i + 1];
      }
    }
    return null;
  }

  getAll(name) {
    if (typeof this !== 'object' || this === null || !(#searchParams in this))
      throw new ERR_INVALID_THIS('URLSearchParams');

    if (arguments.length < 1) {
      throw new ERR_MISSING_ARGS('name');
    }

    const list = this.#searchParams;
    const values = [];
    name = toUSVString(name);
    const { length } = list;
    for (let i = 0; i < length; i += 2) {
      if (list[i] === name) {
        ArrayPrototypePush(values, list[i + 1]);
      }
    }
    return values;
  }

  has(name, value = undefined) {
    if (typeof this !== 'object' || this === null || !(#searchParams in this))
      throw new ERR_INVALID_THIS('URLSearchParams');

    if (arguments.length < 1) {
      throw new ERR_MISSING_ARGS('name');
    }

    const list = this.#searchParams;
    name = toUSVString(name);

    if (value !== undefined) {
      value = toUSVString(value);
    }

    const { length } = list;
    for (let i = 0; i < length; i += 2) {
      if (list[i] === name) {
        if (value === undefined || list[i + 1] === value) {
          return true;
        }
      }
    }

    return false;
  }

  set(name, value) {
    if (typeof this !== 'object' || this === null || !(#searchParams in this))
      throw new ERR_INVALID_THIS('URLSearchParams');

    if (arguments.length < 2) {
      throw new ERR_MISSING_ARGS('name', 'value');
    }

    const list = this.#searchParams;
    name = toUSVString(name);
    value = toUSVString(value);
    const { length } = list;

    let found = false;
    let write = 0;
    for (let i = 0; i < length; i += 2) {
      const cur = list[i];
      let keep = true;
      if (cur === name) {
        if (!found) {
          list[write] = cur;
          list[write + 1] = value;
          found = true;
        } else {
          keep = false;
        }
      } else if (write !== i) {
        list[write] = cur;
        list[write + 1] = list[i + 1];
      }
      if (keep)
        write += 2;
    }

    if (found && write !== length) {
      list.length = write;
    }

    if (!found) {
      ArrayPrototypePush(list, name, value);
    }

    this.#markUpdated();
  }

  sort() {
    if (typeof this !== 'object' || this === null || !(#searchParams in this))
      throw new ERR_INVALID_THIS('URLSearchParams');

    const a = this.#searchParams;
    const len = a.length;

    if (len <= 2) {
    } else if (len < 100) {
      for (let i = 2; i < len; i += 2) {
        const curKey = a[i];
        const curVal = a[i + 1];
        let j;
        for (j = i - 2; j >= 0; j -= 2) {
          if (a[j] > curKey) {
            a[j + 2] = a[j];
            a[j + 3] = a[j + 1];
          } else {
            break;
          }
        }
        a[j + 2] = curKey;
        a[j + 3] = curVal;
      }
    } else {
      const lBuffer = new Array(len);
      const rBuffer = new Array(len);
      for (let step = 2; step < len; step *= 2) {
        for (let start = 0; start < len - 2; start += 2 * step) {
          const mid = start + step;
          let end = mid + step;
          end = end < len ? end : len;
          if (mid > end)
            continue;
          merge(a, start, mid, end, lBuffer, rBuffer);
        }
      }
    }

    this.#markUpdated();
  }

  // https://heycam.github.io/webidl/#es-iterators
  entries() {
    if (typeof this !== 'object' || this === null || !(#searchParams in this))
      throw new ERR_INVALID_THIS('URLSearchParams');

    return new URLSearchParamsIterator(this, 'key+value');
  }

  forEach(callback, thisArg = undefined) {
    if (typeof this !== 'object' || this === null || !(#searchParams in this))
      throw new ERR_INVALID_THIS('URLSearchParams');

    validateFunction(callback, 'callback');

    let list = this.#searchParams;

    let i = 0;
    while (i < list.length) {
      const key = list[i];
      const value = list[i + 1];
      callback.call(thisArg, value, key, this);
      list = this.#searchParams;
      i += 2;
    }
  }

  // https://heycam.github.io/webidl/#es-iterable
  keys() {
    if (typeof this !== 'object' || this === null || !(#searchParams in this))
      throw new ERR_INVALID_THIS('URLSearchParams');

    return new URLSearchParamsIterator(this, 'key');
  }

  values() {
    if (typeof this !== 'object' || this === null || !(#searchParams in this))
      throw new ERR_INVALID_THIS('URLSearchParams');

    return new URLSearchParamsIterator(this, 'value');
  }

  // https://heycam.github.io/webidl/#es-stringifier
  // https://url.spec.whatwg.org/#urlsearchparams-stringification-behavior
  toString() {
    if (typeof this !== 'object' || this === null || !(#searchParams in this))
      throw new ERR_INVALID_THIS('URLSearchParams');

    if (this.#serialized !== undefined) {
      return this.#serialized;
    }
    const serialized = serializeParams(this.#searchParams);
    this.#serialized = serialized;
    return serialized;
  }
}

ObjectDefineProperties(URLSearchParams.prototype, {
  append: kEnumerableProperty,
  delete: kEnumerableProperty,
  get: kEnumerableProperty,
  getAll: kEnumerableProperty,
  has: kEnumerableProperty,
  set: kEnumerableProperty,
  size: kEnumerableProperty,
  sort: kEnumerableProperty,
  entries: kEnumerableProperty,
  forEach: kEnumerableProperty,
  keys: kEnumerableProperty,
  values: kEnumerableProperty,
  toString: kEnumerableProperty,
  [SymbolToStringTag]: { __proto__: null, configurable: true, value: 'URLSearchParams' },

  // https://heycam.github.io/webidl/#es-iterable-entries
  [SymbolIterator]: {
    __proto__: null,
    configurable: true,
    writable: true,
    value: URLSearchParams.prototype.entries,
  },
});

function toUSVString(value) {
  return typeof value === 'string' ?
    StringPrototypeToWellFormed(value) :
    StringPrototypeToWellFormed(`${value}`);
}

function unescapeFormComponent(s) {
  try {
    return decodeURIComponent(s);
  } catch {
    return querystring.unescapeBuffer(s).toString();
  }
}

function hasPercentHex(s) {
  const end = s.length - 2;
  for (let i = 0; i < end; i++) {
    if (StringPrototypeCharCodeAt(s, i) === 37 && // '%'
        isHexTable[StringPrototypeCharCodeAt(s, i + 1)] === 1 &&
        isHexTable[StringPrototypeCharCodeAt(s, i + 2)] === 1) {
      return true;
    }
  }
  return false;
}

function decodeFormComponent(qs, start, end) {
  if (start >= end) {
    return '';
  }
  const s = qs.slice(start, end);
  const plus = s.indexOf('+');
  const pct = s.indexOf('%');
  if (plus === -1 && pct === -1) {
    return s;
  }
  const replaced = plus === -1 ? s : s.replaceAll('+', ' ');
  if (pct === -1 || !hasPercentHex(replaced)) {
    return replaced;
  }
  return unescapeFormComponent(replaced);
}

// Ref: https://url.spec.whatwg.org/#concept-urlencoded-parser
function parseParams(qs) {
  const len = qs.length;
  let i = qs[0] === '?' ? 1 : 0;
  if (i >= len) {
    return [];
  }

  const out = [];
  const encoded = qs.indexOf('+', i) !== -1 || qs.indexOf('%', i) !== -1;
  while (i < len) {
    let amp = qs.indexOf('&', i);
    if (amp === -1) {
      amp = len;
    }
    if (amp !== i) {
      const eq = qs.indexOf('=', i);
      if (eq === -1 || eq > amp) {
        out.push(encoded ? decodeFormComponent(qs, i, amp) : qs.slice(i, amp), '');
      } else {
        out.push(
          encoded ? decodeFormComponent(qs, i, eq) : qs.slice(i, eq),
          encoded ? decodeFormComponent(qs, eq + 1, amp) : qs.slice(eq + 1, amp),
        );
      }
    }
    i = amp + 1;
  }
  return out;
}

// Ref: https://url.spec.whatwg.org/#concept-urlencoded-byte-serializer
const noEscape = new Int8Array([
/*
  0, 1, 2, 3, 4, 5, 6, 7, 8, 9, A, B, C, D, E, F
*/
  0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // 0x00 - 0x0F
  0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // 0x10 - 0x1F
  0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 1, 1, 0, // 0x20 - 0x2F
  1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, // 0x30 - 0x3F
  0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, // 0x40 - 0x4F
  1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 1, // 0x50 - 0x5F
  0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, // 0x60 - 0x6F
  1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0,  // 0x70 - 0x7F
]);

const paramHexTable = hexTable.slice();
paramHexTable[0x20] = '+';

// Ref: https://url.spec.whatwg.org/#concept-urlencoded-serializer
function serializeParams(array) {
  const len = array.length;
  if (len === 0)
    return '';

  if (len === 2) {
    return encodeStr(array[0], noEscape, paramHexTable) + '=' +
           encodeStr(array[1], noEscape, paramHexTable);
  }

  const pairs = new Array(len / 2);
  for (let i = 0, j = 0; i < len; i += 2, ++j) {
    pairs[j] = encodeStr(array[i], noEscape, paramHexTable) + '=' +
               encodeStr(array[i + 1], noEscape, paramHexTable);
  }
  return ArrayPrototypeJoin(pairs, '&');
}

function merge(out, start, mid, end, lBuffer, rBuffer) {
  const sizeLeft = mid - start;
  const sizeRight = end - mid;
  let l, r, o;

  for (l = 0; l < sizeLeft; l++)
    lBuffer[l] = out[start + l];
  for (r = 0; r < sizeRight; r++)
    rBuffer[r] = out[mid + r];

  l = 0;
  r = 0;
  o = start;
  while (l < sizeLeft && r < sizeRight) {
    if (lBuffer[l] <= rBuffer[r]) {
      out[o++] = lBuffer[l++];
      out[o++] = lBuffer[l++];
    } else {
      out[o++] = rBuffer[r++];
      out[o++] = rBuffer[r++];
    }
  }
  while (l < sizeLeft)
    out[o++] = lBuffer[l++];
  while (r < sizeRight)
    out[o++] = rBuffer[r++];
}


module.exports = { URLSearchParams };
