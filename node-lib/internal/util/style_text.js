'use strict';

const {
  ArrayIsArray,
  NumberParseInt,
  ObjectGetOwnPropertyNames,
  RegExpPrototypeExec,
  SafeMap,
  StringPrototypeSlice,
} = primordials;

const {
  codes: {
    ERR_INVALID_ARG_TYPE,
    ERR_INVALID_ARG_VALUE,
    ERR_OUT_OF_RANGE,
  },
} = require('internal/errors');
const { inspect } = require('internal/util/inspect');
const {
  validateBoolean,
  validateObject,
  validateOneOf,
  validateString,
} = require('internal/validators');

let utilColors;
function lazyUtilColors() {
  utilColors ??= require('internal/util/colors');
  return utilColors;
}

function isStreamLike(obj) {
  if (!obj) return false;
  if (obj === process.stdout || obj === process.stderr) return true;
  return typeof obj.pipeThrough === 'function' && typeof obj.getReader === 'function' &&
      typeof obj.cancel === 'function' ||
    typeof obj.getWriter === 'function' && typeof obj.abort === 'function' ||
    !!(obj._readableState || obj._writableState ||
      (typeof obj.write === 'function' && typeof obj.on === 'function') ||
      (typeof obj.pipe === 'function' && typeof obj.on === 'function'));
}


// Pre-computed ANSI escape code constants
const kEscape = '\u001b[';
const kEscapeEnd = 'm';

// Codes for dim (2) and bold (1) - these share close code 22
const kDimCode = 2;
const kBoldCode = 1;

// Close sequence for 24-bit foreground colors (reset to default foreground)
const kHexCloseSeq = kEscape + '39' + kEscapeEnd;

let styleCache;

const kHexStyleCacheMax = 256;

let hexStyleCache;

function getHexStyleCache() {
  hexStyleCache ??= new SafeMap();
  return hexStyleCache;
}

function codesToStyle(codes) {
  const openNum = codes[0];
  return {
    __proto__: null,
    openSeq: kEscape + openNum + kEscapeEnd,
    closeSeq: kEscape + codes[1] + kEscapeEnd,
    keepClose: openNum === kDimCode || openNum === kBoldCode,
  };
}

function getStyleCache() {
  if (styleCache === undefined) {
    styleCache = { __proto__: null };
    const colors = inspect.colors;
    for (const key of ObjectGetOwnPropertyNames(colors)) {
      const codes = colors[key];
      if (codes) {
        styleCache[key] = codesToStyle(codes);
      }
    }
  }
  return styleCache;
}

/**
 * Returns the cached ANSI escape sequences for a hex color.
 * Computes and caches on first use to avoid repeated Buffer allocations.
 * @param {string} hex A valid hex color string (#RGB or #RRGGBB)
 * @returns {{openSeq: string, closeSeq: string}}
 */
function getHexStyle(hex) {
  const cache = getHexStyleCache();
  const cached = cache.get(hex);
  if (cached !== undefined) return cached;
  const { 0: r, 1: g, 2: b } = hexToRgb(hex);
  const style = {
    __proto__: null,
    openSeq: kEscape + rgbToAnsi24Bit(r, g, b) + kEscapeEnd,
    closeSeq: kHexCloseSeq,
  };
  if (cache.size >= kHexStyleCacheMax)
    cache.delete(cache.keys().next().value);
  cache.set(hex, style);
  return style;
}

function replaceCloseCode(str, closeSeq, openSeq, keepClose) {
  const closeLen = closeSeq.length;
  let index = str.indexOf(closeSeq);
  if (index === -1) return str;

  let result = '';
  let lastIndex = 0;
  const replacement = keepClose ? closeSeq + openSeq : openSeq;

  do {
    const afterClose = index + closeLen;
    if (afterClose < str.length) {
      result += str.slice(lastIndex, index) + replacement;
      lastIndex = afterClose;
    } else {
      break;
    }
    index = str.indexOf(closeSeq, lastIndex);
  } while (index !== -1);

  return result + str.slice(lastIndex);
}

// Matches #RGB or #RRGGBB
const hexColorRegExp = /^#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{6})$/;

/**
 * Parses a hex color string into RGB components.
 * Supports both 3-digit (#RGB) and 6-digit (#RRGGBB) formats.
 * @param {string} hex A valid hex color string
 * @returns {Buffer} The RGB components
 */
function hexToRgb(hex) {
  // Normalize to 6 digits
  let hexStr;
  if (hex.length === 4) {
    // Expand #RGB to #RRGGBB
    hexStr = hex[1] + hex[1] + hex[2] + hex[2] + hex[3] + hex[3];
  } else if (hex.length === 7) {
    hexStr = StringPrototypeSlice(hex, 1);
  } else {
    throw new ERR_OUT_OF_RANGE('hex', '#RGB or #RRGGBB', hex);
  }

  // TODO(araujogui): use Uint8Array.fromHex
  return [
    NumberParseInt(StringPrototypeSlice(hexStr, 0, 2), 16),
    NumberParseInt(StringPrototypeSlice(hexStr, 2, 4), 16),
    NumberParseInt(StringPrototypeSlice(hexStr, 4, 6), 16),
  ];
}

/**
 * Generates the ANSI TrueColor (24-bit) escape sequence for a foreground color.
 * @param {number} r Red component (0-255)
 * @param {number} g Green component (0-255)
 * @param {number} b Blue component (0-255)
 * @returns {string} The ANSI escape sequence
 */
function rgbToAnsi24Bit(r, g, b) {
  return `38;2;${r};${g};${b}`;
}

/**
 * @param {string | string[]} format
 * @param {string} text
 * @param {object} [options]
 * @param {boolean} [options.validateStream] - Whether to validate the stream.
 * @param {Stream} [options.stream] - The stream used for validation.
 * @returns {string}
 */
function styleText(format, text, options) {
  const validateStream = options?.validateStream ?? true;

  // Fast path: single format string with validateStream=false
  if (!validateStream && typeof format === 'string' && typeof text === 'string') {
    const cache = getStyleCache();
    if (format === 'none') return text;
    const style = cache[format];
    if (style !== undefined) {
      const processed = replaceCloseCode(text, style.closeSeq, style.openSeq, style.keepClose);
      return style.openSeq + processed + style.closeSeq;
    }

    if (format[0] === '#') {
      let hexStyle = getHexStyleCache().get(format);
      if (hexStyle === undefined && RegExpPrototypeExec(hexColorRegExp, format) !== null) {
        hexStyle = getHexStyle(format);
      }
      if (hexStyle !== undefined) {
        const processed = replaceCloseCode(text, hexStyle.closeSeq, hexStyle.openSeq, false);
        return hexStyle.openSeq + processed + hexStyle.closeSeq;
      }
    }
  }

  validateString(text, 'text');
  if (options !== undefined) {
    validateObject(options, 'options');
  }
  validateBoolean(validateStream, 'options.validateStream');

  let skipColorize;
  if (validateStream) {
    const stream = options?.stream ?? process.stdout;
    if (!isStreamLike(stream)) {
      throw new ERR_INVALID_ARG_TYPE('stream', ['ReadableStream', 'WritableStream', 'Stream'], stream);
    }
    skipColorize = !lazyUtilColors().shouldColorize(stream);
  }

  const formatArray = ArrayIsArray(format) ? format : [format];
  const colors = inspect.colors;

  let openCodes = '';
  let closeCodes = '';
  let processedText = text;

  for (const key of formatArray) {
    if (key === 'none') continue;

    if (typeof key === 'string' && key[0] === '#') {
      if (RegExpPrototypeExec(hexColorRegExp, key) === null) {
        throw new ERR_INVALID_ARG_VALUE('format', key,
                                        'must be a valid hex color (#RGB or #RRGGBB)');
      }
      if (skipColorize) continue;
      const { 0: r, 1: g, 2: b } = hexToRgb(key);
      const hexOpenSeq = kEscape + rgbToAnsi24Bit(r, g, b) + kEscapeEnd;
      openCodes += hexOpenSeq;
      closeCodes = kHexCloseSeq + closeCodes;
      processedText = replaceCloseCode(processedText, kHexCloseSeq, hexOpenSeq, false);
      continue;
    }

    const codes = colors[key];
    if (!codes) {
      validateOneOf(key, 'format', ObjectGetOwnPropertyNames(inspect.colors));
    }
    const { openSeq, closeSeq, keepClose } = codesToStyle(codes);
    openCodes += openSeq;
    closeCodes = closeSeq + closeCodes;
    processedText = replaceCloseCode(processedText, closeSeq, openSeq, keepClose);
  }

  if (skipColorize) return text;

  return `${openCodes}${processedText}${closeCodes}`;
}

module.exports = { styleText };
