# Phase 9: Built-in Objects

**Spec Reference:** §19–28

## Goal
Implement all standard built-in objects, constructors, prototypes, and their methods.

Ordered roughly by dependency and importance.

## Tasks

### 9.1 Global Object (§19)
- [ ] Global value properties: `Infinity`, `NaN`, `undefined`, `globalThis`
- [ ] `eval()` (direct and indirect)
- [ ] `isFinite()`
- [ ] `isNaN()`
- [ ] `parseFloat()`
- [ ] `parseInt()`
- [ ] `decodeURI()` / `decodeURIComponent()`
- [ ] `encodeURI()` / `encodeURIComponent()`

**Tests:** `built-ins/global/` (29), `built-ins/eval/` (10), `built-ins/isFinite/` (15), `built-ins/isNaN/` (15), `built-ins/parseFloat/` (54), `built-ins/parseInt/` (55), `built-ins/decodeURI/` (55), `built-ins/decodeURIComponent/` (56), `built-ins/encodeURI/` (31), `built-ins/encodeURIComponent/` (31), `built-ins/Infinity/` (6), `built-ins/NaN/` (6), `built-ins/undefined/` (8)

### 9.2 Fundamental Objects (§20)
- [ ] **Object** (§20.1)
  - [ ] `Object()` constructor
  - [ ] `Object.assign()`, `Object.create()`, `Object.defineProperty()`, `Object.defineProperties()`
  - [ ] `Object.entries()`, `Object.fromEntries()`
  - [ ] `Object.freeze()`, `Object.isFrozen()`
  - [ ] `Object.getOwnPropertyDescriptor()`, `Object.getOwnPropertyDescriptors()`
  - [ ] `Object.getOwnPropertyNames()`, `Object.getOwnPropertySymbols()`
  - [ ] `Object.getPrototypeOf()`, `Object.setPrototypeOf()`
  - [ ] `Object.groupBy()`
  - [ ] `Object.hasOwn()`
  - [ ] `Object.is()`
  - [ ] `Object.isExtensible()`, `Object.preventExtensions()`
  - [ ] `Object.keys()`, `Object.values()`
  - [ ] `Object.seal()`, `Object.isSealed()`
  - [ ] `Object.prototype.hasOwnProperty()`, `Object.prototype.isPrototypeOf()`
  - [ ] `Object.prototype.propertyIsEnumerable()`
  - [ ] `Object.prototype.toLocaleString()`, `Object.prototype.toString()`, `Object.prototype.valueOf()`
  - [ ] `Object.prototype.__proto__` (Annex B)
  - [ ] `Object.prototype.__defineGetter__`, `__defineSetter__`, `__lookupGetter__`, `__lookupSetter__` (Annex B)
- [ ] **Function** (§20.2)
  - [ ] `Function()` constructor (dynamic function creation)
  - [ ] `Function.prototype.apply()`, `.bind()`, `.call()`
  - [ ] `Function.prototype.toString()`
  - [ ] `Function.prototype[@@hasInstance]`
  - [ ] `Function.prototype.constructor`
  - [ ] `name`, `length` properties
- [ ] **Boolean** (§20.3)
  - [ ] `Boolean()` constructor
  - [ ] `Boolean.prototype.toString()`, `.valueOf()`
- [ ] **Symbol** (§20.4)
  - [ ] `Symbol()` constructor (not newable)
  - [ ] `Symbol.for()`, `Symbol.keyFor()`
  - [ ] `Symbol.prototype.toString()`, `.valueOf()`, `.description`
  - [ ] `Symbol.prototype[@@toPrimitive]`, `[@@toStringTag]`
  - [ ] All well-known symbols as static properties
- [ ] **Error** objects (§20.5)
  - [ ] `Error()`, `Error.prototype.message`, `.name`, `.toString()`
  - [ ] `Error.isError()`
  - [ ] `Error.prototype.stack` (implementation-defined)
  - [ ] Native error types: `EvalError`, `RangeError`, `ReferenceError`, `SyntaxError`, `TypeError`, `URIError`
  - [ ] `AggregateError`
  - [ ] `SuppressedError`

**Tests:** `built-ins/Object/` (3,411), `built-ins/Function/` (509), `built-ins/Boolean/` (51), `built-ins/Symbol/` (94), `built-ins/Error/` (53), `built-ins/NativeErrors/` (92), `built-ins/AggregateError/` (25), `built-ins/SuppressedError/` (22)

### 9.3 Numbers & Dates (§21)
- [ ] **Number** (§21.1)
  - [ ] `Number()` constructor
  - [ ] `Number.isFinite()`, `.isInteger()`, `.isNaN()`, `.isSafeInteger()`
  - [ ] `Number.MAX_SAFE_INTEGER`, `.MIN_SAFE_INTEGER`, `.MAX_VALUE`, `.MIN_VALUE`, `.EPSILON`, `.NaN`, `.POSITIVE_INFINITY`, `.NEGATIVE_INFINITY`
  - [ ] `Number.parseFloat()`, `.parseInt()`
  - [ ] `Number.prototype.toExponential()`, `.toFixed()`, `.toPrecision()`, `.toString()`, `.valueOf()`, `.toLocaleString()`
- [ ] **BigInt** (§21.2)
  - [ ] `BigInt()` function (not constructor)
  - [ ] `BigInt.asIntN()`, `BigInt.asUintN()`
  - [ ] `BigInt.prototype.toString()`, `.valueOf()`, `.toLocaleString()`
- [ ] **Math** (§21.3)
  - [ ] All constants: `E`, `LN10`, `LN2`, `LOG10E`, `LOG2E`, `PI`, `SQRT1_2`, `SQRT2`
  - [ ] All methods: `abs`, `acos`, `acosh`, `asin`, `asinh`, `atan`, `atanh`, `atan2`, `cbrt`, `ceil`, `clz32`, `cos`, `cosh`, `exp`, `expm1`, `floor`, `fround`, `f16round`, `hypot`, `imul`, `log`, `log1p`, `log10`, `log2`, `max`, `min`, `pow`, `random`, `round`, `sign`, `sin`, `sinh`, `sqrt`, `sumPrecise`, `tan`, `tanh`, `trunc`
  - [ ] `Math[@@toStringTag]` = `"Math"`
- [ ] **Date** (§21.4)
  - [ ] `Date()` constructor (multiple overloads)
  - [ ] `Date.now()`, `Date.parse()`, `Date.UTC()`
  - [ ] All prototype get/set methods (getFullYear, getMonth, getDate, getHours, getMinutes, getSeconds, getMilliseconds, getDay, getTime, getTimezoneOffset, and all `setX`/`getUTCX` variants)
  - [ ] `Date.prototype.toDateString()`, `.toTimeString()`, `.toISOString()`, `.toJSON()`, `.toLocaleDateString()`, `.toLocaleString()`, `.toLocaleTimeString()`, `.toString()`, `.toUTCString()`
  - [ ] `Date.prototype.valueOf()`, `[@@toPrimitive]`

**Tests:** `built-ins/Number/` (335), `built-ins/BigInt/` (75), `built-ins/Math/` (327), `built-ins/Date/` (594)

### 9.4 Text Processing (§22)
- [ ] **String** (§22.1)
  - [ ] `String()` constructor
  - [ ] `String.fromCharCode()`, `String.fromCodePoint()`, `String.raw()`
  - [ ] `String.prototype` methods: `at`, `charAt`, `charCodeAt`, `codePointAt`, `concat`, `endsWith`, `includes`, `indexOf`, `isWellFormed`, `lastIndexOf`, `localeCompare`, `match`, `matchAll`, `normalize`, `padEnd`, `padStart`, `repeat`, `replace`, `replaceAll`, `search`, `slice`, `split`, `startsWith`, `substring`, `toLocaleLowerCase`, `toLocaleUpperCase`, `toLowerCase`, `toString`, `toUpperCase`, `toWellFormed`, `trim`, `trimEnd`, `trimStart`, `valueOf`
  - [ ] `String.prototype[@@iterator]`
  - [ ] String HTML methods (Annex B): `anchor`, `big`, `blink`, `bold`, `fixed`, `fontcolor`, `fontsize`, `italics`, `link`, `small`, `strike`, `sub`, `sup`
- [ ] **RegExp** (§22.2)
  - [ ] `RegExp()` constructor
  - [ ] `RegExp.prototype.exec()`
  - [ ] `RegExp.prototype.test()`
  - [ ] `RegExp.prototype.toString()`
  - [ ] `RegExp.prototype[@@match]`, `[@@matchAll]`, `[@@replace]`, `[@@search]`, `[@@split]`
  - [ ] All flags: `d`, `g`, `i`, `m`, `s`, `u`, `v`, `y`
  - [ ] Flag properties: `dotAll`, `global`, `hasIndices`, `ignoreCase`, `multiline`, `source`, `flags`, `sticky`, `unicode`, `unicodeSets`
  - [ ] Named capture groups
  - [ ] Lookbehind assertions
  - [ ] Unicode property escapes
  - [ ] Set notation (`v` flag)
  - [ ] `lastIndex` handling
  - [ ] `RegExp.$1`–`$9` and legacy features (Annex B)

**Tests:** `built-ins/String/` (1,215), `built-ins/RegExp/` (1,879), `built-ins/StringIteratorPrototype/` (7), `built-ins/RegExpStringIteratorPrototype/` (17)

### 9.5 Indexed Collections (§23)
- [ ] **Array** (§23.1)
  - [ ] `Array()` constructor
  - [ ] `Array.from()`, `Array.isArray()`, `Array.of()`
  - [ ] `Array.prototype` methods: `at`, `concat`, `copyWithin`, `entries`, `every`, `fill`, `filter`, `find`, `findIndex`, `findLast`, `findLastIndex`, `flat`, `flatMap`, `forEach`, `includes`, `indexOf`, `join`, `keys`, `lastIndexOf`, `map`, `pop`, `push`, `reduce`, `reduceRight`, `reverse`, `shift`, `slice`, `some`, `sort`, `splice`, `toLocaleString`, `toReversed`, `toSorted`, `toSpliced`, `toString`, `unshift`, `values`, `with`
  - [ ] `Array.prototype[@@iterator]`, `[@@unscopables]`
  - [ ] Array length semantics
  - [ ] Array species (`@@species`)
  - [ ] Array groupBy
- [ ] **TypedArray** (§23.2)
  - [ ] `%TypedArray%` intrinsic (abstract base)
  - [ ] All concrete constructors: `Int8Array`, `Uint8Array`, `Uint8ClampedArray`, `Int16Array`, `Uint16Array`, `Int32Array`, `Uint32Array`, `Float16Array`, `Float32Array`, `Float64Array`, `BigInt64Array`, `BigUint64Array`
  - [ ] `TypedArray.from()`, `TypedArray.of()`
  - [ ] All prototype methods (similar to Array but with typed semantics)
  - [ ] Buffer, byteOffset, byteLength, length
  - [ ] `Uint8Array.fromBase64()`, `Uint8Array.fromHex()`, `.toBase64()`, `.toHex()`, `.setFromBase64()`, `.setFromHex()`

**Tests:** `built-ins/Array/` (3,079), `built-ins/ArrayIteratorPrototype/` (27), `built-ins/TypedArray/` (1,438), `built-ins/TypedArrayConstructors/` (736), `built-ins/Uint8Array/` (68)

### 9.6 Keyed Collections (§24)
- [ ] **Map** (§24.1)
  - [ ] `Map()` constructor
  - [ ] `Map.prototype`: `clear`, `delete`, `entries`, `forEach`, `get`, `has`, `keys`, `set`, `size`, `values`, `[@@iterator]`, `[@@toStringTag]`
  - [ ] `Map.groupBy()`
- [ ] **Set** (§24.2)
  - [ ] `Set()` constructor
  - [ ] `Set.prototype`: `add`, `clear`, `delete`, `difference`, `entries`, `forEach`, `has`, `intersection`, `isDisjointFrom`, `isSubsetOf`, `isSupersetOf`, `keys`, `size`, `symmetricDifference`, `union`, `values`, `[@@iterator]`, `[@@toStringTag]`
- [ ] **WeakMap** (§24.3)
  - [ ] `WeakMap()` constructor
  - [ ] `delete`, `get`, `has`, `set`
- [ ] **WeakSet** (§24.4)
  - [ ] `WeakSet()` constructor
  - [ ] `add`, `delete`, `has`

**Tests:** `built-ins/Map/` (204), `built-ins/MapIteratorPrototype/` (11), `built-ins/Set/` (383), `built-ins/SetIteratorPrototype/` (11), `built-ins/WeakMap/` (141), `built-ins/WeakSet/` (85)

### 9.7 Structured Data (§25)
- [ ] **ArrayBuffer** (§25.1)
  - [ ] `ArrayBuffer()` constructor
  - [ ] `ArrayBuffer.isView()`
  - [ ] `ArrayBuffer.prototype`: `byteLength`, `detached`, `maxByteLength`, `resizable`, `resize`, `slice`, `transfer`, `transferToFixedLength`
- [ ] **SharedArrayBuffer** (§25.2)
  - [ ] `SharedArrayBuffer()` constructor
  - [ ] `grow`, `growable`, `byteLength`, `maxByteLength`, `slice`
- [ ] **DataView** (§25.3)
  - [ ] `DataView()` constructor
  - [ ] All get/set methods for each numeric type
- [ ] **Atomics** (§25.4)
  - [ ] `add`, `and`, `compareExchange`, `exchange`, `isLockFree`, `load`, `or`, `pause`, `store`, `sub`, `wait`, `waitAsync`, `notify`, `xor`
- [ ] **JSON** (§25.5)
  - [ ] `JSON.parse()` (with reviver)
  - [ ] `JSON.stringify()` (with replacer, space)
  - [ ] `JSON.isRawJSON()`, `JSON.rawJSON()`
  - [ ] `JSON[@@toStringTag]`

**Tests:** `built-ins/ArrayBuffer/` (196), `built-ins/SharedArrayBuffer/` (104), `built-ins/DataView/` (561), `built-ins/Atomics/` (382), `built-ins/JSON/` (165)

### 9.8 Managing Memory (§26)
- [ ] **WeakRef** (§26.1)
  - [ ] `WeakRef()` constructor
  - [ ] `WeakRef.prototype.deref()`
- [ ] **FinalizationRegistry** (§26.2)
  - [ ] `FinalizationRegistry()` constructor
  - [ ] `register()`, `unregister()`

**Tests:** `built-ins/WeakRef/` (29), `built-ins/FinalizationRegistry/` (47)

### 9.9 Control Abstraction Objects (§27)
- [ ] **Iterator** (§27.1)
  - [ ] `Iterator()` constructor
  - [ ] `Iterator.from()`
  - [ ] `Iterator.prototype`: `drop`, `every`, `filter`, `find`, `flatMap`, `forEach`, `map`, `reduce`, `some`, `take`, `toArray`, `[@@iterator]`, `[@@toStringTag]`
- [ ] **AsyncIteratorPrototype** (§27.1.4)
- [ ] **Promise** (§27.2)
  - [ ] `Promise()` constructor
  - [ ] `Promise.all()`, `Promise.allSettled()`, `Promise.any()`, `Promise.race()`, `Promise.try()`
  - [ ] `Promise.reject()`, `Promise.resolve()`, `Promise.withResolvers()`
  - [ ] `Promise.prototype.then()`, `.catch()`, `.finally()`
  - [ ] NewPromiseReactionJob, NewPromiseResolveThenableJob
  - [ ] PromiseResolve, PerformPromiseThen
  - [ ] PromiseCapability Records
- [ ] **GeneratorFunction** (§27.3)
- [ ] **AsyncGeneratorFunction** (§27.4)
- [ ] **Generator** prototype (§27.5)
- [ ] **AsyncGenerator** prototype (§27.6)
- [ ] **AsyncFunction** (§27.7)

**Tests:** `built-ins/Iterator/` (510), `built-ins/AsyncIteratorPrototype/` (13), `built-ins/Promise/` (639), `built-ins/GeneratorFunction/` (23), `built-ins/AsyncGeneratorFunction/` (23), `built-ins/GeneratorPrototype/` (61), `built-ins/AsyncGeneratorPrototype/` (48), `built-ins/AsyncFunction/` (18), `built-ins/AsyncFromSyncIteratorPrototype/` (38)

### 9.10 Reflection (§28)
- [ ] **Reflect** (§28.1)
  - [ ] `Reflect.apply()`, `.construct()`, `.defineProperty()`, `.deleteProperty()`, `.get()`, `.getOwnPropertyDescriptor()`, `.getPrototypeOf()`, `.has()`, `.isExtensible()`, `.ownKeys()`, `.preventExtensions()`, `.set()`, `.setPrototypeOf()`
- [ ] **Proxy** (§28.2)
  - [ ] `Proxy()` constructor
  - [ ] `Proxy.revocable()`
  - [ ] All 13 proxy handler traps: `getPrototypeOf`, `setPrototypeOf`, `isExtensible`, `preventExtensions`, `getOwnPropertyDescriptor`, `defineProperty`, `has`, `get`, `set`, `deleteProperty`, `ownKeys`, `apply`, `construct`
  - [ ] Proxy invariant enforcement

**Tests:** `built-ins/Reflect/` (153), `built-ins/Proxy/` (311)

### 9.11 Resource Management
- [ ] **DisposableStack** (§Disposable)
  - [ ] `DisposableStack()` constructor
  - [ ] `adopt()`, `defer()`, `dispose()`, `move()`, `use()`, `disposed`
- [ ] **AsyncDisposableStack**
  - [ ] Same methods as DisposableStack, async variants
- [ ] `Symbol.dispose`, `Symbol.asyncDispose`

**Tests:** `built-ins/DisposableStack/` (91), `built-ins/AsyncDisposableStack/` (52)

### 9.12 ShadowRealm
- [ ] `ShadowRealm()` constructor
- [ ] `evaluate()`, `importValue()`

**Tests:** `built-ins/ShadowRealm/` (67)

### 9.13 Temporal (Stage 3 — Optional)
- [ ] All Temporal types: `Instant`, `ZonedDateTime`, `PlainDateTime`, `PlainDate`, `PlainTime`, `PlainYearMonth`, `PlainMonthDay`, `Duration`, `Calendar`, `TimeZone`
- [ ] Full arithmetic, comparison, formatting

**Tests:** `built-ins/Temporal/` (4,482) — can be deferred

### 9.14 ThrowTypeError (§10.2.4)
- [ ] `%ThrowTypeError%` intrinsic function

**Tests:** `built-ins/ThrowTypeError/` (14)
