'use strict';

const {
  ArrayPrototypeJoin,
  ArrayPrototypeMap,
  MathTrunc,
  NumberIsFinite,
  NumberIsNaN,
  NumberPrototypeToString,
  ObjectIs,
  RegExp,
  RegExpPrototypeExec,
  RegExpPrototypeSymbolReplace,
  RegExpPrototypeSymbolSplit,
  SafeStringIterator,
  String,
  StringPrototypeCharCodeAt,
  StringPrototypeCodePointAt,
  StringPrototypeIncludes,
  StringPrototypeIndexOf,
  StringPrototypeNormalize,
  StringPrototypeRepeat,
  StringPrototypeSlice,
  SymbolPrototypeToString,
} = primordials;

const { validateString } = require('internal/validators');
const assert = require('internal/assert');

let inspect;

function init(deps) {
  inspect = deps.inspect;
}

const strEscapeSequencesReplacerSingle = /[\x00-\x1f\x5c\x7f-\x9f]|[\ud800-\udbff](?![\udc00-\udfff])|(?<![\ud800-\udbff])[\udc00-\udfff]/g;
const strEscapeSequencesRegExpSingle = /[\x00-\x1f\x5c\x7f-\x9f]|[\ud800-\udbff](?![\udc00-\udfff])|(?<![\ud800-\udbff])[\udc00-\udfff]/;
const strEscapeSequencesReplacer = /[\x00-\x1f\x27\x5c\x7f-\x9f]|[\ud800-\udbff](?![\udc00-\udfff])|(?<![\ud800-\udbff])[\udc00-\udfff]/g;

const kMinLineLength = 16;

// Escaped control characters (plus the single quote and the backslash). Use
// empty strings to fill up unused entries.
const meta = [
  '\\x00', '\\x01', '\\x02', '\\x03', '\\x04', '\\x05', '\\x06', '\\x07', // x07
  '\\b', '\\t', '\\n', '\\x0B', '\\f', '\\r', '\\x0E', '\\x0F',           // x0F
  '\\x10', '\\x11', '\\x12', '\\x13', '\\x14', '\\x15', '\\x16', '\\x17', // x17
  '\\x18', '\\x19', '\\x1A', '\\x1B', '\\x1C', '\\x1D', '\\x1E', '\\x1F', // x1F
  '', '', '', '', '', '', '', "\\'", '', '', '', '', '', '', '', '',      // x2F
  '', '', '', '', '', '', '', '', '', '', '', '', '', '', '', '',         // x3F
  '', '', '', '', '', '', '', '', '', '', '', '', '', '', '', '',         // x4F
  '', '', '', '', '', '', '', '', '', '', '', '', '\\\\', '', '', '',     // x5F
  '', '', '', '', '', '', '', '', '', '', '', '', '', '', '', '',         // x6F
  '', '', '', '', '', '', '', '', '', '', '', '', '', '', '', '\\x7F',    // x7F
  '\\x80', '\\x81', '\\x82', '\\x83', '\\x84', '\\x85', '\\x86', '\\x87', // x87
  '\\x88', '\\x89', '\\x8A', '\\x8B', '\\x8C', '\\x8D', '\\x8E', '\\x8F', // x8F
  '\\x90', '\\x91', '\\x92', '\\x93', '\\x94', '\\x95', '\\x96', '\\x97', // x97
  '\\x98', '\\x99', '\\x9A', '\\x9B', '\\x9C', '\\x9D', '\\x9E', '\\x9F', // x9F
];

// Regex used for ansi escape code splitting
// Ref: https://github.com/chalk/ansi-regex/blob/72bc570aaf25fca25541b49c6a8564f3ec63e835/index.js
// License: MIT by Sindre Sorhus <sindresorhus@gmail.com>
// Matches all ansi escape code sequences in a string
const ansi = new RegExp(
  '(?:\\u001B\\][\\s\\S]*?(?:\\u0007|\\u001B\\u005C|\\u009C))' +
  '|[\\u001B\\u009B][[\\]()#;?]*' +
  '(?:\\d{1,4}(?:[;:]\\d{0,4})*)?' +
  '[\\dA-PR-TZcf-nq-uy=><~]', 'g',
);

/* eslint-disable no-control-regex */
const strEscapeSequencesRegExp = /[\x00-\x1f\x27\x5c\x7f-\x9f]|[\ud800-\udbff](?![\udc00-\udfff])|(?<![\ud800-\udbff])[\udc00-\udfff]/;

function addQuotes(str, quotes) {
  if (quotes === -1) {
    return `"${str}"`;
  }
  if (quotes === -2) {
    return `\`${str}\``;
  }
  return `'${str}'`;
}

function escapeFn(str) {
  const charCode = StringPrototypeCharCodeAt(str);
  return meta.length > charCode ? meta[charCode] : `\\u${NumberPrototypeToString(charCode, 16)}`;
}

// Escape control characters, single quotes and the backslash.
// This is similar to JSON stringify escaping.
function strEscape(str) {
  let escapeTest = strEscapeSequencesRegExp;
  let escapeReplace = strEscapeSequencesReplacer;
  let singleQuote = 39;

  // Check for double quotes. If not present, do not escape single quotes and
  // instead wrap the text in double quotes. If double quotes exist, check for
  // backticks. If they do not exist, use those as fallback instead of the
  // double quotes.
  if (StringPrototypeIncludes(str, "'")) {
    // This invalidates the charCode and therefore can not be matched for
    // anymore.
    if (!StringPrototypeIncludes(str, '"')) {
      singleQuote = -1;
    } else if (!StringPrototypeIncludes(str, '`') &&
               !StringPrototypeIncludes(str, '${')) {
      singleQuote = -2;
    }
    if (singleQuote !== 39) {
      escapeTest = strEscapeSequencesRegExpSingle;
      escapeReplace = strEscapeSequencesReplacerSingle;
    }
  }

  // Some magic numbers that worked out fine while benchmarking with v8 6.0
  if (str.length < 5000 && RegExpPrototypeExec(escapeTest, str) === null)
    return addQuotes(str, singleQuote);
  if (str.length > 100) {
    str = RegExpPrototypeSymbolReplace(escapeReplace, str, escapeFn);
    return addQuotes(str, singleQuote);
  }

  let result = '';
  let last = 0;
  for (let i = 0; i < str.length; i++) {
    const point = StringPrototypeCharCodeAt(str, i);
    if (point === singleQuote ||
        point === 92 ||
        point < 32 ||
        (point > 126 && point < 160)) {
      if (last === i) {
        result += meta[point];
      } else {
        result += `${StringPrototypeSlice(str, last, i)}${meta[point]}`;
      }
      last = i + 1;
    } else if (point >= 0xd800 && point <= 0xdfff) {
      if (point <= 0xdbff && i + 1 < str.length) {
        const point = StringPrototypeCharCodeAt(str, i + 1);
        if (point >= 0xdc00 && point <= 0xdfff) {
          i++;
          continue;
        }
      }
      result += `${StringPrototypeSlice(str, last, i)}\\u${NumberPrototypeToString(point, 16)}`;
      last = i + 1;
    }
  }

  if (last !== str.length) {
    result += StringPrototypeSlice(str, last);
  }
  return addQuotes(result, singleQuote);
}

function stylizeWithColor(str, styleType) {
  const style = inspect.styles[styleType];
  if (style !== undefined) {
    const color = inspect.colors[style];
    if (color !== undefined) {
      return `\u001b[${color[0]}m${str}\u001b[${color[1]}m`;
    }
    if (typeof style === 'function') {
      return style(str);
    }
  }
  return str;
}

function stylizeNoColor(str) {
  return str;
}

let getStringWidth;

function isZeroWidthCodePoint(code) {
  return code <= 0x1F || // C0 control codes
    (code >= 0x7F && code <= 0x9F) || // C1 control codes
    (code >= 0x300 && code <= 0x36F) || // Combining Diacritical Marks
    (code >= 0x200B && code <= 0x200F) || // Modifying Invisible Characters
    // Combining Diacritical Marks for Symbols
    (code >= 0x20D0 && code <= 0x20FF) ||
    (code >= 0xFE00 && code <= 0xFE0F) || // Variation Selectors
    (code >= 0xFE20 && code <= 0xFE2F) || // Combining Half Marks
    (code >= 0xE0100 && code <= 0xE01EF); // Variation Selectors
}
  /**
   * @param {string} str
   * @param {boolean} [removeControlChars]
   * @returns {number} number of columns required to display the given string.
   */
  getStringWidth = function getStringWidth(str, removeControlChars = true) {
    let width = 0;

    if (removeControlChars)
      str = stripVTControlCharacters(str);
    str = StringPrototypeNormalize(str, 'NFC');
    for (const char of new SafeStringIterator(str)) {
      const code = StringPrototypeCodePointAt(char, 0);
      if (isFullWidthCodePoint(code)) {
        width += 2;
      } else if (!isZeroWidthCodePoint(code)) {
        width++;
      }
    }

    return width;
  };

  /**
   * Returns true if the character represented by a given
   * Unicode code point is full-width. Otherwise returns false.
   * @param {string} code
   * @returns {boolean}
   */
  const isFullWidthCodePoint = (code) => {
    // Code points are partially derived from:
    // https://www.unicode.org/Public/UNIDATA/EastAsianWidth.txt
    return code >= 0x1100 && (
      code <= 0x115f ||  // Hangul Jamo
      code === 0x2329 || // LEFT-POINTING ANGLE BRACKET
      code === 0x232a || // RIGHT-POINTING ANGLE BRACKET
      // CJK Radicals Supplement .. Enclosed CJK Letters and Months
      (code >= 0x2e80 && code <= 0x3247 && code !== 0x303f) ||
      // Enclosed CJK Letters and Months .. CJK Unified Ideographs Extension A
      (code >= 0x3250 && code <= 0x4dbf) ||
      // CJK Unified Ideographs .. Yi Radicals
      (code >= 0x4e00 && code <= 0xa4c6) ||
      // Hangul Jamo Extended-A
      (code >= 0xa960 && code <= 0xa97c) ||
      // Hangul Syllables
      (code >= 0xac00 && code <= 0xd7a3) ||
      // CJK Compatibility Ideographs
      (code >= 0xf900 && code <= 0xfaff) ||
      // Vertical Forms
      (code >= 0xfe10 && code <= 0xfe19) ||
      // CJK Compatibility Forms .. Small Form Variants
      (code >= 0xfe30 && code <= 0xfe6b) ||
      // Halfwidth and Fullwidth Forms
      (code >= 0xff01 && code <= 0xff60) ||
      (code >= 0xffe0 && code <= 0xffe6) ||
      // Kana Supplement
      (code >= 0x1b000 && code <= 0x1b001) ||
      // Enclosed Ideographic Supplement
      (code >= 0x1f200 && code <= 0x1f251) ||
      // Miscellaneous Symbols and Pictographs 0x1f300 - 0x1f5ff
      // Emoticons 0x1f600 - 0x1f64f
      (code >= 0x1f300 && code <= 0x1f64f) ||
      // CJK Unified Ideographs Extension B .. Tertiary Ideographic Plane
      (code >= 0x20000 && code <= 0x3fffd)
    );
  };



/**
 * Remove all VT control characters. Use to estimate displayed string width.
 * @param {string} str
 * @returns {string}
 */
function stripVTControlCharacters(str) {
  validateString(str, 'str');

  // Short-circuit: all ANSI escape sequences start with either
  // ESC (\u001B, 7-bit) or CSI (\u009B, 8-bit) introducer.
  // If neither is present, the string has no VT control characters.
  if (StringPrototypeIndexOf(str, '\u001B') === -1 &&
      StringPrototypeIndexOf(str, '\u009B') === -1)
    return str;

  return RegExpPrototypeSymbolReplace(ansi, str, '');
}

function addNumericSeparator(integerString) {
  let result = '';
  let i = integerString.length;
  assert(i !== 0);
  const start = integerString[0] === '-' ? 1 : 0;
  for (; i >= start + 4; i -= 3) {
    result = `_${StringPrototypeSlice(integerString, i - 3, i)}${result}`;
  }
  return i === integerString.length ?
    integerString :
    `${StringPrototypeSlice(integerString, 0, i)}${result}`;
}

function addNumericSeparatorEnd(integerString) {
  let result = '';
  let i = 0;
  for (; i < integerString.length - 3; i += 3) {
    result += `${StringPrototypeSlice(integerString, i, i + 3)}_`;
  }
  return i === 0 ?
    integerString :
    `${result}${StringPrototypeSlice(integerString, i)}`;
}

function formatNumber(fn, number, numericSeparator) {
  // Format -0 as '-0'. Checking `number === -0` won't distinguish 0 from -0.
  // String(-0) === '0', so this must be checked before any String() conversion.
  if (ObjectIs(number, -0)) {
    return fn('-0', 'number');
  }
  if (!numericSeparator) {
    return fn(`${number}`, 'number');
  }

  const numberString = String(number);
  const integer = MathTrunc(number);

  if (integer === number) {
    if (!NumberIsFinite(number) || StringPrototypeIncludes(numberString, 'e')) {
      return fn(numberString, 'number');
    }
    return fn(addNumericSeparator(numberString), 'number');
  }
  if (NumberIsNaN(number) || StringPrototypeIncludes(numberString, 'e')) {
    return fn(numberString, 'number');
  }

  const decimalIndex = StringPrototypeIndexOf(numberString, '.');
  const integerPart = StringPrototypeSlice(numberString, 0, decimalIndex);
  const fractionalPart = StringPrototypeSlice(numberString, decimalIndex + 1);

  return fn(`${
    addNumericSeparator(integerPart)
  }.${
    addNumericSeparatorEnd(fractionalPart)
  }`, 'number');
}

function formatBigInt(fn, bigint, numericSeparator) {
  const string = String(bigint);
  if (!numericSeparator) {
    return fn(`${string}n`, 'bigint');
  }
  return fn(`${addNumericSeparator(string)}n`, 'bigint');
}

function formatPrimitive(fn, value, ctx) {
  if (typeof value === 'string') {
    let trailer = '';
    if (value.length > ctx.maxStringLength) {
      const remaining = value.length - ctx.maxStringLength;
      value = StringPrototypeSlice(value, 0, ctx.maxStringLength);
      trailer = `... ${remaining} more character${remaining > 1 ? 's' : ''}`;
    }
    if (ctx.compact !== true &&
        // We do not support handling unicode characters width with
        // the readline getStringWidth function as there are
        // performance implications.
        value.length > kMinLineLength &&
        value.length > ctx.breakLength - ctx.indentationLvl - 4) {
      return ArrayPrototypeJoin(
        ArrayPrototypeMap(
          RegExpPrototypeSymbolSplit(/(?<=\n)/, value),
          (line) => fn(strEscape(line), 'string'),
        ),
        ` +\n${StringPrototypeRepeat(' ', ctx.indentationLvl + 2)}`,
      ) + trailer;
    }
    return fn(strEscape(value), 'string') + trailer;
  }
  if (typeof value === 'number')
    return formatNumber(fn, value, ctx.numericSeparator);
  if (typeof value === 'bigint')
    return formatBigInt(fn, value, ctx.numericSeparator);
  if (typeof value === 'boolean')
    return fn(`${value}`, 'boolean');
  if (typeof value === 'undefined')
    return fn('undefined', 'undefined');
  // es6 symbol primitive
  return fn(SymbolPrototypeToString(value), 'symbol');
}

module.exports = {
  init,
  isZeroWidthCodePoint,
  stripVTControlCharacters,
  getStringWidth,
  formatNumber,
  formatBigInt,
  stylizeNoColor,
  formatPrimitive,
  escapeFn,
  strEscape,
  stylizeWithColor,
};
