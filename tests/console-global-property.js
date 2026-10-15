// Host compatibility regression for #1021. The Console Standard exposes the
// console namespace on the global object; Web IDL namespace bindings specify a
// writable, non-enumerable, configurable data property.
// https://console.spec.whatwg.org/#namespace-console
// https://webidl.spec.whatwg.org/#dfn-define-the-global-property-references

function sameValue(actual, expected, label) {
  if (!Object.is(actual, expected)) {
    throw new Error(label + ": unexpected value");
  }
}

const original = console;
sameValue(console, globalThis.console, "initial identity");
const descriptor = Object.getOwnPropertyDescriptor(globalThis, "console");
sameValue(descriptor.value, original, "initial property value");
sameValue(descriptor.writable, true, "writable");
sameValue(descriptor.enumerable, false, "non-enumerable");
sameValue(descriptor.configurable, true, "configurable");

// Reuse the same name sites across replacement and descriptor changes so the
// compiled global access hints must keep observing the current property.
function readConsole() { return console; }
function writeConsole(value) { "use strict"; console = value; }
sameValue(readConsole(), original, "function initial identity");

let calls = 0;
const replacement = {
  log(message) {
    sameValue(this, replacement, "replacement receiver");
    sameValue(message, "this should be suppressed", "replacement argument");
    calls++;
  }
};
globalThis.console = replacement;
sameValue(console, replacement, "property replacement");
sameValue(readConsole(), replacement, "function sees replacement");
console.log("this should be suppressed");
sameValue(calls, 1, "replacement called instead of host logger");

console = original;
sameValue(globalThis.console, original, "bare-name replacement");
writeConsole(replacement);
sameValue(globalThis.console, replacement, "strict function replacement");

let current = original;
Object.defineProperty(globalThis, "console", {
  get() { return current; },
  set(value) { current = value; },
  configurable: true
});
sameValue(readConsole(), original, "redefined getter");
writeConsole(replacement);
sameValue(current, replacement, "redefined setter");
sameValue(readConsole(), replacement, "getter after setter");

sameValue(delete globalThis.console, true, "property deletion");
sameValue(typeof console, "undefined", "deleted binding");
let missing = false;
try { readConsole(); } catch (error) { missing = error instanceof ReferenceError; }
sameValue(missing, true, "deleted name throws ReferenceError");

Object.defineProperty(globalThis, "console", {
  value: original, writable: false, enumerable: false, configurable: true
});
sameValue(readConsole(), original, "recreated property");
let readOnly = false;
try { writeConsole(replacement); } catch (error) { readOnly = error instanceof TypeError; }
sameValue(readOnly, true, "strict assignment to read-only property");
sameValue(readConsole(), original, "read-only property unchanged");

Object.defineProperty(globalThis, "console", descriptor);
sameValue(delete console, true, "bare-name deletion");
sameValue(Object.hasOwn(globalThis, "console"), false, "bare delete removes property");
Object.defineProperty(globalThis, "console", descriptor);
sameValue(readConsole(), original, "restored identity");
