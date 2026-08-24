// Shared Node host-compat prelude for jsse library-test bundles.
//
// This shim is prepended (via scripts/run-library-tests.sh) to an esbuild
// bundle so that a real-world npm library's own test runner — written against
// Node globals — can execute on jsse. It is a pure-JS shim: nothing here is
// baked into jsse's default global object, so test262 is unaffected.
//
// The readable-output layer (process, the full console method set, and the
// util.format / util.inspect core they share) is built on top of the flag-gated
// Rust host floor (issue #229): __host_write (byte-accurate fd I/O),
// __host_hrtime (monotonic clock), __host_exit (real process exit), and
// __host_proxy_target (trap-free Proxy metadata). The harness runs jsse with
// `--node` so those primitives exist; when they are absent (jsse without
// --node) each surface degrades to a pure-JS fallback.
//
// Everything below is skipped on real Node, where `process`, the full
// `console`, and `require('util')` already exist. That inertness is what lets
// `run-library-tests.sh --node` run the exact same bundle against Node as a
// reference oracle.

(function () {
  "use strict";

  // On Node, `process.versions.node` is set; the whole shim is a no-op there.
  var onNode =
    typeof process !== "undefined" &&
    !!(process.versions && process.versions.node);
  if (onNode) return;

  // The syscall floor (issue #229), present only under jsse `--node`.
  var hostWrite = typeof __host_write !== "undefined" ? __host_write : null;
  var hostHrtime = typeof __host_hrtime !== "undefined" ? __host_hrtime : null;
  var hostExit = typeof __host_exit !== "undefined" ? __host_exit : null;
  var hostProxyTarget =
    typeof __host_proxy_target !== "undefined" ? __host_proxy_target : null;
  // This metadata escape hatch exists only for the shim. Keep the captured
  // closure private so bundled library code cannot bypass Proxy handlers.
  if (hostProxyTarget) delete globalThis.__host_proxy_target;
  var fallbackConsoleLog = console.log;

  var NS_PER_SEC = 1000000000;

  // ---- util.inspect (best-effort) ------------------------------------------
  //
  // A readable, Node-flavoured rendering of arbitrary values for console.dir,
  // the %o/%O format specifiers, and console.log of non-strings. It is
  // deliberately NOT byte-compatible with Node's util.inspect (colour
  // heuristics, `<ref *N>` back-references, hidden keys, getters, Map/Set
  // internals — a bottomless pit); it only needs to be correct on depth,
  // cycles, and the common types.
  // `String.prototype.replace` with a RegExp dispatches through
  // `RegExp.prototype[@@replace]`, which reads `rx.exec` — a bundled library
  // that monkey-patches `RegExp.prototype.exec` would therefore run during a
  // diagnostic print. Split/join on a plain string separator is equivalent and
  // touches no user-replaceable hook.
  function replaceAll(s, needle, replacement) {
    // The needle is absent from most strings; skip the array allocation then.
    // `indexOf` with a string argument dispatches no user hook (only
    // `String.prototype.search` consults %Symbol.search%).
    if (stringIndexOf(s, needle) === -1) return s;
    return arrayJoin(stringSplit(s, needle), replacement);
  }

  function quoteString(s) {
    s = stringConstructor(s);
    // The overwhelmingly common string needs no escaping at all, and one
    // scan settles that; the replaceAll chain below scans three times
    // unconditionally. Non-global pattern, so `lastIndex` is untouched.
    if (regexpExec(escapableCharPattern, s) === null) return "'" + s + "'";
    return (
      "'" +
      replaceAll(
        replaceAll(replaceAll(s, "\\", "\\\\"), "'", "\\'"),
        "\n",
        "\\n"
      ) +
      "'"
    );
  }

  function isIdentifierKey(k) {
    return regexpExec(identifierKeyPattern, k) !== null;
  }

  // Capture uncurried intrinsics before bundled library code runs. Node's
  // formatter reads built-in internal slots rather than user-overridable
  // prototype methods.
  var functionCall = Function.prototype.call;
  var arrayConstructor = Array;
  var bigintConstructor = BigInt;
  var booleanConstructor = Boolean;
  var dateConstructor = Date;
  var errorConstructor = Error;
  var numberConstructor = Number;
  var objectConstructor = Object;
  var regexpConstructor = RegExp;
  var stringConstructor = String;
  var symbolConstructor = Symbol;
  var typeErrorConstructor = TypeError;
  var arrayIndexOf = functionCall.bind(arrayConstructor.prototype.indexOf);
  var arrayIsArray = arrayConstructor.isArray;
  var arrayJoin = functionCall.bind(arrayConstructor.prototype.join);
  var arrayPush = functionCall.bind(arrayConstructor.prototype.push);
  var objectGetOwnPropertyDescriptor =
    objectConstructor.getOwnPropertyDescriptor;
  var objectGetPrototypeOf = objectConstructor.getPrototypeOf;
  var objectHasOwnProperty = functionCall.bind(
    objectConstructor.prototype.hasOwnProperty
  );
  var objectIs = objectConstructor.is;
  var objectKeys = objectConstructor.keys;
  var objectPrototype = objectConstructor.prototype;
  var bigintPrototype = bigintConstructor.prototype;
  var booleanPrototype = booleanConstructor.prototype;
  var datePrototype = dateConstructor.prototype;
  var numberPrototype = numberConstructor.prototype;
  var stringPrototype = stringConstructor.prototype;
  var symbolPrototype = symbolConstructor.prototype;
  var numberIsNaN = numberConstructor.isNaN;
  var dateGetTime = functionCall.bind(dateConstructor.prototype.getTime);
  var dateToISOString = functionCall.bind(
    dateConstructor.prototype.toISOString
  );
  var errorIsError = errorConstructor.isError;
  var errorPrototype = errorConstructor.prototype;
  var regexpPrototype = regexpConstructor.prototype;

  // Captured defensively rather than as `descriptor.get`: on a host missing any
  // one of these accessors that read throws from inside this IIFE, before
  // `process` and `console` are installed, so a single absent flag would cost
  // every library test its diagnostic output instead of one rendered letter.
  function intrinsicGetter(target, key) {
    var desc = objectGetOwnPropertyDescriptor(target, key);
    if (!desc || isDataDescriptor(desc)) return null;
    return typeof desc.get === "function" ? functionCall.bind(desc.get) : null;
  }

  // Null when the host lacks the accessor: `tryApplyIntrinsic` then rejects the
  // slot probe and the value falls to the ordinary descriptor path.
  var regexpGetSource = intrinsicGetter(regexpPrototype, "source");

  // Node's flag order. A getter this host lacks contributes no letter.
  var REGEXP_FLAGS = [
    { letter: "d", get: intrinsicGetter(regexpPrototype, "hasIndices") },
    { letter: "g", get: intrinsicGetter(regexpPrototype, "global") },
    { letter: "i", get: intrinsicGetter(regexpPrototype, "ignoreCase") },
    { letter: "m", get: intrinsicGetter(regexpPrototype, "multiline") },
    { letter: "s", get: intrinsicGetter(regexpPrototype, "dotAll") },
    { letter: "u", get: intrinsicGetter(regexpPrototype, "unicode") },
    { letter: "v", get: intrinsicGetter(regexpPrototype, "unicodeSets") },
    { letter: "y", get: intrinsicGetter(regexpPrototype, "sticky") },
  ];
  var regexpExec = functionCall.bind(regexpConstructor.prototype.exec);
  var arrayPop = functionCall.bind(arrayConstructor.prototype.pop);
  var stringCharCodeAt = functionCall.bind(
    stringConstructor.prototype.charCodeAt
  );
  var stringSlice = functionCall.bind(stringConstructor.prototype.slice);
  var identifierKeyPattern = /^[A-Za-z_$][A-Za-z0-9_$]*$/;
  var escapableCharPattern = /[\\'\n]/;
  var circularMessagePattern = /circular/i;
  var numberValueOf = functionCall.bind(numberConstructor.prototype.valueOf);
  var stringValueOf = functionCall.bind(stringConstructor.prototype.valueOf);
  var booleanValueOf = functionCall.bind(
    booleanConstructor.prototype.valueOf
  );
  var bigintValueOf = functionCall.bind(bigintConstructor.prototype.valueOf);
  var symbolToString = functionCall.bind(
    symbolConstructor.prototype.toString
  );
  var stringSplit = functionCall.bind(stringConstructor.prototype.split);
  var stringIndexOf = functionCall.bind(stringConstructor.prototype.indexOf);

  // The wrapper families whose rendering is uniform: probe the internal slot,
  // classify the prototype chain, emit "[Label: text]". BigInt and Symbol stay
  // out on purpose — both gate their builtin rendering on a live @@hasInstance
  // check, and Symbol's probe already yields a string, so folding them in would
  // cost more variation flags than the table saves.
  var BOXED_WRAPPERS = [
    { label: "Number", probe: numberValueOf, prototype: numberPrototype },
    { label: "String", probe: stringValueOf, prototype: stringPrototype },
    { label: "Boolean", probe: booleanValueOf, prototype: booleanPrototype },
  ];

  function tryApplyIntrinsic(intrinsic, value) {
    try {
      return { value: intrinsic(value) };
    } catch (e) {
      // An object can inherit a built-in prototype without carrying the
      // corresponding internal slot. The intrinsic slot probe rejects it.
      return null;
    }
  }

  function tryInstanceOf(value, constructor) {
    try {
      return value instanceof constructor;
    } catch (e) {
      return false;
    }
  }

  function formatRegExp(value, source) {
    var flags = "";
    for (var i = 0; i < REGEXP_FLAGS.length; i++) {
      var entry = REGEXP_FLAGS[i];
      if (entry.get && entry.get(value)) flags += entry.letter;
    }
    return "/" + source + "/" + flags;
  }

  // Prototype chains are acyclic in practice, but a Proxy defeats
  // OrdinarySetPrototypeOf's cycle check, so every walk below is bounded.
  // Exhausting the bound always degrades to the walk's not-found result
  // ("ordinary" / null / false), never to a verdict read off whichever chain
  // node the last hop happened to land on. A chain deeper than this therefore
  // classifies as if it ended there, which diverges from Node; no finite bound
  // avoids that, and only a pathological chain reaches it.
  var MAX_PROTOTYPE_HOPS = 1000;

  // Sentinel for a [[Prototype]] read that threw — an exotic object in the
  // no-host fallback. Distinct from every value a real chain can hold.
  var UNKNOWN_PROTOTYPE = {};
  // The host hook reports a revoked Proxy with null, which must stay distinct
  // from a genuine null [[Prototype]]. The sentinel is private to this closure
  // and therefore cannot collide with any object in a rendered value's chain.
  var REVOKED_PROXY = {};

  // The unwrapped [[Prototype]] of `v`. Read once per value and threaded into
  // every builtinPrototypeKind() call below plus constructorName(), so
  // classifying eight built-in families and deriving the class-name prefix
  // costs one metadata read rather than nine — and, in the no-host fallback
  // where `v` may still be a Proxy, one getPrototypeOf trap.
  function prototypeOf(v) {
    try {
      return unwrapProxy(objectGetPrototypeOf(v));
    } catch (e) {
      return UNKNOWN_PROTOTYPE;
    }
  }

  // Classify a built-in's presentation from trap-free prototype metadata. A
  // genuine built-in can be reparented while retaining its internal slot, but
  // Node renders it generically unless its prototype chain still identifies
  // that built-in family. Slot-bearing prototype objects also recognize the
  // corresponding chain from another realm without invoking @@hasInstance.
  function builtinPrototypeKind(
    startPrototype,
    intrinsicPrototype,
    prototypeProbe
  ) {
    var current = startPrototype;
    if (current === UNKNOWN_PROTOTYPE) return "ordinary";
    if (current === REVOKED_PROXY) return "revoked";
    if (current === null) return "null";
    try {
      var hops = MAX_PROTOTYPE_HOPS;
      while (current !== null && current !== objectPrototype && hops-- > 0) {
        if (current === REVOKED_PROXY) return "revoked";
        if (
          current === intrinsicPrototype ||
          (prototypeProbe && tryApplyIntrinsic(prototypeProbe, current))
        ) {
          return "builtin";
        }
        current = unwrapProxy(objectGetPrototypeOf(current));
      }
    } catch (e) {
      // Exotic fallback: keep the value on the ordinary descriptor path.
    }
    return "ordinary";
  }

  // Walk a Proxy chain down to its non-Proxy target without dispatching to any
  // handler. Returns the value unchanged when it is not a Proxy (or when the
  // host floor is absent) and REVOKED_PROXY for a revoked Proxy.
  function unwrapProxy(v) {
    if (!hostProxyTarget) return v;
    // Unbounded on purpose, unlike every prototype walk below: a Proxy's
    // [[ProxyTarget]] is fixed at construction and can never be made to point
    // back at the Proxy, so a target chain is acyclic by construction and
    // finite in the number of live Proxies.
    while (true) {
      var target = hostProxyTarget(v);
      if (target === undefined) return v;
      if (target === null) return REVOKED_PROXY;
      v = target;
    }
  }

  // `desc` is always FromPropertyDescriptor output — an ordinary object whose
  // fields are own data properties — so an own-property check keeps a polluted
  // `Object.prototype.value` out of the result. That output is always either a
  // data descriptor (own `value`) or an accessor one (own `get`/`set`), never
  // both and never neither, so probing `value` alone discriminates the two.
  function isDataDescriptor(desc) {
    return !!desc && objectHasOwnProperty(desc, "value");
  }

  function dataDescriptorValue(desc) {
    return isDataDescriptor(desc) ? desc.value : undefined;
  }

  // The canonical safe read of this module: an own property's value, taken
  // from its data descriptor, so neither an accessor nor a Proxy get trap is
  // observed. `undefined` when the property is absent or accessor-valued.
  function ownDataValue(obj, key) {
    return dataDescriptorValue(objectGetOwnPropertyDescriptor(obj, key));
  }

  function findPropertyDescriptor(v, key) {
    try {
      var current = v;
      var hops = MAX_PROTOTYPE_HOPS;
      while (current !== null && hops-- > 0) {
        current = unwrapProxy(current);
        if (current === null || current === REVOKED_PROXY) return null;
        var desc = objectGetOwnPropertyDescriptor(current, key);
        if (desc) return desc;
        current = objectGetPrototypeOf(current);
      }
    } catch (e) {
      // The pure-JS no-host fallback cannot unwrap exotic objects. A failed
      // metadata walk degrades to the caller's default instead of escaping.
    }
    return null;
  }

  // A function's `name` read from its own data descriptor only, so neither an
  // accessor `name` nor a Proxy get trap is observed. Returns "" when absent.
  function functionName(fn) {
    var name = ownDataValue(fn, "name");
    return typeof name === "string" ? name : "";
  }

  function emptyItems(count) {
    return "<" + count + " empty item" + (count === 1 ? "" : "s") + ">";
  }

  function primitiveString(value, fallback) {
    var type = typeof value;
    if (
      value !== null &&
      (type === "object" || type === "function" || type === "undefined")
    ) {
      return fallback;
    }
    return type === "symbol" ? symbolToString(value) : stringConstructor(value);
  }

  function errorField(v, key, fallback) {
    return primitiveString(
      dataDescriptorValue(findPropertyDescriptor(v, key)),
      fallback
    );
  }

  function renderError(v) {
    // Test the raw descriptor value first. Stringifying a falsy primitive
    // such as 0 or false would otherwise turn it into a truthy string and
    // suppress the normal `[Error: message]` fallback.
    var stackValue = dataDescriptorValue(findPropertyDescriptor(v, "stack"));
    if (stackValue) {
      var stack = primitiveString(stackValue, "");
      if (stack) return stack;
    }

    var name = errorField(v, "name", "Error");
    var message = errorField(v, "message", "");
    var text = !name ? message : !message ? name : name + ": " + message;
    return "[" + text + "]";
  }

  function renderNullPrototypeError(v) {
    var name = errorField(v, "name", "Error") || "Error";
    var message = errorField(v, "message", "");
    return (
      "[" + name + ": null prototype]" + (message ? ": " + message : "")
    );
  }

  // Derive the "ClassName " prefix without a plain `v.constructor` get, which
  // would invoke an accessor `constructor` or a Proxy get-trap — Node reads
  // constructor metadata via the prototype chain, not by calling a getter. Use
  // data descriptors only, and treat any exotic-trap throw as "no prefix".
  // `prototype` is the caller's already-unwrapped [[Prototype]] of `v`, so no
  // second metadata read (and, on the no-host fallback, no second
  // getPrototypeOf trap) is needed here.
  function constructorName(v, prototype) {
    try {
      // An own `constructor` wins even when it is accessor-valued: the first
      // descriptor found is the one Node's walk would stop at, and
      // dataDescriptorValue then declines to call the getter.
      var desc = objectGetOwnPropertyDescriptor(v, "constructor");
      if (
        !desc &&
        prototype &&
        prototype !== REVOKED_PROXY &&
        prototype !== UNKNOWN_PROTOTYPE
      ) {
        desc = objectGetOwnPropertyDescriptor(prototype, "constructor");
      }
      // Node's getConstructorName only accepts a callable `constructor` with
      // a non-empty name; anything else yields no prefix at all (never a bare
      // leading space).
      var ctor = unwrapProxy(dataDescriptorValue(desc));
      if (typeof ctor !== "function") return "";
      // Node accepts a `constructor` only when the value is an INSTANCE of
      // it, and a constructor's own `prototype` object never is. Rejecting
      // that self-reference gives every intrinsic prototype Node's plain
      // `{}` rendering without enumerating them one by one.
      if (ownDataValue(ctor, "prototype") === v) return "";
      var name = functionName(ctor);
      return name && name !== "Object" ? name + " " : "";
    } catch (e) {
      return "";
    }
  }

  function inspect(value, opts) {
    opts = opts || {};
    var maxDepth = typeof opts.depth === "number" ? opts.depth : 2;
    var seen = [];

    function render(v, depth) {
      var t = typeof v;
      if (v === null) return "null";
      if (t === "undefined") return "undefined";
      if (t === "string") return quoteString(v);
      if (t === "number") return objectIs(v, -0) ? "-0" : stringConstructor(v);
      if (t === "bigint") return stringConstructor(v) + "n";
      if (t === "boolean") return stringConstructor(v);
      if (t === "symbol") return symbolToString(v);

      // Every primitive `typeof` has returned by now, so `v` is an object or
      // a function. Node's native inspector can read [[ProxyTarget]] without
      // dispatching through the handler. The --node host floor exposes that
      // one piece of metadata so the JS shim can do the same before any
      // instanceof, reflection, property access, or enumeration. Nested
      // Proxies are unwrapped recursively; a revoked Proxy is an opaque
      // terminal value.
      v = unwrapProxy(v);
      if (v === REVOKED_PROXY) return "<Revoked Proxy>";
      t = typeof v;

      if (t === "function") {
        var name = functionName(v);
        return "[Function" + (name ? ": " + name : " (anonymous)") + "]";
      }

      // Objects.
      if (arrayIndexOf(seen, v) !== -1) return "[Circular *1]";
      // Every builtinPrototypeKind() answer for a direct child of
      // %Object.prototype% is "ordinary" — its walk body never runs — so the
      // whole built-in classification below (seven throwing slot probes and
      // eight chain walks) is pure waste for the commonest object shape.
      var prototype = prototypeOf(v);
      // `Array.isArray` reads an internal slot and pierces a Proxy without
      // dispatching a trap, so this is free of user code. Hoisted above the
      // classification block, which it prunes, and reused by both call sites
      // below.
      var isArr = arrayIsArray(v);
      if (prototype !== objectPrototype) {
        // This first classification does double duty: "revoked" is a property
        // of the CHAIN, not of the Error family, and every walk below would
        // report it too — but only this one throws. It must therefore stay
        // ahead of them all, and outside the `isArr` guard, so an array whose
        // chain contains a revoked Proxy still throws.
        var errorKind = builtinPrototypeKind(prototype, errorPrototype, null);
        if (errorKind === "revoked") {
          throw new typeErrorConstructor(
            "Cannot perform 'get' on a proxy that has been revoked"
          );
        }
        if (errorKind === "builtin") return renderError(v);
        if (errorKind === "null" && errorIsError && errorIsError(v)) {
          return renderNullPrototypeError(v);
        }
      }

      // Internal-slot presentation. An Array exotic object is created by
      // ArrayCreate, never by OrdinaryCreateFromConstructor, so it cannot
      // carry [[RegExpMatcher]], [[DateValue]], [[NumberData]],
      // [[StringData]], [[BooleanData]], [[BigIntData]] or [[SymbolData]].
      // Every probe below would therefore throw and be caught — six
      // thrown-and-unwound TypeErrors per array, at every nesting level.
      // Skipping them for arrays is a zero-divergence prune; the Error
      // chain walk above is deliberately NOT skipped.
      if (!isArr && prototype !== objectPrototype) {
        var boxed;
        // `get RegExp.prototype.source` uniquely does NOT throw for
        // %RegExp.prototype% (it answers "(?:)"), so the slot probe alone would
        // misreport the prototype itself as a RegExp. Only reachable when
        // library code reparents %RegExp.prototype% — an unreparented one is a
        // direct child of %Object.prototype% and never enters this block at
        // all. After detecting a genuine
        // RegExp, classify its prototype chain before presentation: a reparented
        // RegExp must not dispatch ordinary `source`/`flags` gets through a user
        // object. Compose the standard rendering from captured slot getters.
        boxed =
          v === regexpPrototype
            ? null
            : tryApplyIntrinsic(regexpGetSource, v);
        if (boxed) {
          var regexpKind = builtinPrototypeKind(
            prototype,
            regexpPrototype,
            regexpGetSource
          );
          if (regexpKind === "builtin") return formatRegExp(v, boxed.value);
          if (regexpKind === "null") {
            return "[RegExp: null prototype] " + formatRegExp(v, boxed.value);
          }
        }
        boxed = tryApplyIntrinsic(dateGetTime, v);
        if (boxed) {
          var dateText = numberIsNaN(boxed.value)
            ? "Invalid Date"
            : dateToISOString(v);
          var dateKind = builtinPrototypeKind(
            prototype,
            datePrototype,
            dateGetTime
          );
          if (dateKind === "builtin") return dateText;
          if (dateKind === "null") {
            return "[Date: null prototype] " + dateText;
          }
        }
        for (var w = 0; w < BOXED_WRAPPERS.length; w++) {
          var wrapper = BOXED_WRAPPERS[w];
          boxed = tryApplyIntrinsic(wrapper.probe, v);
          if (!boxed) continue;
          var wrapperText = render(boxed.value, depth);
          var wrapperKind = builtinPrototypeKind(
            prototype,
            wrapper.prototype,
            wrapper.probe
          );
          if (wrapperKind === "builtin") {
            return "[" + wrapper.label + ": " + wrapperText + "]";
          }
          if (wrapperKind === "null") {
            return (
              "[" + wrapper.label + " (null prototype): " + wrapperText + "]"
            );
          }
        }
        boxed = tryApplyIntrinsic(bigintValueOf, v);
        if (boxed) {
          var bigintText = render(boxed.value, depth);
          var bigintKind = builtinPrototypeKind(
            prototype,
            bigintPrototype,
            bigintValueOf
          );
          if (bigintKind === "null") {
            return "[BigInt (null prototype): " + bigintText + "]";
          }
          if (bigintKind === "builtin") {
            // Unlike the wrappers above, Node's boxed BigInt/Symbol rendering
            // intentionally observes a constructor's current @@hasInstance
            // result. A false or throwing hook selects its generic object
            // shape, but must not intercept the internal-slot probe.
            return tryInstanceOf(v, bigintConstructor)
              ? "[BigInt: " + bigintText + "]"
              : "Object [BigInt] {}";
          }
        }
        boxed = tryApplyIntrinsic(symbolToString, v);
        if (boxed) {
          var symbolKind = builtinPrototypeKind(
            prototype,
            symbolPrototype,
            symbolToString
          );
          if (symbolKind === "null") {
            return "[Symbol (null prototype): " + boxed.value + "]";
          }
          if (symbolKind === "builtin") {
            return tryInstanceOf(v, symbolConstructor)
              ? "[Symbol: " + boxed.value + "]"
              : "Object [Symbol] {}";
          }
        }
      }

      if (depth < 0) return isArr ? "[Array]" : "[Object]";

      arrayPush(seen, v);
      var out;
      try {
        if (isArr) {
          out = renderArray(v, depth);
        } else {
          // Own properties are rendered from their descriptors WITHOUT
          // invoking accessors — Node's util.inspect shows [Getter]/[Setter]
          // rather than calling the getter, so a throwing or side-effecting
          // accessor cannot make a diagnostic print throw/mutate under jsse
          // where it would not under Node.
          var keys = objectKeys(v);
          var parts = [];
          for (var j = 0; j < keys.length; j++) {
            var k = keys[j];
            var label = isIdentifierKey(k) ? k : quoteString(k);
            var memberDesc = objectGetOwnPropertyDescriptor(v, k);
            arrayPush(parts, label + ": " + renderDescriptor(memberDesc, depth));
          }
          var ctorName = constructorName(v, prototype);
          out = parts.length
            ? ctorName + "{ " + arrayJoin(parts, ", ") + " }"
            : ctorName + "{}";
        }
      } finally {
        arrayPop(seen);
      }
      return out;
    }

    function renderDescriptor(desc, depth) {
      // Not a data descriptor => an accessor one, whose `get`/`set` are own
      // data properties that are undefined-or-callable. Both may be undefined
      // (`defineProperty(o, k, { get: undefined, set: undefined })`); Node
      // renders that as the absent value, so it must fall through.
      if (desc && !isDataDescriptor(desc)) {
        if (desc.get) return desc.set ? "[Getter/Setter]" : "[Getter]";
        if (desc.set) return "[Setter]";
        // Both undefined: fall through to render the absent value, as Node
        // does.
      }
      return render(dataDescriptorValue(desc), depth - 1);
    }

    // Array length and elements are read from own descriptors on the unwrapped
    // target. A missing descriptor is a hole, never an invitation to read
    // through Array.prototype (which could invoke an inherited getter).
    // Probing every index is O(length); the O(elements) own-index-key form is
    // blocked on jsse#516 (Object.getOwnPropertyNames/Reflect.ownKeys omit
    // index keys assigned after array creation, so every element of a
    // push-built array would render as a hole).
    function renderArray(v, depth) {
      var length = ownDataValue(v, "length");
      if (typeof length !== "number" || length <= 0) return "[]";

      // Collect into a local array and join once. Accumulating with `+=`
      // instead is superlinear here: JsString is a flat buffer with no rope
      // representation, so each concat copies the whole prefix.
      var parts = [];
      var holes = 0;

      for (var i = 0; i < length; i++) {
        var desc = objectGetOwnPropertyDescriptor(v, i);
        if (!desc) {
          holes++;
          continue;
        }
        if (holes) {
          arrayPush(parts, emptyItems(holes));
          holes = 0;
        }
        arrayPush(parts, renderDescriptor(desc, depth));
      }
      if (holes) arrayPush(parts, emptyItems(holes));
      // `length > 0` guarantees the loop ran, so every index became either an
      // element or a hole, and `parts` cannot be empty.
      return "[ " + arrayJoin(parts, ", ") + " ]";
    }

    return render(value, maxDepth);
  }

  // ---- util.format ----------------------------------------------------------
  //
  // Node's printf-style formatter. The %s %d %i %f %j %c %% specifiers are
  // deterministic and matched exactly; %o/%O defer to the best-effort inspect.
  // Node creates this set from globalThis while internal/util/inspect is
  // bootstrapping. By the time user code runs, Node and jsse have both added
  // more globals, but those late names are deliberately absent from Node's
  // classifier. Keep the Node 26.5.0 bootstrap membership explicit so jsse-only
  // globals (for example ShadowRealm) cannot change %s dispatch.
  var builtInObjectNames = (function () {
    var names = Object.create(null);
    var nodeBootstrapNames = [
      "Object",
      "Function",
      "Array",
      "Number",
      "Infinity",
      "NaN",
      "Boolean",
      "String",
      "Symbol",
      "Date",
      "Promise",
      "RegExp",
      "Error",
      "AggregateError",
      "EvalError",
      "RangeError",
      "ReferenceError",
      "SyntaxError",
      "TypeError",
      "URIError",
      "JSON",
      "Math",
      "Intl",
      "ArrayBuffer",
      "Atomics",
      "Uint8Array",
      "Int8Array",
      "Uint16Array",
      "Int16Array",
      "Uint32Array",
      "Int32Array",
      "BigUint64Array",
      "BigInt64Array",
      "Uint8ClampedArray",
      "Float32Array",
      "Float64Array",
      "DataView",
      "Map",
      "BigInt",
      "Set",
      "Iterator",
      "WeakMap",
      "WeakSet",
      "Proxy",
      "Reflect",
      "FinalizationRegistry",
      "WeakRef",
    ];
    for (var i = 0; i < nodeBootstrapNames.length; i++) {
      names[nodeBootstrapNames[i]] = true;
    }
    return names;
  })();
  var symbolToPrimitive = symbolConstructor.toPrimitive;

  function returnFalse() {
    return false;
  }

  // Match Node's hasBuiltInToString classification. A bundled library's
  // prototype method is user-defined even when inherited, while coercion hooks
  // owned by a built-in prototype route through inspect.
  function hasBuiltInToString(value) {
    var hasOwnToString = objectHasOwnProperty;
    var hasOwnToPrimitive = objectHasOwnProperty;

    // Node reads [[ProxyTarget]] here before touching a single property, so
    // classification never dispatches a Proxy get trap. Do the same via the
    // host floor; a revoked Proxy has no coercion hook at all, so it routes to
    // inspect. (When classification answers "user-defined", `convS` still
    // coerces the ORIGINAL value, so the trap runs there — on Node too.)
    value = unwrapProxy(value);
    if (value === REVOKED_PROXY) return true;

    if (typeof value.toString !== "function") {
      if (typeof value[symbolToPrimitive] !== "function") return true;
      if (objectHasOwnProperty(value, symbolToPrimitive)) return false;
      hasOwnToString = returnFalse;
    } else if (objectHasOwnProperty(value, "toString")) {
      return false;
    } else if (typeof value[symbolToPrimitive] !== "function") {
      hasOwnToPrimitive = returnFalse;
    } else if (objectHasOwnProperty(value, symbolToPrimitive)) {
      return false;
    }

    // Unwrap at every hop, not just at the top: a Proxy anywhere in the chain
    // would otherwise run its getPrototypeOf/getOwnPropertyDescriptor traps
    // during a diagnostic print. This is a deliberate divergence — Node *would*
    // fire the trap here — so a `--node` cross-check difference on a
    // prototype-chain Proxy is expected, not a regression.
    var pointer = value;
    var hops = MAX_PROTOTYPE_HOPS;
    try {
      do {
        // Exhausting the bound means the owner was not found, exactly as
        // reaching a null [[Prototype]] does; falling out of the loop with a
        // non-null pointer would instead classify off an arbitrary chain node.
        if (hops-- <= 0) return false;
        pointer = unwrapProxy(objectGetPrototypeOf(pointer));
        if (pointer === REVOKED_PROXY) return false;
      } while (
        pointer !== null &&
        !hasOwnToString(pointer, "toString") &&
        !hasOwnToPrimitive(pointer, symbolToPrimitive)
      );
    } catch (e) {
      // Without the host floor the shim cannot unwrap an exotic object, so a
      // failed owner walk falls back to ordinary coercion.
      return false;
    }

    // A callable hook with no owner in the reported prototype chain (only
    // reachable on the no-host fallback, where a Proxy stays opaque) is treated
    // as user-defined.
    if (pointer === null) return false;

    // Deliberately an ordinary `.value`/`.name` read, NOT the descriptor
    // helpers used elsewhere: routing here decides whether `%s` reaches
    // inspect at all, and tightening it to data-descriptor lookups would
    // change which values Node-compatibly route there. See the design doc.
    var descriptor = objectGetOwnPropertyDescriptor(pointer, "constructor");
    return (
      descriptor !== undefined &&
      typeof descriptor.value === "function" &&
      builtInObjectNames[descriptor.value.name] === true
    );
  }

  function convS(v) {
    var t = typeof v;
    if (t === "string") return v;
    if (t === "bigint") return String(v) + "n";
    if (t === "number") return Object.is(v, -0) ? "-0" : String(v);
    if (v === null) return "null";
    if (t === "undefined") return "undefined";
    if (t === "boolean") return String(v);
    if (t === "symbol") return symbolToString(v);
    if (t === "function") return inspect(v, { depth: 0 });
    return hasBuiltInToString(v) ? inspect(v, { depth: 0 }) : String(v);
  }

  function convD(v) {
    var t = typeof v;
    if (t === "bigint") return String(v) + "n";
    if (t === "symbol") return "NaN";
    return String(Number(v));
  }

  function convI(v) {
    var t = typeof v;
    if (t === "bigint") return String(v) + "n";
    if (t === "symbol") return "NaN";
    return String(parseInt(v, 10));
  }

  function convF(v) {
    if (typeof v === "symbol") return "NaN";
    return String(parseFloat(v));
  }

  function convJ(v) {
    try {
      var s = JSON.stringify(v);
      return s === undefined ? "undefined" : s;
    } catch (e) {
      // Node's %j suppresses ONLY circular-structure failures (returning
      // "[Circular]") and re-throws everything else — BigInt, and user
      // toJSON/getter exceptions. jsse's circular error is
      // "Converting circular structure to JSON"; its BigInt/toJSON errors do not
      // mention "circular", so matching the message is safe here (the shim is
      // inert on Node, so this only ever sees jsse's error text).
      if (e && regexpExec(circularMessagePattern, stringConstructor(e.message)))
        return "[Circular]";
      throw e;
    }
  }

  function format() {
    var args = arguments;
    var first = args[0];
    if (typeof first !== "string") {
      // No format string: inspect every argument, join with a space.
      var pieces = [];
      for (var i = 0; i < args.length; i++) {
        arrayPush(
          pieces,
          typeof args[i] === "string" ? args[i] : inspect(args[i])
        );
      }
      return arrayJoin(pieces, " ");
    }
    // A lone string argument is returned verbatim — Node performs no specifier
    // substitution unless there is at least one argument to format, so e.g.
    // format("%%") is "%%" and format("%s") is "%s", but format("%%", x)
    // substitutes.
    if (args.length === 1) return first;

    var out = "";
    var lastPos = 0;
    var argIndex = 1;
    var f = first;
    var n = f.length;
    for (var p = 0; p < n - 1; p++) {
      if (stringCharCodeAt(f, p) !== 37 /* % */) continue;
      var next = stringCharCodeAt(f, p + 1);
      if (next === 37 /* %% */) {
        out += stringSlice(f, lastPos, p) + "%";
        lastPos = p + 2;
        p++;
        continue;
      }
      // Specifiers that consume an argument only fire while one remains.
      var repl = null;
      if (argIndex < args.length) {
        switch (next) {
          case 115: repl = convS(args[argIndex++]); break; // s
          case 100: repl = convD(args[argIndex++]); break; // d
          case 105: repl = convI(args[argIndex++]); break; // i
          case 102: repl = convF(args[argIndex++]); break; // f
          case 106: repl = convJ(args[argIndex++]); break; // j
          case 111: repl = inspect(args[argIndex++], { depth: 4 }); break; // o
          case 79: repl = inspect(args[argIndex++], {}); break; // O
          case 99: argIndex++; repl = ""; break; // c (CSS ignored)
        }
      }
      if (repl !== null) {
        out += stringSlice(f, lastPos, p) + repl;
        lastPos = p + 2;
        p++;
      }
    }
    out += stringSlice(f, lastPos);

    // Trailing arguments beyond the specifiers are appended, space-separated.
    for (; argIndex < args.length; argIndex++) {
      var extra = args[argIndex];
      out += " " + (typeof extra === "string" ? extra : inspect(extra));
    }
    return out;
  }

  globalThis.util = {
    format: format,
    formatWithOptions: function (opts, first) {
      return format.apply(null, Array.prototype.slice.call(arguments, 1));
    },
    inspect: inspect,
  };

  // ---- process --------------------------------------------------------------
  function makeStream(fd) {
    if (hostWrite) {
      return {
        fd: fd,
        isTTY: false,
        write: function (chunk, encodingOrCb, cb) {
          hostWrite(fd, String(chunk));
          var callback = typeof encodingOrCb === "function" ? encodingOrCb : cb;
          if (typeof callback === "function") callback();
          return true;
        },
        _flush: function () {},
      };
    }
    // Fallback: jsse without the syscall floor only exposes newline-appending
    // console.log, so accumulate partial writes and emit one line at a time.
    // Use the original native log because this shim replaces console.log below.
    var buf = "";
    return {
      fd: fd,
      isTTY: false,
      write: function (chunk, encodingOrCb, cb) {
        buf += String(chunk);
        var idx;
        while ((idx = buf.indexOf("\n")) !== -1) {
          fallbackConsoleLog.call(console, buf.slice(0, idx));
          buf = buf.slice(idx + 1);
        }
        var callback = typeof encodingOrCb === "function" ? encodingOrCb : cb;
        if (typeof callback === "function") callback();
        return true;
      },
      _flush: function () {
        if (buf.length) {
          fallbackConsoleLog.call(console, buf);
          buf = "";
        }
      },
    };
  }

  var stdout = makeStream(1);
  var stderr = makeStream(2);
  var hrtimeFn;

  function makeHrtime() {
    var hr;
    if (hostHrtime) {
      hr = function (prev) {
        var now = hostHrtime(); // BigInt nanoseconds, monotonic
        if (prev) {
          var prevNs =
            BigInt(prev[0]) * BigInt(NS_PER_SEC) + BigInt(prev[1]);
          var delta = now - prevNs;
          return [
            Number(delta / BigInt(NS_PER_SEC)),
            Number(delta % BigInt(NS_PER_SEC)),
          ];
        }
        return [
          Number(now / BigInt(NS_PER_SEC)),
          Number(now % BigInt(NS_PER_SEC)),
        ];
      };
      hr.bigint = function () {
        return hostHrtime();
      };
    } else {
      hr = function (prev) {
        var ms = Date.now();
        var s = Math.floor(ms / 1000);
        var ns = (ms % 1000) * 1e6;
        if (prev) {
          var ds = s - prev[0];
          var dns = ns - prev[1];
          if (dns < 0) {
            ds -= 1;
            dns += NS_PER_SEC;
          }
          return [ds, dns];
        }
        return [s, ns];
      };
      hr.bigint = function () {
        return BigInt(Math.floor(Date.now() * 1e6));
      };
    }
    return hr;
  }

  hrtimeFn = makeHrtime();

  globalThis.process = {
    argv: ["node", "/bundle.js"],
    argv0: "node",
    execPath: "/usr/bin/node",
    env: {},
    pid: 1,
    ppid: 0,
    platform: "linux",
    arch: "x64",
    version: "v20.0.0",
    versions: { node: "20.0.0" },
    cwd: function () {
      return "/";
    },
    // Node's nextTick runs before Promise microtasks, but jsse has no separate
    // tick queue; a microtask is close enough for library test runners.
    nextTick: function (cb) {
      var extra = Array.prototype.slice.call(arguments, 1);
      Promise.resolve().then(function () {
        cb.apply(undefined, extra);
      });
    },
    stdout: stdout,
    stderr: stderr,
    hrtime: hrtimeFn,
    exit: function (code) {
      code = code ? code | 0 : 0;
      if (hostExit) {
        hostExit(code); // real, uncatchable exit (issue #242)
        return;
      }
      // Fallback: flush buffered output, then let a non-zero code surface as a
      // throw the harness can see.
      stdout._flush();
      stderr._flush();
      if (code) throw new Error("process.exit(" + code + ")");
    },
    on: function () {
      return globalThis.process;
    },
    once: function () {
      return globalThis.process;
    },
    emit: function () {
      return false;
    },
  };

  // ---- console --------------------------------------------------------------
  var groupIndent = "";

  function writeLine(stream, args) {
    var line = format.apply(null, args);
    if (groupIndent) {
      line = groupIndent + replaceAll(line, "\n", "\n" + groupIndent);
    }
    stream.write(line + "\n");
  }

  var counts = {};
  var timers = {};

  function timerNow() {
    return hrtimeFn.bigint();
  }

  var jsseConsole = {
    log: function () {
      writeLine(stdout, arguments);
    },
    info: function () {
      writeLine(stdout, arguments);
    },
    debug: function () {
      writeLine(stdout, arguments);
    },
    error: function () {
      writeLine(stderr, arguments);
    },
    warn: function () {
      writeLine(stderr, arguments);
    },
    dir: function (obj, opts) {
      stdout.write((groupIndent || "") + inspect(obj, opts || {}) + "\n");
    },
    trace: function () {
      var msg = format.apply(null, arguments);
      var stack = new Error().stack || "";
      stderr.write("Trace" + (msg ? ": " + msg : "") + "\n" + stack + "\n");
    },
    assert: function (cond) {
      if (cond) return;
      var rest = Array.prototype.slice.call(arguments, 1);
      var msg = rest.length ? ": " + format.apply(null, rest) : "";
      stderr.write("Assertion failed" + msg + "\n");
    },
    group: function () {
      if (arguments.length) writeLine(stdout, arguments);
      groupIndent += "  ";
    },
    groupCollapsed: function () {
      if (arguments.length) writeLine(stdout, arguments);
      groupIndent += "  ";
    },
    groupEnd: function () {
      groupIndent = groupIndent.slice(0, groupIndent.length - 2);
    },
    count: function (label) {
      label = label === undefined ? "default" : String(label);
      counts[label] = (counts[label] || 0) + 1;
      jsseConsole.log(label + ": " + counts[label]);
    },
    countReset: function (label) {
      label = label === undefined ? "default" : String(label);
      counts[label] = 0;
    },
    time: function (label) {
      label = label === undefined ? "default" : String(label);
      timers[label] = timerNow();
    },
    timeEnd: function (label) {
      label = label === undefined ? "default" : String(label);
      if (!(label in timers)) {
        jsseConsole.warn("Warning: No such label '" + label + "'");
        return;
      }
      var ms = Number(timerNow() - timers[label]) / 1e6;
      delete timers[label];
      jsseConsole.log(label + ": " + ms + "ms");
    },
    timeLog: function (label) {
      label = label === undefined ? "default" : String(label);
      if (!(label in timers)) {
        jsseConsole.warn("Warning: No such label '" + label + "'");
        return;
      }
      var ms = Number(timerNow() - timers[label]) / 1e6;
      var rest = Array.prototype.slice.call(arguments, 1);
      var extra = rest.length ? " " + format.apply(null, rest) : "";
      jsseConsole.log(label + ": " + ms + "ms" + extra);
    },
    // Best-effort: Node renders an ASCII table; a readable inspect dump is
    // close enough for the test runners that call it.
    table: function (data) {
      jsseConsole.dir(data, { depth: null });
    },
  };

  // jsse binds `console` as a lexical const (not a plain global-object
  // property), so a `globalThis.console = …` reassignment would be shadowed by
  // bare `console` references in the bundle. Mutate the existing object instead:
  // its native `log` is writable/configurable and the object is extensible, so
  // overriding `log` and adding the rest of the method set takes effect for
  // both `console.x` and bare `console` uses.
  for (var method in jsseConsole) {
    if (Object.prototype.hasOwnProperty.call(jsseConsole, method)) {
      console[method] = jsseConsole[method];
    }
  }
})();
