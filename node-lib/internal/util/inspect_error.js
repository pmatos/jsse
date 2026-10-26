'use strict';

const {
  ArrayIsArray,
  ArrayPrototypeIncludes,
  ArrayPrototypeIndexOf,
  ArrayPrototypeJoin,
  ArrayPrototypePush,
  ArrayPrototypeSplice,
  Error,
  ErrorPrototypeToString,
  MathMin,
  ObjectPrototypeHasOwnProperty,
  ObjectPrototypeToString,
  RegExpPrototypeExec,
  SafeMap,
  SafeSet,
  StringPrototypeEndsWith,
  StringPrototypeIncludes,
  StringPrototypeIndexOf,
  StringPrototypeRepeat,
  StringPrototypeReplace,
  StringPrototypeReplaceAll,
  StringPrototypeSlice,
  StringPrototypeSplit,
  StringPrototypeStartsWith,
} = primordials;

const { isError } = require('internal/util');
const { builtinExists } = require('internal/binding/util');

let getPrefix, formatValue;

function init(deps) {
  getPrefix = deps.getPrefix;
  formatValue = deps.formatValue;
}


function safeGetCWD() {
  let workingDirectory;
  try {
    workingDirectory = process.cwd();
  } catch {
    return;
  }
  return workingDirectory;
}

const coreModuleRegExp = /^ {4}at (?:[^/\\(]+ \(|)node:(.+):\d+:\d+\)?$/;

function identicalSequenceRange(a, b) {
  for (let i = 0; i < a.length - 3; i++) {
    // Find the first entry of b that matches the current entry of a.
    const pos = ArrayPrototypeIndexOf(b, a[i]);
    if (pos !== -1) {
      const rest = b.length - pos;
      if (rest > 3) {
        let len = 1;
        const maxLen = MathMin(a.length - i, rest);
        // Count the number of consecutive entries.
        while (maxLen > len && a[i + len] === b[pos + len]) {
          len++;
        }
        if (len > 3) {
          return [len, i];
        }
      }
    }
  }

  return [0, 0];
}

function getDuplicateErrorFrameRanges(frames) {
  // Build a map: frame line -> sorted list of indices where it occurs
  const result = [];
  const lineToPositions = new SafeMap();

  for (let i = 0; i < frames.length; i++) {
    const positions = lineToPositions.get(frames[i]);
    if (positions === undefined) {
      lineToPositions.set(frames[i], [i]);
    } else {
      positions[positions.length] = i;
    }
  }

  const minimumDuplicateRange = 3;
  // Not enough duplicate lines to consider collapsing
  if (frames.length - lineToPositions.size <= minimumDuplicateRange) {
    return result;
  }

  for (let i = 0; i < frames.length - minimumDuplicateRange; i++) {
    const positions = lineToPositions.get(frames[i]);
    // Find the next occurrence of the same line after i, if any
    if (positions.length === 1 || positions[positions.length - 1] === i) {
      continue;
    }

    const current = positions.indexOf(i) + 1;
    if (current === positions.length) {
      continue;
    }

    // Theoretical maximum range, adjusted while iterating
    let range = positions[positions.length - 1] - i;
    if (range < minimumDuplicateRange) {
      continue;
    }
    let extraSteps;
    if (current + 1 < positions.length) {
      // Optimize initial step size by choosing the greatest common divisor (GCD)
      // of all candidate distances to the same frame line. This tends to match
      // the true repeating block size and minimizes fallback iterations.
      let gcdRange = 0;
      for (let j = current; j < positions.length; j++) {
        let distance = positions[j] - i;
        while (distance !== 0) {
          const remainder = gcdRange % distance;
          if (gcdRange !== 0) {
            // Add other possible ranges as fallback
            extraSteps ??= new SafeSet();
            extraSteps.add(gcdRange);
          }
          gcdRange = distance;
          distance = remainder;
        }
        if (gcdRange === 1) break;
      }
      range = gcdRange;
      if (extraSteps) {
        extraSteps.delete(range);
        extraSteps = [...extraSteps];
      }
    }
    let maxRange = range;
    let maxDuplicates = 0;

    let duplicateRanges = 0;

    for (let nextStart = i + range; /* ignored */ ; nextStart += range) {
      let equalFrames = 0;
      for (let j = 0; j < range; j++) {
        if (frames[i + j] !== frames[nextStart + j]) {
          break;
        }
        equalFrames++;
      }
      // Adjust the range to match different type of ranges.
      if (equalFrames !== range) {
        if (!extraSteps?.length) {
          break;
        }
        // Memorize former range in case the smaller one would hide less.
        if (duplicateRanges !== 0 && maxRange * maxDuplicates < range * duplicateRanges) {
          maxRange = range;
          maxDuplicates = duplicateRanges;
        }
        range = extraSteps.pop();
        nextStart = i;
        duplicateRanges = 0;
        continue;
      }
      duplicateRanges++;
    }

    if (maxDuplicates !== 0 && maxRange * maxDuplicates >= range * duplicateRanges) {
      range = maxRange;
      duplicateRanges = maxDuplicates;
    }

    if (duplicateRanges * range >= 3) {
      result.push(i + range, range, duplicateRanges);
      // Skip over the collapsed portion to avoid overlapping matches.
      i += range * (duplicateRanges + 1) - 1;
    }
  }

  return result;
}

function getStackString(ctx, error) {
  let stack;
  try {
    stack = error.stack;
  } catch {
    // If stack is getter that throws, we ignore the error.
  }
  if (stack) {
    if (typeof stack === 'string') {
      return stack;
    }
    ctx.seen.push(error);
    ctx.indentationLvl += 4;
    const result = formatValue(ctx, stack);
    ctx.indentationLvl -= 4;
    ctx.seen.pop();
    return `${ErrorPrototypeToString(error)}\n    ${result}`;
  }
  return ErrorPrototypeToString(error);
}

function getStackFrames(ctx, err, stack) {
  const frames = StringPrototypeSplit(stack, '\n');

  let cause;
  try {
    ({ cause } = err);
  } catch {
    // If 'cause' is a getter that throws, ignore it.
  }

  // Remove stack frames identical to frames in cause.
  if (cause != null && isError(cause)) {
    const causeStack = getStackString(ctx, cause);
    const causeStackStart = StringPrototypeIndexOf(causeStack, '\n    at');
    if (causeStackStart !== -1) {
      const causeFrames = StringPrototypeSplit(StringPrototypeSlice(causeStack, causeStackStart + 1), '\n');
      const { 0: len, 1: offset } = identicalSequenceRange(frames, causeFrames);
      if (len > 0) {
        const skipped = len - 2;
        const msg = `    ... ${skipped} lines matching cause stack trace ...`;
        frames.splice(offset + 1, skipped, ctx.stylize(msg, 'undefined'));
      }
    }
  }

  // Remove recursive repetitive stack frames in long stacks
  if (frames.length > 10) {
    const ranges = getDuplicateErrorFrameRanges(frames);

    for (let i = ranges.length - 3; i >= 0; i -= 3) {
      const offset = ranges[i];
      const length = ranges[i + 1];
      const duplicateRanges = ranges[i + 2];

      const msg = `    ... collapsed ${length * duplicateRanges} duplicate lines ` +
        'matching above ' +
        (duplicateRanges > 1 ?
          `${length} lines ${duplicateRanges} times...` :
          'lines ...');
      frames.splice(offset, length * duplicateRanges, ctx.stylize(msg, 'undefined'));
    }
  }

  return frames;
}

/** @type {(stack: string, constructor: string | null, name: unknown, tag: string) => string} */
function improveStack(stack, constructor, name, tag) {
  // A stack trace may contain arbitrary data. Only manipulate the output
  // for "regular errors" (errors that "look normal") for now.
  let len = name.length;

  if (typeof name !== 'string') {
    stack = StringPrototypeReplace(
      stack,
      `${name}`,
      `${name} [${StringPrototypeSlice(getPrefix(constructor, tag, 'Error'), 0, -1)}]`,
    );
  }

  if (constructor === null ||
      (StringPrototypeEndsWith(name, 'Error') &&
      StringPrototypeStartsWith(stack, name) &&
      (stack.length === len || stack[len] === ':' || stack[len] === '\n'))) {
    let fallback = 'Error';
    if (constructor === null) {
      const start = RegExpPrototypeExec(/^([A-Z][a-z_ A-Z0-9[\]()-]+)(?::|\n {4}at)/, stack) ||
      RegExpPrototypeExec(/^([a-z_A-Z0-9-]*Error)$/, stack);
      fallback = (start?.[1]) || '';
      len = fallback.length;
      fallback ||= 'Error';
    }
    const prefix = StringPrototypeSlice(getPrefix(constructor, tag, fallback), 0, -1);
    if (name !== prefix) {
      if (StringPrototypeIncludes(prefix, name)) {
        if (len === 0) {
          stack = `${prefix}: ${stack}`;
        } else {
          stack = `${prefix}${StringPrototypeSlice(stack, len)}`;
        }
      } else {
        stack = `${prefix} [${name}]${StringPrototypeSlice(stack, len)}`;
      }
    }
  }
  return stack;
}

function markNodeModules(ctx, line) {
  let tempLine = '';
  let lastPos = 0;
  let searchFrom = 0;

  while (true) {
    const nodeModulePosition = StringPrototypeIndexOf(line, 'node_modules', searchFrom);
    if (nodeModulePosition === -1) {
      break;
    }

    // Ensure it's a path segment: must have a path separator before and after
    const separator = line[nodeModulePosition - 1];
    const after = line[nodeModulePosition + 12]; // 'node_modules'.length === 12

    if ((after !== '/' && after !== '\\') || (separator !== '/' && separator !== '\\')) {
      // Not a proper segment; continue searching
      searchFrom = nodeModulePosition + 1;
      continue;
    }

    const moduleStart = nodeModulePosition + 13; // Include trailing separator

    // Append up to and including '/node_modules/'
    tempLine += StringPrototypeSlice(line, lastPos, moduleStart);

    let moduleEnd = StringPrototypeIndexOf(line, separator, moduleStart);
    if (moduleEnd === -1) {
      // No trailing separator: the module name runs to the end of the line.
      moduleEnd = line.length;
    } else if (line[moduleStart] === '@') {
      // Namespaced modules have an extra slash: @namespace/package
      moduleEnd = StringPrototypeIndexOf(line, separator, moduleEnd + 1);
      if (moduleEnd === -1) {
        moduleEnd = line.length;
      }
    }

    const nodeModule = StringPrototypeSlice(line, moduleStart, moduleEnd);
    tempLine += ctx.stylize(nodeModule, 'module');

    lastPos = moduleEnd;
    searchFrom = moduleEnd;
  }

  if (lastPos !== 0) {
    line = tempLine + StringPrototypeSlice(line, lastPos);
  }
  return line;
}

function markCwd(ctx, line, workingDirectory) {
  let cwdStartPos = StringPrototypeIndexOf(line, workingDirectory);
  let tempLine = '';
  let cwdLength = workingDirectory.length;
  if (cwdStartPos !== -1) {
    if (StringPrototypeSlice(line, cwdStartPos - 7, cwdStartPos) === 'file://') {
      cwdLength += 7;
      cwdStartPos -= 7;
    }
    const start = line[cwdStartPos - 1] === '(' ? cwdStartPos - 1 : cwdStartPos;
    const end = start !== cwdStartPos && StringPrototypeEndsWith(line, ')') ? -1 : line.length;
    const workingDirectoryEndPos = cwdStartPos + cwdLength + 1;
    const cwdSlice = StringPrototypeSlice(line, start, workingDirectoryEndPos);

    tempLine += StringPrototypeSlice(line, 0, start);
    tempLine += ctx.stylize(cwdSlice, 'undefined');
    tempLine += StringPrototypeSlice(line, workingDirectoryEndPos, end);
    if (end === -1) {
      tempLine += ctx.stylize(')', 'undefined');
    }
  } else {
    tempLine += line;
  }
  return tempLine;
}

function formatError(err, constructor, tag, ctx, keys) {
  let message, name, stack;
  try {
    stack = getStackString(ctx, err);
  } catch {
    return ObjectPrototypeToString(err);
  }

  let messageIsGetterThatThrows = false;
  try {
    message = err.message;
  } catch {
    messageIsGetterThatThrows = true;
  }
  let nameIsGetterThatThrows = false;
  try {
    name = err.name;
  } catch {
    nameIsGetterThatThrows = true;
  }

  if (!ctx.showHidden && keys.length !== 0) {
    const index = ArrayPrototypeIndexOf(keys, 'stack');
    if (index !== -1) {
      ArrayPrototypeSplice(keys, index, 1);
    }

    if (!messageIsGetterThatThrows) {
      const index = ArrayPrototypeIndexOf(keys, 'message');
      // Only hide the property if it's a string and if it's part of the original stack
      if (index !== -1 && (typeof message !== 'string' || StringPrototypeIncludes(stack, message))) {
        ArrayPrototypeSplice(keys, index, 1);
      }
    }

    if (!nameIsGetterThatThrows) {
      const index = ArrayPrototypeIndexOf(keys, 'name');
      // Only hide the property if it's a string and if it's part of the original stack
      if (index !== -1 && (typeof name !== 'string' || StringPrototypeIncludes(stack, name))) {
        ArrayPrototypeSplice(keys, index, 1);
      }
    }
  }
  name ??= 'Error';

  if (ObjectPrototypeHasOwnProperty(err, 'cause') &&
      (keys.length === 0 || !ArrayPrototypeIncludes(keys, 'cause'))) {
    ArrayPrototypePush(keys, 'cause');
  }

  // Print errors aggregated into AggregateError
  try {
    const errors = err.errors;
    if (ArrayIsArray(errors) && ObjectPrototypeHasOwnProperty(err, 'errors') &&
      (keys.length === 0 || !ArrayPrototypeIncludes(keys, 'errors'))) {
      ArrayPrototypePush(keys, 'errors');
    }
  } catch {
    // If errors is a getter that throws, we ignore the error.
  }

  stack = improveStack(stack, constructor, name, tag);

  // Ignore the error message if it's contained in the stack.
  let pos = (message && StringPrototypeIndexOf(stack, message)) || -1;
  if (pos !== -1)
    pos += message.length;
  // Wrap the error in brackets in case it has no stack trace.
  const stackStart = StringPrototypeIndexOf(stack, '\n    at', pos);
  if (stackStart === -1) {
    stack = `[${stack}]`;
  } else {
    let newStack = StringPrototypeSlice(stack, 0, stackStart);
    const stackFramePart = StringPrototypeSlice(stack, stackStart + 1);
    const lines = getStackFrames(ctx, err, stackFramePart);
    if (ctx.colors) {
      // Highlight userland code and node modules.
      const workingDirectory = safeGetCWD();
      let esmWorkingDirectory;
      for (let line of lines) {
        const core = RegExpPrototypeExec(coreModuleRegExp, line);
        if (core !== null && builtinExists(core[1])) {
          newStack += `\n${ctx.stylize(line, 'undefined')}`;
        } else {
          newStack += '\n';

          line = markNodeModules(ctx, line);
          if (workingDirectory !== undefined) {
            let newLine = markCwd(ctx, line, workingDirectory);
            if (newLine === line) {
              esmWorkingDirectory ??= pathToFileUrlHref(workingDirectory);
              newLine = markCwd(ctx, line, esmWorkingDirectory);
            }
            line = newLine;
          }

          newStack += line;
        }
      }
    } else {
      newStack += `\n${ArrayPrototypeJoin(lines, '\n')}`;
    }
    stack = newStack;
  }
  // The message and the stack have to be indented as well!
  if (ctx.indentationLvl !== 0) {
    const indentation = StringPrototypeRepeat(' ', ctx.indentationLvl);
    stack = StringPrototypeReplaceAll(stack, '\n', `\n${indentation}`);
  }
  return stack;
}

function handleMaxCallStackSize(ctx, err, constructorName, indentationLvl) {
  ctx.seen.pop();
  ctx.indentationLvl = indentationLvl;
  return ctx.stylize(
    `[${constructorName}: Inspection interrupted ` +
      'prematurely. Maximum call stack size exceeded.]',
    'special',
  );
}

function pathToFileUrlHref(filepath) {
  const encoded = StringPrototypeReplaceAll(
    StringPrototypeReplaceAll(encodeURI(filepath), '?', '%3F'), '#', '%23');
  return `file://${encoded}`;
}

module.exports = {
  init,
  identicalSequenceRange,
  formatError,
  handleMaxCallStackSize,
};
