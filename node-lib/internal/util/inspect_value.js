'use strict';

const {
  AggregateError,
  AggregateErrorPrototype,
  Array,
  ArrayBuffer,
  ArrayBufferPrototype,
  ArrayPrototype,
  ArrayPrototypeFilter,
  ArrayPrototypeForEach,
  ArrayPrototypePop,
  ArrayPrototypePush,
  ArrayPrototypePushApply,
  Boolean,
  BooleanPrototype,
  DataView,
  DataViewPrototype,
  Date,
  DatePrototype,
  Error,
  ErrorPrototype,
  Function,
  FunctionPrototype,
  FunctionPrototypeCall,
  FunctionPrototypeSymbolHasInstance,
  Map,
  MapPrototype,
  Number,
  NumberPrototype,
  Object,
  ObjectGetOwnPropertyDescriptor,
  ObjectGetOwnPropertyNames,
  ObjectGetOwnPropertySymbols,
  ObjectGetPrototypeOf,
  ObjectKeys,
  ObjectPrototype,
  ObjectPrototypeHasOwnProperty,
  ObjectPrototypePropertyIsEnumerable,
  Promise,
  PromisePrototype,
  RangeError,
  RangeErrorPrototype,
  ReflectOwnKeys,
  RegExp,
  RegExpPrototype,
  RegExpPrototypeExec,
  SafeMap,
  SafeSet,
  Set,
  SetPrototype,
  String,
  StringPrototype,
  StringPrototypeRepeat,
  StringPrototypeReplaceAll,
  TypeError,
  TypeErrorPrototype,
  TypedArray,
  TypedArrayPrototype,
  WeakMap,
  WeakMapPrototype,
  WeakSet,
  WeakSetPrototype,
  globalThis,
} = primordials;

const { customInspectSymbol } = require('internal/util');
const { isModuleNamespaceObject, isNativeError } = require('internal/util/types');
const assert = require('internal/assert');
const { getProxyDetails, getConstructorName: internalGetConstructorName } = require('internal/binding/util');

let formatPrimitive, formatRaw, getPrefix, formatProperty, reduceToSingleString, getUserOptions, inspect;

function init(deps) {
  formatPrimitive = deps.formatPrimitive;
  formatRaw = deps.formatRaw;
  getPrefix = deps.getPrefix;
  formatProperty = deps.formatProperty;
  reduceToSingleString = deps.reduceToSingleString;
  getUserOptions = deps.getUserOptions;
  inspect = deps.inspect;
}


function isInstanceof(object, proto) {
  try {
    return object instanceof proto;
  } catch {
    return false;
  }
}

// https://tc39.es/ecma262/#sec-IsHTMLDDA-internal-slot
const isUndetectableObject = (v) => typeof v === 'undefined' && v !== undefined;
const kArrayExtrasType = 2;

const kObjectType = 0;

const builtInObjects = new SafeSet(
  ArrayPrototypeFilter(
    ObjectGetOwnPropertyNames(globalThis),
    (e) => RegExpPrototypeExec(/^[A-Z][a-zA-Z0-9]+$/, e) !== null,
  ),
);

// Special-case for some builtin prototypes in case their `constructor` property has been tampered.
const wellKnownPrototypes = new SafeMap()
  .set(ArrayPrototype, { name: 'Array', constructor: Array })
  .set(ArrayBufferPrototype, { name: 'ArrayBuffer', constructor: ArrayBuffer })
  .set(FunctionPrototype, { name: 'Function', constructor: Function })
  .set(MapPrototype, { name: 'Map', constructor: Map })
  .set(SetPrototype, { name: 'Set', constructor: Set })
  .set(ObjectPrototype, { name: 'Object', constructor: Object })
  .set(TypedArrayPrototype, { name: 'TypedArray', constructor: TypedArray })
  .set(RegExpPrototype, { name: 'RegExp', constructor: RegExp })
  .set(DatePrototype, { name: 'Date', constructor: Date })
  .set(DataViewPrototype, { name: 'DataView', constructor: DataView })

  .set(ErrorPrototype, { name: 'Error', constructor: Error })
  .set(AggregateErrorPrototype, { name: 'AggregateError', constructor: AggregateError })
  .set(RangeErrorPrototype, { name: 'RangeError', constructor: RangeError })
  .set(TypeErrorPrototype, { name: 'TypeError', constructor: TypeError })

  .set(BooleanPrototype, { name: 'Boolean', constructor: Boolean })
  .set(NumberPrototype, { name: 'Number', constructor: Number })
  .set(StringPrototype, { name: 'String', constructor: String })
  .set(PromisePrototype, { name: 'Promise', constructor: Promise })
  .set(WeakMapPrototype, { name: 'WeakMap', constructor: WeakMap })
  .set(WeakSetPrototype, { name: 'WeakSet', constructor: WeakSet });

function getConstructorName(obj, ctx, recurseTimes, protoProps) {
  let firstProto;
  const tmp = obj;
  while (obj || isUndetectableObject(obj)) {
    const wellKnownPrototypeNameAndConstructor = wellKnownPrototypes.get(obj);
    if (wellKnownPrototypeNameAndConstructor !== undefined) {
      const { name, constructor } = wellKnownPrototypeNameAndConstructor;
      if (FunctionPrototypeSymbolHasInstance(constructor, tmp)) {
        if (protoProps !== undefined && firstProto !== obj) {
          addPrototypeProperties(
            ctx, tmp, firstProto || tmp, recurseTimes, protoProps);
        }
        return name;
      }
    }
    const descriptor = ObjectGetOwnPropertyDescriptor(obj, 'constructor');
    if (descriptor !== undefined &&
        typeof descriptor.value === 'function' &&
        descriptor.value.name !== '' &&
        isInstanceof(tmp, descriptor.value)) {
      if (protoProps !== undefined &&
         (firstProto !== obj ||
         !builtInObjects.has(descriptor.value.name))) {
        addPrototypeProperties(
          ctx, tmp, firstProto || tmp, recurseTimes, protoProps);
      }
      return String(descriptor.value.name);
    }

    obj = ObjectGetPrototypeOf(obj);
    if (firstProto === undefined) {
      firstProto = obj;
    }
  }

  if (firstProto === null) {
    return null;
  }

  const res = internalGetConstructorName(tmp);

  if (recurseTimes > ctx.depth && ctx.depth !== null) {
    return `${res} <Complex prototype>`;
  }

  const protoConstr = getConstructorName(
    firstProto, ctx, recurseTimes + 1, protoProps);

  if (protoConstr === null) {
    return `${res} <${inspect(firstProto, {
      ...ctx,
      customInspect: false,
      depth: -1,
    })}>`;
  }

  return `${res} <${protoConstr}>`;
}

// This function has the side effect of adding prototype properties to the
// `output` argument (which is an array). This is intended to highlight user
// defined prototype properties.
function addPrototypeProperties(ctx, main, obj, recurseTimes, output) {
  let depth = 0;
  let keys;
  let keySet;
  do {
    if (depth !== 0 || main === obj) {
      obj = ObjectGetPrototypeOf(obj);
      // Stop as soon as a null prototype is encountered.
      if (obj === null) {
        return;
      }
      // Stop as soon as a built-in object type is detected.
      const descriptor = ObjectGetOwnPropertyDescriptor(obj, 'constructor');
      if (descriptor !== undefined &&
          typeof descriptor.value === 'function' &&
          builtInObjects.has(descriptor.value.name)) {
        return;
      }
    }

    if (depth === 0) {
      keySet = new SafeSet();
    } else {
      ArrayPrototypeForEach(keys, (key) => keySet.add(key));
    }
    // Get all own property names and symbols.
    keys = ReflectOwnKeys(obj);
    ArrayPrototypePush(ctx.seen, main);
    for (const key of keys) {
      // Ignore the `constructor` property and keys that exist on layers above.
      if (key === 'constructor' ||
          ObjectPrototypeHasOwnProperty(main, key) ||
          (depth !== 0 && keySet.has(key))) {
        continue;
      }
      const desc = ObjectGetOwnPropertyDescriptor(obj, key);
      if (typeof desc.value === 'function') {
        continue;
      }
      const value = formatProperty(
        ctx, obj, recurseTimes, key, kObjectType, desc, main);
      if (ctx.colors) {
        // Faint!
        ArrayPrototypePush(output, `\u001b[2m${value}\u001b[22m`);
      } else {
        ArrayPrototypePush(output, value);
      }
    }
    ArrayPrototypePop(ctx.seen);
  // Limit the inspection to up to three prototype layers. Using `recurseTimes`
  // is not a good choice here, because it's as if the properties are declared
  // on the current object from the users perspective.
  } while (++depth !== 3);
}

// Look up the keys of the object.
function getKeys(value, showHidden) {
  let keys;
  const symbols = ObjectGetOwnPropertySymbols(value);
  if (showHidden) {
    keys = ObjectGetOwnPropertyNames(value);
    if (symbols.length !== 0)
      ArrayPrototypePushApply(keys, symbols);
  } else {
    // This might throw if `value` is a Module Namespace Object from an
    // unevaluated module, but we don't want to perform the actual type
    // check because it's expensive.
    // TODO(devsnek): track https://github.com/tc39/ecma262/issues/1209
    // and modify this logic as needed.
    try {
      keys = ObjectKeys(value);
    } catch (err) {
      assert(isNativeError(err) && err.name === 'ReferenceError' &&
             isModuleNamespaceObject(value));
      keys = ObjectGetOwnPropertyNames(value);
    }
    if (symbols.length !== 0) {
      const filter = (key) => ObjectPrototypePropertyIsEnumerable(value, key);
      ArrayPrototypePushApply(keys, ArrayPrototypeFilter(symbols, filter));
    }
  }
  return keys;
}

function getCtxStyle(value, constructor, tag) {
  let fallback = '';
  if (constructor === null) {
    fallback = internalGetConstructorName(value);
    if (fallback === tag) {
      fallback = 'Object';
    }
  }
  return getPrefix(constructor, tag, fallback);
}

function formatProxy(ctx, proxy, recurseTimes) {
  if (recurseTimes > ctx.depth && ctx.depth !== null) {
    return ctx.stylize('Proxy [Array]', 'special');
  }
  recurseTimes += 1;
  ctx.indentationLvl += 2;
  const res = [
    formatValue(ctx, proxy[0], recurseTimes),
    formatValue(ctx, proxy[1], recurseTimes),
  ];
  ctx.indentationLvl -= 2;
  return reduceToSingleString(
    ctx, res, '', ['Proxy [', ']'], kArrayExtrasType, recurseTimes);
}

// Note: using `formatValue` directly requires the indentation level to be
// corrected by setting `ctx.indentationLvL += diff` and then to decrease the
// value afterwards again.
function formatValue(ctx, value, recurseTimes, typedArray) {
  // Primitive types cannot have properties.
  if (typeof value !== 'object' &&
      typeof value !== 'function' &&
      !isUndetectableObject(value)) {
    return formatPrimitive(ctx.stylize, value, ctx);
  }
  if (value === null) {
    return ctx.stylize('null', 'null');
  }

  // Memorize the context for custom inspection on proxies.
  const context = value;
  let proxies = 0;
  // Always check for proxies to prevent side effects and to prevent triggering
  // any proxy handlers.
  let proxy = getProxyDetails(value, !!ctx.showProxy);
  if (proxy !== undefined) {
    if (ctx.showProxy) {
      if (proxy[0] === null) {
        return ctx.stylize('<Revoked Proxy>', 'special');
      }
      return formatProxy(ctx, proxy, recurseTimes);
    }
    do {
      if (proxy === null) {
        let formatted = ctx.stylize('<Revoked Proxy>', 'special');
        for (let i = 0; i < proxies; i++) {
          formatted = `${ctx.stylize('Proxy(', 'special')}${formatted}${ctx.stylize(')', 'special')}`;
        }
        return formatted;
      }
      value = proxy;
      proxy = getProxyDetails(value, false);
      proxies += 1;
    } while (proxy !== undefined);
  }

  // Provide a hook for user-specified inspect functions.
  // Check that value is an object with an inspect function on it.
  if (ctx.customInspect) {
    const maybeCustom = value[customInspectSymbol];
    if (typeof maybeCustom === 'function' &&
        // Filter out the util module, its inspect function is special.
        maybeCustom !== inspect &&
        // Also filter out any prototype objects using the circular check.
        ObjectGetOwnPropertyDescriptor(value, 'constructor')?.value?.prototype !== value) {
      // This makes sure the recurseTimes are reported as before while using
      // a counter internally.
      const depth = ctx.depth === null ? null : ctx.depth - recurseTimes;
      const isCrossContext =
        proxies !== 0 || !FunctionPrototypeSymbolHasInstance(Object, context);
      const ret = FunctionPrototypeCall(
        maybeCustom,
        context,
        depth,
        getUserOptions(ctx, isCrossContext),
        inspect,
      );
      // If the custom inspection method returned `this`, don't go into
      // infinite recursion.
      if (ret !== context) {
        if (typeof ret !== 'string') {
          return formatValue(ctx, ret, recurseTimes);
        }
        return StringPrototypeReplaceAll(ret, '\n', `\n${StringPrototypeRepeat(' ', ctx.indentationLvl)}`);
      }
    }
  }

  // Using an array here is actually better for the average case than using
  // a Set. `seen` will only check for the depth and will never grow too large.
  if (ctx.seen.includes(value)) {
    let index = 1;
    if (ctx.circular === undefined) {
      ctx.circular = new SafeMap();
      ctx.circular.set(value, index);
    } else {
      index = ctx.circular.get(value);
      if (index === undefined) {
        index = ctx.circular.size + 1;
        ctx.circular.set(value, index);
      }
    }
    return ctx.stylize(`[Circular *${index}]`, 'special');
  }

  let formatted = formatRaw(ctx, value, recurseTimes, typedArray);

  if (proxies !== 0) {
    for (let i = 0; i < proxies; i++) {
      formatted = `${ctx.stylize('Proxy(', 'special')}${formatted}${ctx.stylize(')', 'special')}`;
    }
  }

  return formatted;
}

module.exports = {
  init,
  formatValue,
  getConstructorName,
  getKeys,
  getCtxStyle,
  builtInObjects,
};
