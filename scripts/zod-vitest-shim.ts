// Focused Vitest compatibility module for Zod's v4 classic runtime suite.
// The shared TAP harness supplies registration/lifecycle behavior; Jest's
// published expect core and Vitest's published pretty-printer supply the
// assertion semantics used by the upstream tests.

import matcherModule from "expect/build/matchers";
import {
  equals,
  iterableEquality,
  subsetEquality,
} from "@jest/expect-utils";
import * as matcherUtils from "jest-matcher-utils";
import { format } from "@vitest/pretty-format";
import * as core from "zod/v4/core";

const tapDescribe = globalThis.describe;
const tapTest = globalThis.test;
const tapBeforeEach = globalThis.beforeEach;
const tapAfterEach = globalThis.afterEach;
const allowsEval = core.util.allowsEval;
const allowsEvalAccessor = Object.getOwnPropertyDescriptor(allowsEval, "value");
const bundleJitless = (globalThis as any).__ZOD_BUNDLE_JITLESS__ === true;
const jitlessSkipNames = new Set(["function with async refinements"]);
let defaultConfig: Record<string, unknown> | undefined;

if (!allowsEvalAccessor?.get) {
  throw new Error("Zod allowsEval cache is not resettable");
}

function serialize(value: unknown): string {
  return format(value, {
    escapeRegex: true,
    escapeString: false,
    indent: 2,
    maxOutputLength: 2 ** 27,
    printBasicPrototype: false,
    printFunctionName: false,
  });
}

function normalizeInlineSnapshot(snapshot: unknown): string {
  const text = String(snapshot).replace(/\r\n?|\u2028|\u2029/g, "\n");
  const lines = text.split("\n");
  if (lines.length < 3 || lines[0].trim() || lines[lines.length - 1].trim()) {
    return text;
  }

  const firstContent = lines.find(
    (line, index) => index > 0 && index < lines.length - 1 && line.trim()
  );
  const indentation = firstContent?.match(/^\s*/)?.[0] ?? "";
  if (!indentation) return text;

  const body = lines.slice(1, -1);
  if (body.some((line) => line && !line.startsWith(indentation))) return text;
  return body.map((line) => (line ? line.slice(indentation.length) : line)).join("\n");
}

function snapshotResult(received: unknown, expected: unknown) {
  const actual = serialize(received);
  const normalizedExpected = normalizeInlineSnapshot(expected);
  const pass = actual === normalizedExpected;
  return {
    pass,
    message: () =>
      pass
        ? "Expected value not to match inline snapshot"
        : `Inline snapshot mismatch\nExpected:\n${normalizedExpected}\nReceived:\n${actual}`,
  };
}

function throwResult(received: unknown, expected?: unknown) {
  if (typeof received !== "function") {
    return { pass: false, message: () => "Expected value to be a function" };
  }
  let thrown: unknown;
  try {
    received();
  } catch (error) {
    thrown = error;
  }

  let pass = thrown !== undefined;
  if (pass && expected !== undefined) {
    const message =
      thrown && typeof thrown === "object" && "message" in thrown
        ? String((thrown as any).message)
        : String(thrown);
    if (typeof expected === "function") {
      pass = thrown instanceof (expected as Function);
    }
    else if (typeof expected === "string") pass = message.includes(expected);
    else if (expected instanceof RegExp) pass = expected.test(message);
    else if (expected instanceof Error) pass = message === expected.message;
    else pass = equals(thrown, expected);
  }

  return {
    pass,
    message: () => (pass ? "Expected function not to throw" : "Expected function to throw"),
  };
}

const matcherCore = (matcherModule as any).default ?? matcherModule;
const matchers: Record<string, Function> = {
  ...matcherCore,
  toThrow: throwResult,
  toThrowError: throwResult,
  toBeTypeOf(received: unknown, expected: string) {
    const actual = typeof received;
    const pass = actual === expected;
    return {
      pass,
      message: () => `Expected typeof ${String(received)} to be ${expected}, received ${actual}`,
    };
  },
  toMatchInlineSnapshot(received: unknown, expected: unknown) {
    return snapshotResult(received, expected);
  },
  toThrowErrorMatchingInlineSnapshot(received: unknown, expected: unknown) {
    if (typeof received !== "function") {
      return { pass: false, message: () => "Expected value to be a function" };
    }
    try {
      received();
    } catch (error) {
      return snapshotResult(error, expected);
    }
    return { pass: false, message: () => "Expected function to throw" };
  },
};

const utils = {
  ...matcherUtils,
  iterableEquality,
  subsetEquality,
};

function runMatcher(name: string, actual: unknown, args: unknown[], isNot: boolean) {
  const matcher = matchers[name];
  const result = matcher.call(
    {
      customTesters: [],
      equals,
      expand: false,
      isNot,
      promise: "",
      utils,
    },
    actual,
    ...args
  );
  if (!result || typeof result.pass !== "boolean") {
    throw new Error(`Invalid result from matcher ${name}`);
  }
  if (result.pass === isNot) {
    throw new Error(
      typeof result.message === "function"
        ? result.message()
        : result.message || `Matcher ${name} failed`
    );
  }
}

function matcherSide(actual: unknown, isNot: boolean) {
  const side: Record<string, any> = {};
  for (const name of Object.keys(matchers)) {
    side[name] = (...args: unknown[]) => runMatcher(name, actual, args, isNot);
  }
  return side;
}

function promiseMatcherSide(actual: unknown, mode: "resolves" | "rejects") {
  const side: Record<string, any> = {};
  for (const name of Object.keys(matchers)) {
    side[name] = (...args: unknown[]) => {
      let candidate: unknown;
      try {
        candidate = typeof actual === "function" ? (actual as Function)() : actual;
      } catch (error) {
        candidate = Promise.reject(error);
      }
      return Promise.resolve(candidate).then(
        (value) => {
          if (mode === "rejects") throw new Error("Expected promise to reject, but it resolved");
          runMatcher(name, value, args, false);
        },
        (error) => {
          if (mode === "resolves") throw error;
          const received =
            name === "toThrow" || name === "toThrowError"
              ? () => {
                  throw error;
                }
              : error;
          runMatcher(name, received, args, false);
        }
      );
    };
  }
  return side;
}

export function expect(actual: unknown) {
  const positive = matcherSide(actual, false);
  positive.not = matcherSide(actual, true);
  positive.resolves = promiseMatcherSide(actual, "resolves");
  positive.rejects = promiseMatcherSide(actual, "rejects");
  return positive;
}

function setMode(jitless: boolean) {
  const config = core.globalConfig as Record<string, unknown>;
  defaultConfig ??= { ...config };
  for (const key of Object.keys(config)) delete config[key];
  Object.assign(config, defaultConfig);
  if (jitless) config.jitless = true;
  else delete config.jitless;
  (globalThis as any).__zod_globalConfig = config;
  (globalThis as any).__zod_globalRegistry = core.globalRegistry;
  Object.defineProperty(allowsEval, "value", allowsEvalAccessor);
}

function wrapTest(fn: Function, jitless: boolean) {
  if (fn.length > 0) {
    return function (this: unknown, done: Function) {
      setMode(jitless);
      return fn.call(this, done);
    };
  }
  return function (this: unknown) {
    setMode(jitless);
    return fn.call(this);
  };
}

export function test(name: unknown, fn: Function, timeout?: number) {
  const mode = bundleJitless ? "jitless" : "normal";
  const register =
    bundleJitless && jitlessSkipNames.has(String(name)) ? tapTest.skip : tapTest;
  return register(`${String(name)} [${mode}]`, wrapTest(fn, bundleJitless), timeout);
}

test.skip = function (name: unknown, fn: Function, timeout?: number) {
  const mode = bundleJitless ? "jitless" : "normal";
  return tapTest.skip(`${String(name)} [${mode}]`, wrapTest(fn, bundleJitless), timeout);
};
test.only = test;

export const it = test;
export const describe = tapDescribe;
export const beforeEach = tapBeforeEach;
export const afterEach = tapAfterEach;

let typeExpectation: any;
typeExpectation = new Proxy(function () {
  return typeExpectation;
}, {
  apply() {
    return typeExpectation;
  },
  get() {
    return typeExpectation;
  },
});

// expectTypeOf assertions belong to Vitest's separate TypeScript project. The
// JavaScript-engine corpus preserves their surrounding runtime test bodies but
// deliberately makes the erased type assertions no-ops.
export function expectTypeOf() {
  return typeExpectation;
}
