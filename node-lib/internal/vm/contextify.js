'use strict';

// Node's `contextify` binding over the realm primitives of the host floor
// (src/interpreter/builtins/node_host_vm.rs). A context is a fresh realm. The
// contextified object stays the user-visible handle; the realm's global object
// is kept in step with it (./sync), which is what the C++ named-property
// interceptors do. See
// docs/adr/2026-10-26-2300-node-host-vm.md.

const {
  ArrayPrototypeJoin,
  ErrorCaptureStackTrace,
  ObjectDefineProperty,
  PromiseResolve,
  ReflectApply,
  RegExpPrototypeExec,
  SafeWeakMap,
  StringPrototypeIncludes,
  StringPrototypeRepeat,
  Symbol,
} = primordials;

const host = require('internal/host/vm');
const { createSync } = require('internal/vm/sync');

const symbols = {
  __proto__: null,
  vm_context_no_contextify: Symbol('vm_context_no_contextify'),
  vm_dynamic_import_main_context_default:
    Symbol('vm_dynamic_import_main_context_default'),
};

// contextified object (or DONT_CONTEXTIFY global) -> { global, sync }: the
// realm's global object and what keeps it in step with the object.
const contexts = new SafeWeakMap();
const releaser = new FinalizationRegistry((id) => host.release(id));

const V8_CONSOLE_METHODS = [
  'debug', 'error', 'info', 'log', 'warn', 'dir', 'dirxml', 'table', 'trace',
  'group', 'groupCollapsed', 'groupEnd', 'clear', 'count', 'countReset',
  'assert', 'profile', 'profileEnd', 'time', 'timeLog', 'timeEnd',
  'timeStamp', 'context',
];

function defineHidden(object, key, value) {
  ObjectDefineProperty(object, key, {
    __proto__: null,
    value,
    writable: true,
    enumerable: false,
    configurable: true,
  });
}

// What every V8 context has and a bare jsse realm does not: the no-op console
// and the V8 Error API.
function prepareGlobal(plain) {
  const console = {};
  for (let i = 0; i < V8_CONSOLE_METHODS.length; i++) {
    const name = V8_CONSOLE_METHODS[i];
    defineHidden(console, name, { [name]() {} }[name]);
  }
  defineHidden(plain, 'console', console);
  const ErrorCtor = plain.Error;
  ObjectDefineProperty(ErrorCtor, 'stackTraceLimit', {
    __proto__: null,
    value: 10,
    writable: true,
    enumerable: true,
    configurable: true,
  });
  defineHidden(ErrorCtor, 'captureStackTrace', ErrorCaptureStackTrace);
}

function makeContext(contextObject) {
  const global = host.newContext();
  prepareGlobal(global);
  host.prepare(global);
  if (contextObject === symbols.vm_context_no_contextify) {
    contexts.set(global, { global, sync: undefined });
    return global;
  }
  const sync = createSync(contextObject, global);
  sync.enter();
  contexts.set(contextObject, { global, sync });
  return contextObject;
}

function isContext(object) {
  return contexts.has(object);
}

// Throws what V8 does for source that does not compile: a SyntaxError of the
// realm the code was compiled for, whose stack begins with the `file:line`,
// source line and arrow Node's decorateErrorStack prints.
function throwSyntaxError(failure, filename, parsingContext) {
  const { 0: message, 1: line, 2: text, 3: column, 4: width } = failure;
  const Ctor = parsingContext === undefined ? SyntaxError :
    contexts.get(parsingContext).global.SyntaxError;
  const error = new Ctor(message);
  ErrorCaptureStackTrace(error, throwSyntaxError);
  const stack = error.stack;
  if (typeof stack === 'string') {
    defineHidden(error, 'stack',
                 `${filename}:${line}\n${text}\n` +
                 `${StringPrototypeRepeat(' ', column)}` +
                 `${StringPrototypeRepeat('^', width)}\n\n${stack}`);
  }
  throw error;
}

function lastDirective(code, name) {
  if (!StringPrototypeIncludes(code, `${name}=`)) return undefined;
  const re = new RegExp(
    `(?:^|[\\n\\r\\u2028\\u2029])[ \\t]*//[#@][ \\t]*${name}=[ \\t]*(\\S+)`, 'g');
  let found;
  for (let m = RegExpPrototypeExec(re, code); m !== null;
       m = RegExpPrototypeExec(re, code)) {
    found = m[1];
  }
  return found;
}

class ContextifyScript {
  #id;

  constructor(code, filename, lineOffset, columnOffset, cachedData,
              produceCachedData, parsingContext) {
    const id = host.compile(code, filename, lineOffset, columnOffset);
    if (typeof id !== 'number') throwSyntaxError(id, filename, parsingContext);
    this.#id = id;
    releaser.register(this, id, this);
    if (cachedData !== undefined) this.cachedDataRejected = false;
    if (produceCachedData) {
      this.cachedData = this.createCachedData();
      this.cachedDataProduced = true;
    }
    this.sourceURL = lastDirective(code, 'sourceURL');
    this.sourceMapURL = lastDirective(code, 'sourceMappingURL');
  }

  runInContext(contextifiedObject) {
    const context = contextifiedObject === null ? undefined :
      contexts.get(contextifiedObject);
    if (context === undefined) return host.run(undefined, this.#id);
    const { global, sync } = context;
    if (sync === undefined) return host.run(global, this.#id);
    sync.enter();
    let result;
    try {
      result = host.run(global, this.#id);
    } catch (error) {
      // The script's own exception wins over one from the sandbox's traps.
      try { sync.leave(); } catch { /* ignored */ }
      throw error;
    }
    sync.leave();
    return result;
  }

  // For a script nobody else holds (the one-shot runIn* functions).
  static release(script) {
    releaser.unregister(script);
    host.release(script.#id);
  }

  createCachedData() {
    return Buffer.from('jsse-code-cache');
  }
}

function compileFunction(code, filename, lineOffset, columnOffset, cachedData,
                         produceCachedData, parsingContext, contextExtensions,
                         params) {
  const wrapper = `(function (${params === undefined ? '' :
    ArrayPrototypeJoin(params, ', ')}) {\n${code}\n})`;
  const context = parsingContext === undefined ? undefined :
    contexts.get(parsingContext);
  const result = ReflectApply(host.compileFunction, undefined, [
    wrapper, filename, lineOffset, columnOffset,
    context === undefined ? undefined : context.global,
    ...contextExtensions,
  ]);
  if (typeof result !== 'function') {
    throwSyntaxError(result, filename, parsingContext);
  }
  const compiled = { function: result };
  if (cachedData !== undefined) compiled.cachedDataRejected = false;
  return compiled;
}

const measureMemory = {
  mode: { SUMMARY: 0, DETAILED: 1 },
  execution: { DEFAULT: 0, EAGER: 1 },
};

function measure(mode) {
  const used = process.memoryUsage().heapUsed;
  const total = { jsMemoryEstimate: used, jsMemoryRange: [used, used] };
  return PromiseResolve(mode === measureMemory.mode.DETAILED ?
    { total, current: total, other: [] } : { total });
}

module.exports = {
  ContextifyScript,
  compileFunction,
  constants: { measureMemory },
  isContext,
  makeContext,
  measure,
  symbols,
  contexts,
};
