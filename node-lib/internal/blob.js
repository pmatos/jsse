'use strict';

// Blob and File over the prelude's Buffer, shaped after Node v26.8.2
// lib/internal/blob.js and lib/internal/file.js. The bytes are copied into one
// Uint8Array when the Blob is constructed. `stream()` and `textStream()` read
// that copy through a byte ReadableStream (#1360); there is no object URL
// registry.

const {
  DateNow,
  MathMax,
  MathMin,
  MathTrunc,
  NumberIsNaN,
  ObjectDefineProperties,
  ObjectAssign,
  PromiseReject,
  PromiseResolve,
  ReflectApply,
  StringPrototypeReplace,
  StringPrototypeToLowerCase,
  SymbolIterator,
  SymbolToStringTag,
  TypedArrayPrototypeSet,
  Uint8Array,
} = primordials;

const { Buffer } = require('internal/host/buffer');
const { inspect } = require('internal/util/inspect');
const { isAnyArrayBuffer, isArrayBufferView } = require('internal/util/types');
const { customInspectSymbol, kEmptyObject } = require('internal/util');
const {
  codes: {
    ERR_INVALID_ARG_TYPE,
    ERR_INVALID_ARG_VALUE,
    ERR_INVALID_THIS,
    ERR_MISSING_ARGS,
  },
} = require('internal/errors');

const eol = process.platform === 'win32' ? '\r\n' : '\n';

function bytesOf(source) {
  if (typeof source === 'string') return Buffer.from(source, 'utf8');
  if (isAnyArrayBuffer(source)) return new Uint8Array(source);
  if (isArrayBufferView(source)) {
    return new Uint8Array(source.buffer, source.byteOffset, source.byteLength);
  }
  return undefined;
}

function toSequence(sources) {
  const method = Object(sources) === sources ? sources[SymbolIterator] : undefined;
  if (typeof method !== 'function') {
    const err = new TypeError('Value cannot be converted to sequence.');
    err.code = 'ERR_INVALID_ARG_TYPE';
    throw err;
  }
  return { [SymbolIterator]() { return ReflectApply(method, sources, []); } };
}

function validateDictionary(value, name) {
  if (value != null && typeof value !== 'object' && typeof value !== 'function') {
    throw new ERR_INVALID_ARG_TYPE(name, 'a dictionary', value);
  }
}

function normalizeType(type) {
  type = `${type}`;
  for (let i = 0; i < type.length; i++) {
    const c = type.charCodeAt(i);
    if (c < 0x20 || c > 0x7e) return '';
  }
  return StringPrototypeToLowerCase(type);
}

function relative(value, size) {
  value = +value;
  if (NumberIsNaN(value)) return 0;
  value = MathTrunc(value);
  return value < 0 ? MathMax(size + value, 0) : MathMin(value, size);
}

let isBlob;

class Blob {
  #bytes;
  #type;

  static {
    isBlob = (value) => Blob.#is(value);
  }

  constructor(sources = [], options = kEmptyObject) {
    const sequence = toSequence(sources);
    validateDictionary(options, 'options');
    let endings = options?.endings;
    endings = endings === undefined ? 'transparent' : `${endings}`;
    if (endings !== 'transparent' && endings !== 'native') {
      throw new ERR_INVALID_ARG_VALUE('options.endings', endings);
    }
    const type = options?.type;
    this.#type = type === undefined ? '' : normalizeType(type);
    this.#bytes = Blob.#collect(sequence, endings);
  }

  static #collect(sequence, endings) {
    const parts = [];
    let total = 0;
    for (const source of sequence) {
      let bytes;
      if (Blob.#is(source)) {
        bytes = source.#bytes;
      } else if (isAnyArrayBuffer(source) || isArrayBufferView(source)) {
        bytes = bytesOf(source);
      } else {
        let text = `${source}`;
        if (endings === 'native') {
          text = StringPrototypeReplace(text, /\r\n|\r|\n/g, eol);
        }
        bytes = bytesOf(text);
      }
      parts.push(bytes);
      total += bytes.length;
    }
    const out = new Uint8Array(total);
    let at = 0;
    for (let i = 0; i < parts.length; i++) {
      TypedArrayPrototypeSet(out, parts[i], at);
      at += parts[i].length;
    }
    return out;
  }

  static #is(blob) {
    return Object(blob) === blob && #bytes in blob;
  }

  static #check(blob) {
    if (!Blob.#is(blob)) throw new ERR_INVALID_THIS('Blob');
  }

  [customInspectSymbol](depth, options) {
    if (depth < 0) return this;
    const opts = ObjectAssign({}, options, {
      depth: options.depth == null ? null : options.depth - 1,
    });
    return `Blob ${inspect({ size: this.size, type: this.type }, opts)}`;
  }

  get type() {
    Blob.#check(this);
    return this.#type;
  }

  get size() {
    Blob.#check(this);
    return this.#bytes.length;
  }

  slice(start = 0, end = this.size, contentType = '') {
    Blob.#check(this);
    const size = this.#bytes.length;
    const from = relative(start, size);
    const to = relative(end, size);
    const blob = new Blob([], { type: contentType });
    blob.#bytes = this.#bytes.slice(from, MathMax(from, to));
    return blob;
  }

  arrayBuffer() {
    if (!Blob.#is(this)) return PromiseReject(new ERR_INVALID_THIS('Blob'));
    return PromiseResolve(this.#bytes.slice().buffer);
  }

  text() {
    if (!Blob.#is(this)) return PromiseReject(new ERR_INVALID_THIS('Blob'));
    const bytes = this.#bytes;
    const bom = bytes.length >= 3 && bytes[0] === 0xef && bytes[1] === 0xbb &&
                bytes[2] === 0xbf ? 3 : 0;
    return PromiseResolve(
      Buffer.from(bytes.buffer, bytes.byteOffset + bom, bytes.length - bom)
        .toString('utf8'));
  }

  bytes() {
    if (!Blob.#is(this)) return PromiseReject(new ERR_INVALID_THIS('Blob'));
    return PromiseResolve(this.#bytes.slice());
  }

  stream() {
    Blob.#check(this);
    const bytes = this.#bytes;
    let done = false;
    const { ReadableStream } = require('internal/webstreams/readablestream');
    return new ReadableStream({
      type: 'bytes',
      pull(controller) {
        if (done || bytes.length === 0) {
          controller.close();
          controller.byobRequest?.respond(0);
          return;
        }
        done = true;
        controller.enqueue(bytes.slice());
      },
    });
  }

  textStream() {
    Blob.#check(this);
    const { TextDecoderStream } = require('internal/webstreams/encoding');
    return this.stream().pipeThrough(new TextDecoderStream());
  }
}

ObjectDefineProperties(Blob.prototype, {
  [SymbolToStringTag]: {
    __proto__: null,
    configurable: true,
    value: 'Blob',
  },
  type: { __proto__: null, enumerable: true },
  size: { __proto__: null, enumerable: true },
  slice: { __proto__: null, enumerable: true },
  arrayBuffer: { __proto__: null, enumerable: true },
  text: { __proto__: null, enumerable: true },
  bytes: { __proto__: null, enumerable: true },
  stream: { __proto__: null, enumerable: true },
  textStream: { __proto__: null, enumerable: true },
});

class File extends Blob {
  #name;
  #lastModified;

  constructor(fileBits, fileName, options = kEmptyObject) {
    if (arguments.length < 2) {
      throw new ERR_MISSING_ARGS('fileBits', 'fileName');
    }
    super(fileBits, options);
    this.#name = `${fileName}`;
    const lastModified = options?.lastModified;
    this.#lastModified = lastModified === undefined ? DateNow() :
      (NumberIsNaN(+lastModified) ? 0 : +lastModified);
  }

  get name() {
    if (Object(this) !== this || !(#name in this)) throw new ERR_INVALID_THIS('File');
    return this.#name;
  }

  get lastModified() {
    if (Object(this) !== this || !(#name in this)) throw new ERR_INVALID_THIS('File');
    return this.#lastModified;
  }

  [customInspectSymbol](depth, options) {
    if (depth < 0) return this;
    const opts = ObjectAssign({}, options, {
      depth: options.depth == null ? null : options.depth - 1,
    });
    return `File ${inspect({
      size: this.size,
      type: this.type,
      name: this.name,
      lastModified: this.lastModified,
    }, opts)}`;
  }
}

ObjectDefineProperties(File.prototype, {
  [SymbolToStringTag]: {
    __proto__: null,
    configurable: true,
    value: 'File',
  },
  name: { __proto__: null, enumerable: true },
  lastModified: { __proto__: null, enumerable: true },
});

function resolveObjectURL(url) {
  return undefined;
}

module.exports = { Blob, File, isBlob, resolveObjectURL };
