'use strict';

// The `SourceMap` class of `module`, after Node's
// lib/internal/source_map/source_map.js (itself the DevTools implementation):
// a source map v3 payload, indexed sections, and lookups by generated position.

const {
  ArrayIsArray,
  ArrayPrototypeMap,
  ArrayPrototypePush,
  ArrayPrototypeSlice,
  StringPrototypeCharAt,
} = primordials;

const { validateObject } = require('internal/validators');

const VLQ_BASE_SHIFT = 5;
const VLQ_BASE_MASK = (1 << 5) - 1;
const VLQ_CONTINUATION_MASK = 1 << 5;

const base64Map = { __proto__: null };
const base64Chars = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';
for (let i = 0; i < base64Chars.length; ++i) base64Map[base64Chars[i]] = i;

class StringCharIterator {
  constructor(string) {
    this.string = string;
    this.position = 0;
  }
  next() {
    return StringPrototypeCharAt(this.string, this.position++);
  }
  peek() {
    return StringPrototypeCharAt(this.string, this.position);
  }
  hasNext() {
    return this.position < this.string.length;
  }
}

function decodeVLQ(iterator) {
  let result = 0;
  let shift = 0;
  let digit;
  do {
    digit = base64Map[iterator.next()];
    result += (digit & VLQ_BASE_MASK) << shift;
    shift += VLQ_BASE_SHIFT;
  } while (digit & VLQ_CONTINUATION_MASK);
  const negative = result & 1;
  result >>= 1;
  return negative ? -result : result;
}

function isSeparator(char) {
  return char === ',' || char === ';';
}

function cloneSourceMapV3(payload) {
  validateObject(payload, 'payload');
  payload = { ...payload };
  for (const key in payload) {
    if (ArrayIsArray(payload[key])) payload[key] = ArrayPrototypeSlice(payload[key]);
  }
  return payload;
}

class SourceMap {
  #payload;
  #mappings = [];
  #lineLengths;

  constructor(payload, { lineLengths } = { __proto__: null }) {
    this.#payload = cloneSourceMapV3(payload);
    this.#parseMappingPayload();
    if (ArrayIsArray(lineLengths) && lineLengths.length) this.#lineLengths = lineLengths;
  }

  get payload() {
    return cloneSourceMapV3(this.#payload);
  }

  get lineLengths() {
    return this.#lineLengths;
  }

  #parseMappingPayload() {
    if (this.#payload.sections) {
      this.#parseSections(this.#payload.sections);
    } else {
      this.#parseMap(this.#payload, 0, 0);
    }
  }

  #parseSections(sections) {
    for (let i = 0; i < sections.length; ++i) {
      const section = sections[i];
      this.#parseMap(section.map, section.offset.line, section.offset.column);
    }
  }

  findEntry(lineOffset, columnOffset) {
    let first = 0;
    let count = this.#mappings.length;
    while (count > 1) {
      const step = count >> 1;
      const middle = first + step;
      const mapping = this.#mappings[middle];
      if (lineOffset < mapping[0] ||
          (lineOffset === mapping[0] && columnOffset < mapping[1])) {
        count = step;
      } else {
        first = middle;
        count -= step;
      }
    }
    const entry = this.#mappings[first];
    if (!entry || (!first && (lineOffset < entry[0] ||
        (lineOffset === entry[0] && columnOffset < entry[1])))) {
      return {};
    }
    return {
      generatedLine: entry[0],
      generatedColumn: entry[1],
      originalSource: entry[2],
      originalLine: entry[3],
      originalColumn: entry[4],
      name: entry[5],
    };
  }

  findOrigin(lineNumber, columnNumber) {
    const range = this.findEntry(lineNumber - 1, columnNumber - 1);
    if (!range.originalSource) return {};
    return {
      name: range.name,
      fileName: range.originalSource,
      lineNumber: range.originalLine + (lineNumber - 1 - range.generatedLine) + 1,
      columnNumber: range.originalColumn + (columnNumber - 1 - range.generatedColumn) + 1,
    };
  }

  #parseMap(map, lineNumber, columnNumber) {
    let sourceIndex = 0;
    let sourceLineNumber = 0;
    let sourceColumnNumber = 0;
    let nameIndex = 0;
    const sources = [];
    for (let i = 0; i < map.sources.length; ++i) {
      ArrayPrototypePush(sources, map.sources[i]);
    }
    const iterator = new StringCharIterator(map.mappings);
    let sourceURL = sources[sourceIndex];
    for (;;) {
      if (iterator.peek() === ',') {
        iterator.next();
      } else {
        while (iterator.peek() === ';') {
          lineNumber += 1;
          columnNumber = 0;
          iterator.next();
        }
        if (!iterator.hasNext()) break;
      }
      columnNumber += decodeVLQ(iterator);
      if (isSeparator(iterator.peek())) {
        ArrayPrototypePush(this.#mappings, [lineNumber, columnNumber]);
        continue;
      }
      const sourceIndexDelta = decodeVLQ(iterator);
      if (sourceIndexDelta) {
        sourceIndex += sourceIndexDelta;
        sourceURL = sources[sourceIndex];
      }
      sourceLineNumber += decodeVLQ(iterator);
      sourceColumnNumber += decodeVLQ(iterator);
      let name;
      if (!isSeparator(iterator.peek())) {
        nameIndex += decodeVLQ(iterator);
        name = map.names?.[nameIndex];
      }
      ArrayPrototypePush(this.#mappings, [
        lineNumber, columnNumber, sourceURL, sourceLineNumber, sourceColumnNumber, name,
      ]);
    }
  }
}

module.exports = { SourceMap };
