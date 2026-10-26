'use strict';

const {
  Array,
  ArrayPrototypePush,
  ArrayPrototypeSort,
  MathFloor,
  MathMax,
  MathMin,
  MathRound,
  MathSqrt,
  ObjectGetOwnPropertyDescriptor,
  ObjectKeys,
  RegExpPrototypeExec,
  RegExpPrototypeSymbolReplace,
  StringPrototypeLastIndexOf,
  StringPrototypePadEnd,
  StringPrototypePadStart,
  StringPrototypeSlice,
  Uint8Array,
  NumberPrototypeToString,
} = primordials;

const { isNativeError } = require('internal/util/types');
const assert = require('internal/assert');
const {
  constants: { kPending, kRejected },
  getPromiseDetails,
  previewEntries,
} = require('internal/binding/util');

let getStringWidth, formatNumber, formatBigInt, formatValue, formatProperty, reduceToSingleString;

function init(deps) {
  getStringWidth = deps.getStringWidth;
  formatNumber = deps.formatNumber;
  formatBigInt = deps.formatBigInt;
  formatValue = deps.formatValue;
  formatProperty = deps.formatProperty;
  reduceToSingleString = deps.reduceToSingleString;
}


const remainingText = (remaining) => `... ${remaining} more item${remaining > 1 ? 's' : ''}`;
const numberRegExp = /^(0|[1-9][0-9]*)$/;
const kMapEntries = 2;
const kIterator = 1;

// Constants to map the iterator state.
const kWeak = 0;
const kArrayExtrasType = 2;
const kArrayType = 1;

const kObjectType = 0;

function groupArrayElements(ctx, output, value) {
  let totalLength = 0;
  let maxLength = 0;
  let i = 0;
  let outputLength = output.length;
  if (ctx.maxArrayLength < output.length) {
    // This makes sure the "... n more items" part is not taken into account.
    outputLength--;
  }
  const separatorSpace = 2; // Add 1 for the space and 1 for the separator.
  const dataLen = new Array(outputLength);
  // Calculate the total length of all output entries and the individual max
  // entries length of all output entries. We have to remove colors first,
  // otherwise the length would not be calculated properly.
  for (; i < outputLength; i++) {
    const len = getStringWidth(output[i], ctx.colors);
    dataLen[i] = len;
    totalLength += len + separatorSpace;
    if (maxLength < len)
      maxLength = len;
  }
  // Add two to `maxLength` as we add a single whitespace character plus a comma
  // in-between two entries.
  const actualMax = maxLength + separatorSpace;
  // Check if at least three entries fit next to each other and prevent grouping
  // of arrays that contains entries of very different length (i.e., if a single
  // entry is longer than 1/5 of all other entries combined). Otherwise the
  // space in-between small entries would be enormous.
  if (actualMax * 3 + ctx.indentationLvl < ctx.breakLength &&
      (totalLength / actualMax > 5 || maxLength <= 6)) {

    const approxCharHeights = 2.5;
    const averageBias = MathSqrt(actualMax - totalLength / output.length);
    const biasedMax = MathMax(actualMax - 3 - averageBias, 1);
    // Dynamically check how many columns seem possible.
    const columns = MathMin(
      // Ideally a square should be drawn. We expect a character to be about 2.5
      // times as high as wide. This is the area formula to calculate a square
      // which contains n rectangles of size `actualMax * approxCharHeights`.
      // Divide that by `actualMax` to receive the correct number of columns.
      // The added bias increases the columns for short entries.
      MathRound(
        MathSqrt(
          approxCharHeights * biasedMax * outputLength,
        ) / biasedMax,
      ),
      // Do not exceed the breakLength.
      MathFloor((ctx.breakLength - ctx.indentationLvl) / actualMax),
      // Limit array grouping for small `compact` modes as the user requested
      // minimal grouping.
      ctx.compact * 4,
      // Limit the columns to a maximum of fifteen.
      15,
    );
    // Return with the original output if no grouping should happen.
    if (columns <= 1) {
      return output;
    }
    const tmp = [];
    const maxLineLength = [];
    for (let i = 0; i < columns; i++) {
      let lineMaxLength = 0;
      for (let j = i; j < output.length; j += columns) {
        if (dataLen[j] > lineMaxLength)
          lineMaxLength = dataLen[j];
      }
      lineMaxLength += separatorSpace;
      maxLineLength[i] = lineMaxLength;
    }
    let order = StringPrototypePadStart;
    if (value !== undefined) {
      for (let i = 0; i < output.length; i++) {
        if (typeof value[i] !== 'number' && typeof value[i] !== 'bigint') {
          order = StringPrototypePadEnd;
          break;
        }
      }
    }
    // Each iteration creates a single line of grouped entries.
    for (let i = 0; i < outputLength; i += columns) {
      // The last lines may contain less entries than columns.
      const max = MathMin(i + columns, outputLength);
      let str = '';
      let j = i;
      for (; j < max - 1; j++) {
        // Calculate extra color padding in case it's active. This has to be
        // done line by line as some lines might contain more colors than
        // others.
        const padding = maxLineLength[j - i] + output[j].length - dataLen[j];
        str += order(`${output[j]}, `, padding, ' ');
      }
      if (order === StringPrototypePadStart) {
        const padding = maxLineLength[j - i] +
                        output[j].length -
                        dataLen[j] -
                        separatorSpace;
        str += StringPrototypePadStart(output[j], padding, ' ');
      } else {
        str += output[j];
      }
      ArrayPrototypePush(tmp, str);
    }
    if (ctx.maxArrayLength < output.length) {
      ArrayPrototypePush(tmp, output[outputLength]);
    }
    output = tmp;
  }
  return output;
}

function formatNamespaceObject(keys, ctx, value, recurseTimes) {
  const output = new Array(keys.length);
  for (let i = 0; i < keys.length; i++) {
    try {
      output[i] = formatProperty(ctx, value, recurseTimes, keys[i],
                                 kObjectType);
    } catch (err) {
      assert(isNativeError(err) && err.name === 'ReferenceError');
      // Use the existing functionality. This makes sure the indentation and
      // line breaks are always correct. Otherwise it is very difficult to keep
      // this aligned, even though this is a hacky way of dealing with this.
      const tmp = { [keys[i]]: '' };
      output[i] = formatProperty(ctx, tmp, recurseTimes, keys[i], kObjectType);
      const pos = StringPrototypeLastIndexOf(output[i], ' ');
      // We have to find the last whitespace and have to replace that value as
      // it will be visualized as a regular string.
      output[i] = StringPrototypeSlice(output[i], 0, pos + 1) +
                  ctx.stylize('<uninitialized>', 'special');
    }
  }
  // Reset the keys to an empty array. This prevents duplicated inspection.
  keys.length = 0;
  return output;
}

// The array is sparse and/or has extra keys
function formatSpecialArray(ctx, value, recurseTimes, maxLength, output, i) {
  const keys = ObjectKeys(value);
  let index = i;
  for (; i < keys.length && output.length < maxLength; i++) {
    const key = keys[i];
    const tmp = +key;
    // Arrays can only have up to 2^32 - 1 entries
    if (tmp > 2 ** 32 - 2) {
      break;
    }
    if (`${index}` !== key) {
      if (RegExpPrototypeExec(numberRegExp, key) === null) {
        break;
      }
      const emptyItems = tmp - index;
      const ending = emptyItems > 1 ? 's' : '';
      const message = `<${emptyItems} empty item${ending}>`;
      ArrayPrototypePush(output, ctx.stylize(message, 'undefined'));
      index = tmp;
      if (output.length === maxLength) {
        break;
      }
    }
    ArrayPrototypePush(output, formatProperty(ctx, value, recurseTimes, key, kArrayType));
    index++;
  }
  const remaining = value.length - index;
  if (output.length !== maxLength) {
    if (remaining > 0) {
      const ending = remaining > 1 ? 's' : '';
      const message = `<${remaining} empty item${ending}>`;
      ArrayPrototypePush(output, ctx.stylize(message, 'undefined'));
    }
  } else if (remaining > 0) {
    ArrayPrototypePush(output, remainingText(remaining));
  }
  return output;
}

function formatArrayBuffer(ctx, value) {
  let buffer;
  try {
    buffer = new Uint8Array(value);
  } catch {
    return [ctx.stylize('(detached)', 'special')];
  }
  const rawString = hexSlice(buffer, 0, MathMin(ctx.maxArrayLength, buffer.length));
  let str = '';
  let i = 0;
  for (; i < rawString.length - 2; i += 2) {
    str += `${rawString[i]}${rawString[i + 1]} `;
  }
  if (rawString.length > 0) {
    str += `${rawString[i]}${rawString[i + 1]}`;
  }
  const remaining = buffer.length - ctx.maxArrayLength;
  if (remaining > 0)
    str += ` ... ${remaining} more byte${remaining > 1 ? 's' : ''}`;
  return [`${ctx.stylize('[Uint8Contents]', 'special')}: <${str}>`];
}

function formatArray(ctx, value, recurseTimes) {
  const valLen = value.length;
  const len = MathMin(MathMax(0, ctx.maxArrayLength), valLen);

  const remaining = valLen - len;
  const output = [];
  for (let i = 0; i < len; i++) {
    const desc = ObjectGetOwnPropertyDescriptor(value, i);
    if (desc === undefined) {
      // Special handle sparse arrays.
      return formatSpecialArray(ctx, value, recurseTimes, len, output, i);
    }
    ArrayPrototypePush(output, formatProperty(ctx, value, recurseTimes, i, kArrayType, desc));
  }
  if (remaining > 0) {
    ArrayPrototypePush(output, remainingText(remaining));
  }
  return output;
}

function formatTypedArray(value, length, ctx) {
  const maxLength = MathMin(MathMax(0, ctx.maxArrayLength), length);
  const remaining = value.length - maxLength;
  const output = new Array(maxLength);
  const elementFormatter = value.length > 0 && typeof value[0] === 'number' ?
    formatNumber :
    formatBigInt;
  for (let i = 0; i < maxLength; ++i) {
    output[i] = elementFormatter(ctx.stylize, value[i], ctx.numericSeparator);
  }
  if (remaining > 0) {
    output[maxLength] = remainingText(remaining);
  }
  return output;
}

function formatSet(value, ctx, ignored, recurseTimes) {
  const length = value.size;
  const maxLength = MathMin(MathMax(0, ctx.maxArrayLength), length);
  const remaining = length - maxLength;
  const output = [];
  ctx.indentationLvl += 2;
  let i = 0;
  for (const v of value) {
    if (i >= maxLength) break;
    ArrayPrototypePush(output, formatValue(ctx, v, recurseTimes));
    i++;
  }
  if (remaining > 0) {
    ArrayPrototypePush(output, remainingText(remaining));
  }
  ctx.indentationLvl -= 2;
  return output;
}

function formatMap(value, ctx, ignored, recurseTimes) {
  const length = value.size;
  const maxLength = MathMin(MathMax(0, ctx.maxArrayLength), length);
  const remaining = length - maxLength;
  const output = [];
  ctx.indentationLvl += 2;
  let i = 0;
  for (const { 0: k, 1: v } of value) {
    if (i >= maxLength) break;
    ArrayPrototypePush(
      output,
      `${formatValue(ctx, k, recurseTimes)} => ${formatValue(ctx, v, recurseTimes)}`,
    );
    i++;
  }
  if (remaining > 0) {
    ArrayPrototypePush(output, remainingText(remaining));
  }
  ctx.indentationLvl -= 2;
  return output;
}

function formatSetIterInner(ctx, recurseTimes, entries, state) {
  const maxArrayLength = MathMax(ctx.maxArrayLength, 0);
  const maxLength = MathMin(maxArrayLength, entries.length);
  const output = new Array(maxLength);
  ctx.indentationLvl += 2;
  for (let i = 0; i < maxLength; i++) {
    output[i] = formatValue(ctx, entries[i], recurseTimes);
  }
  ctx.indentationLvl -= 2;
  if (state === kWeak && !ctx.sorted) {
    // Sort all entries to have a halfway reliable output (if more entries than
    // retrieved ones exist, we can not reliably return the same output) if the
    // output is not sorted anyway.
    ArrayPrototypeSort(output);
  }
  const remaining = entries.length - maxLength;
  if (remaining > 0) {
    ArrayPrototypePush(output, remainingText(remaining));
  }
  return output;
}

function formatMapIterInner(ctx, recurseTimes, entries, state) {
  const maxArrayLength = MathMax(ctx.maxArrayLength, 0);
  // Entries exist as [key1, val1, key2, val2, ...]
  const len = entries.length / 2;
  const remaining = len - maxArrayLength;
  const maxLength = MathMin(maxArrayLength, len);
  const output = new Array(maxLength);
  let i = 0;
  ctx.indentationLvl += 2;
  if (state === kWeak) {
    for (; i < maxLength; i++) {
      const pos = i * 2;
      output[i] =
        `${formatValue(ctx, entries[pos], recurseTimes)} => ${formatValue(ctx, entries[pos + 1], recurseTimes)}`;
    }
    // Sort all entries to have a halfway reliable output (if more entries than
    // retrieved ones exist, we can not reliably return the same output) if the
    // output is not sorted anyway.
    if (!ctx.sorted)
      ArrayPrototypeSort(output);
  } else {
    for (; i < maxLength; i++) {
      const pos = i * 2;
      const res = [
        formatValue(ctx, entries[pos], recurseTimes),
        formatValue(ctx, entries[pos + 1], recurseTimes),
      ];
      output[i] = reduceToSingleString(
        ctx, res, '', ['[', ']'], kArrayExtrasType, recurseTimes);
    }
  }
  ctx.indentationLvl -= 2;
  if (remaining > 0) {
    ArrayPrototypePush(output, remainingText(remaining));
  }
  return output;
}

function formatWeakCollection(ctx) {
  return [ctx.stylize('<items unknown>', 'special')];
}

function formatWeakSet(ctx, value, recurseTimes) {
  const entries = previewEntries(value);
  return formatSetIterInner(ctx, recurseTimes, entries, kWeak);
}

function formatWeakMap(ctx, value, recurseTimes) {
  const entries = previewEntries(value);
  return formatMapIterInner(ctx, recurseTimes, entries, kWeak);
}

function formatIterator(braces, ctx, value, recurseTimes) {
  const { 0: entries, 1: isKeyValue } = previewEntries(value, true);
  if (isKeyValue) {
    // Mark entry iterators as such.
    braces[0] = RegExpPrototypeSymbolReplace(/ Iterator] {$/, braces[0], ' Entries] {');
    return formatMapIterInner(ctx, recurseTimes, entries, kMapEntries);
  }

  return formatSetIterInner(ctx, recurseTimes, entries, kIterator);
}

function formatPromise(ctx, value, recurseTimes) {
  let output;
  const { 0: state, 1: result } = getPromiseDetails(value);
  if (state === kPending) {
    output = [ctx.stylize('<pending>', 'special')];
  } else {
    ctx.indentationLvl += 2;
    const str = formatValue(ctx, result, recurseTimes);
    ctx.indentationLvl -= 2;
    output = [
      state === kRejected ?
        `${ctx.stylize('<rejected>', 'special')} ${str}` :
        str,
    ];
  }
  return output;
}

function formatExtraProperties(ctx, value, recurseTimes, key, typedArray) {
  ctx.indentationLvl += 2;
  const str = formatValue(ctx, value[key], recurseTimes, typedArray);
  ctx.indentationLvl -= 2;

  // These entries are mainly getters. Should they be formatted like getters?
  const name = ctx.stylize(`[${key}]`, 'string');
  return `${name}: ${str}`;
}

function hexSlice(bytes, start, end) {
  let out = '';
  for (let i = start; i < end; i++) {
    out += (bytes[i] < 16 ? '0' : '') + NumberPrototypeToString(bytes[i], 16);
  }
  return out;
}

module.exports = {
  init,
  formatNamespaceObject,
  formatArrayBuffer,
  formatArray,
  formatTypedArray,
  formatSet,
  formatMap,
  formatWeakCollection,
  formatWeakSet,
  formatWeakMap,
  formatIterator,
  formatPromise,
  formatExtraProperties,
  groupArrayElements,
};
