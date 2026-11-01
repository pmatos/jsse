'use strict';

// The text of the fatal-error report `node` prints to stderr for an uncaught
// exception or unhandled rejection (#1229): the source excerpt with a caret,
// then the error as util.inspect shows it. The caller adds "Node.js vX".

const {
  ErrorPrototype,
  ObjectGetOwnPropertyDescriptor,
  ObjectGetPrototypeOf,
  ObjectPrototype,
  ObjectPrototypeToString,
  FunctionPrototypeToString,
  RegExpPrototypeExec,
  SafeWeakSet,
  String,
  StringPrototypeSlice,
  StringPrototypeSplit,
  SymbolPrototypeToString,
  SymbolToStringTag,
} = primordials;

const { inspect } = require('internal/util/inspect');
const { isNativeError, isProxy } = require('internal/util/types');
const { evalSource, throwSite } = require('internal/host/stack');
const { shouldColorize } = require('internal/util/colors');

const rejectionErrors = new SafeWeakSet();
const decorated = new SafeWeakSet();

const INTERNAL_HEADER = 'node:internal/process/promises:322\n' +
  '      new UnhandledPromiseRejection(reason);\n' +
  '      ^\n';

const ENTRY_HEADER = 'node:internal/modules/run_main:107\n' +
  '    triggerUncaughtException(\n' +
  '    ^\n';

const TRACE_HINT = '(Use `node --trace-uncaught ...` to show where the ' +
  'exception was thrown)';

function dataProperty(object, key) {
  let current = object;
  for (let i = 0; current !== null && i < 100; i++) {
    if (isProxy(current)) return undefined;
    const descriptor = ObjectGetOwnPropertyDescriptor(current, key);
    if (descriptor !== undefined) {
      return 'value' in descriptor ? descriptor.value : undefined;
    }
    current = ObjectGetPrototypeOf(current);
  }
  return undefined;
}

// V8's Object::NoSideEffectsToString, as used in the rejection message.
function noSideEffectsToString(value) {
  switch (typeof value) {
    case 'string': return value;
    case 'symbol': return SymbolPrototypeToString(value);
    case 'function': return FunctionPrototypeToString(value);
    case 'object': break;
    default: return String(value);
  }
  if (value === null) return 'null';
  if (isNativeError(value) ||
      dataProperty(value, 'toString') === ErrorPrototype.toString) {
    const name = dataProperty(value, 'name');
    const message = dataProperty(value, 'message');
    const head = typeof name === 'string' ? name : 'Error';
    return typeof message === 'string' && message !== '' ?
      `${head}: ${message}` : head;
  }
  if (dataProperty(value, 'toString') === ObjectPrototype.toString) {
    const ctor = dataProperty(value, 'constructor');
    if (typeof ctor === 'function' && !isProxy(ctor)) {
      const name = ObjectGetOwnPropertyDescriptor(ctor, 'name');
      if (name !== undefined && typeof name.value === 'string' &&
          name.value !== '') {
        return `#<${name.value}>`;
      }
    }
  }
  const tag = dataProperty(value, SymbolToStringTag);
  if (typeof tag === 'string') return `[object ${tag}]`;
  try {
    return ObjectPrototypeToString(value);
  } catch {
    return '[object Object]';
  }
}

function unhandledRejectionError(reason) {
  const error = new Error('This error originated either by throwing inside ' +
    'of an async function without a catch block, or by rejecting a promise ' +
    'which was not handled with .catch(). The promise rejected with the ' +
    `reason "${noSideEffectsToString(reason)}".`);
  error.name = 'UnhandledPromiseRejection';
  error.stack = `UnhandledPromiseRejection: ${error.message}\n` +
    '    at throwUnhandledRejectionsMode (node:internal/process/promises:322:7)\n' +
    '    at processPromiseRejections (node:internal/process/promises:405:17)\n' +
    '    at process.processTicksAndRejections ' +
    '(node:internal/process/task_queues:106:37)';
  error.code = 'ERR_UNHANDLED_REJECTION';
  rejectionErrors.add(error);
  return error;
}

function topFrame(error) {
  let stack;
  try {
    stack = error.stack;
  } catch {
    return undefined;
  }
  if (typeof stack !== 'string') return undefined;
  const start = stack.indexOf('\n    at ');
  if (start === -1) return undefined;
  let end = stack.indexOf('\n', start + 1);
  if (end === -1) end = stack.length;
  const match = RegExpPrototypeExec(
    /^\s+at (?:.* \()?(.*):(\d+):(\d+)\)?$/,
    StringPrototypeSlice(stack, start + 1, end));
  return match === null ? undefined : [match[1], +match[2], +match[3]];
}

// The unpatched synchronous fs, so user code cannot run during the report.
function sourceOf(file) {
  if (file === '[eval]') return evalSource();
  require('fs');
  const { readFileSync } = require('internal/host/fs').sync;
  if (file.startsWith('file://')) {
    return readFileSync(require('url').fileURLToPath(file), 'utf8');
  }
  return readFileSync(file, 'utf8');
}

// util.inspect as the report shows it: coloured when stderr is, as for a
// console, which a TTY or FORCE_COLOR decides.
function show(value) {
  let colors = false;
  try {
    colors = !!shouldColorize(process.stderr);
  } catch {
    // plain text
  }
  return inspect(value, { colors, customInspect: false, depth: 5 });
}

// file:line, the source line and a caret under the column, ending in a newline.
function sourceExcerpt(error, native, rejected) {
  let site = rejected ? undefined : throwSite(error);
  if (site === undefined && (native || typeof error === 'object')) {
    site = topFrame(error);
  }
  if (site === undefined) return '';
  const file = site[0];
  if (typeof file !== 'string' || file.startsWith('node:') ||
      file.startsWith('<')) {
    return '';
  }
  let line;
  try {
    const lines = StringPrototypeSplit(sourceOf(file), /\r\n|[\n\r\u2028\u2029]/);
    line = lines[site[1] - 1];
  } catch {
    return '';
  }
  if (line === undefined) return '';
  let underline = '';
  for (let i = 0; i < site[2] - 1; i++) {
    underline += line[i] === '\t' ? '\t' : ' ';
  }
  return `${file}:${site[1]}\n${line}\n${underline}^\n`;
}

// Node prefixes the stack of an object thrown by the `-e` program itself with
// the excerpt, so a handler of `uncaughtException` sees it there.
function decorate(error) {
  if ((typeof error !== 'object' || error === null) &&
      typeof error !== 'function') {
    return;
  }
  const excerpt = sourceExcerpt(error, isNativeError(error), false);
  if (excerpt === '') return;
  try {
    const stack = error.stack;
    if (typeof stack !== 'string') return;
    error.stack = `${excerpt}\n${stack}`;
    if (error.stack === `${excerpt}\n${stack}`) decorated.add(error);
  } catch {
    // the stack stays as it was
  }
}

// A SyntaxError of the ES module loader (#1484): the stack is the excerpt and
// the error, without Node's internal frames, so inspect would bracket it.
const LOADER_STACK = /^[^\n]+:\d+\n[^\n]*\n[ \t]*\^*\n\n?SyntaxError: /;

function loaderStack(error) {
  let stack;
  try {
    stack = error.stack;
  } catch {
    return undefined;
  }
  if (typeof stack !== 'string' || stack.includes('\n    at ')) {
    return undefined;
  }
  return RegExpPrototypeExec(LOADER_STACK, stack) === null ? undefined : stack;
}

// `mode`: true for a rejection, 'entry' for an error that ended the ES module
// entry point (a rejection too, but a non-error is reported raw under the
// loader's header), 'eval' for a throw of the `-e` program.
function report(error, mode) {
  const isObject = (typeof error === 'object' && error !== null) ||
    typeof error === 'function';
  const native = isObject && isNativeError(error);
  const loaded = native ? loaderStack(error) : undefined;
  if (loaded !== undefined) return loaded;
  let header;
  if (rejectionErrors.has(error)) {
    header = INTERNAL_HEADER;
  } else if (mode === 'entry' && !native) {
    header = ENTRY_HEADER;
  } else if (decorated.has(error)) {
    header = '';
  } else {
    header = sourceExcerpt(error, native, mode === true || mode === 'entry');
  }
  if (native) {
    return `${header}${header === '' ? '' : '\n'}` +
      show(error);
  }
  if (isObject) {
    // Node lays out an object thrown by the `-e` program itself differently.
    const text = show(error);
    if (decorated.has(error)) return text;
    return mode === 'eval' && header !== '' ? `${header}\n${text}` : `\n${header}${text}`;
  }
  const shown = typeof error === 'symbol' ? '' : String(error);
  return `\n${header}${shown}\n${TRACE_HINT}`;
}

module.exports = { decorate, report, unhandledRejectionError };
