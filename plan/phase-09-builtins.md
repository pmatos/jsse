# Phase 9: Built-in Objects

**Spec Reference:** §19–28

## Goal
Implement all standard built-in objects, constructors, prototypes, and their methods.

Ordered roughly by dependency and importance.

## Tasks

### 9.1 Global Object (§19)
- [x] Global value properties: `Infinity`, `NaN`, `undefined`, `globalThis`
- [x] `eval()` (direct and indirect)
- [x] `isFinite()`
- [x] `isNaN()`
- [x] `parseFloat()`
- [x] `parseInt()`
- [ ] `decodeURI()` / `decodeURIComponent()`
- [ ] `encodeURI()` / `encodeURIComponent()`

**Tests:** `built-ins/global/` (29), `built-ins/eval/` (10), `built-ins/isFinite/` (15), `built-ins/isNaN/` (15), `built-ins/parseFloat/` (54), `built-ins/parseInt/` (55), `built-ins/decodeURI/` (55), `built-ins/decodeURIComponent/` (56), `built-ins/encodeURI/` (31), `built-ins/encodeURIComponent/` (31), `built-ins/Infinity/` (6), `built-ins/NaN/` (6), `built-ins/undefined/` (8)

### 9.2 Fundamental Objects (§20)
- [x] **Object** (§20.1) — 35% pass rate (1,199/3,411 tests)
  - [x] `Object()` constructor
  - [x] `Object.assign()`, `Object.create()`, `Object.defineProperty()`, `Object.defineProperties()`
  - [x] `Object.entries()`, `Object.fromEntries()`
  - [x] `Object.freeze()`, `Object.isFrozen()`
  - [x] `Object.getOwnPropertyDescriptor()`, `Object.getOwnPropertyDescriptors()`
  - [x] `Object.getOwnPropertyNames()`, `Object.getOwnPropertySymbols()`
  - [x] `Object.getPrototypeOf()`, `Object.setPrototypeOf()`
  - [ ] `Object.groupBy()`
  - [x] `Object.hasOwn()`
  - [x] `Object.is()`
  - [x] `Object.isExtensible()`, `Object.preventExtensions()`
  - [x] `Object.keys()`, `Object.values()`
  - [x] `Object.seal()`, `Object.isSealed()`
  - [x] `Object.prototype.hasOwnProperty()`, `Object.prototype.isPrototypeOf()`
  - [x] `Object.prototype.propertyIsEnumerable()`
  - [x] `Object.prototype.toLocaleString()`, `Object.prototype.toString()`, `Object.prototype.valueOf()`
  - [ ] `Object.prototype.__proto__` (Annex B)
  - [ ] `Object.prototype.__defineGetter__`, `__defineSetter__`, `__lookupGetter__`, `__lookupSetter__` (Annex B)
- [x] **Function** (§20.2) — 19% pass rate (95/509 tests)
  - [ ] `Function()` constructor (dynamic function creation)
  - [x] `Function.prototype.apply()`, `.bind()`, `.call()`
  - [ ] `Function.prototype.toString()`
  - [ ] `Function.prototype[@@hasInstance]`
  - [x] `Function.prototype.constructor`
  - [x] `name`, `length` properties
- [x] **Boolean** (§20.3)
  - [x] `Boolean()` constructor
  - [x] `Boolean.prototype.toString()`, `.valueOf()`
- [x] **Symbol** (§20.4)
  - [x] `Symbol()` constructor (not newable)
  - [ ] `Symbol.for()`, `Symbol.keyFor()`
  - [x] `Symbol.prototype.toString()`, `.valueOf()`, `.description`
  - [ ] `Symbol.prototype[@@toPrimitive]`, `[@@toStringTag]`
  - [x] All well-known symbols as static properties
- [x] **Error** objects (§20.5)
  - [x] `Error()`, `Error.prototype.message`, `.name`, `.toString()`
  - [ ] `Error.isError()`
  - [ ] `Error.prototype.stack` (implementation-defined)
  - [x] Native error types: `EvalError`, `RangeError`, `ReferenceError`, `SyntaxError`, `TypeError`, `URIError`
  - [ ] `AggregateError`
  - [ ] `SuppressedError`

**Tests:** `built-ins/Object/` (3,411), `built-ins/Function/` (509), `built-ins/Boolean/` (51), `built-ins/Symbol/` (94), `built-ins/Error/` (53), `built-ins/NativeErrors/` (92), `built-ins/AggregateError/` (25), `built-ins/SuppressedError/` (22)

### 9.3 Numbers & Dates (§21)
- [x] **Number** (§21.1) — 74% pass rate (248/335 tests)
  - [x] `Number()` constructor
  - [x] `Number.isFinite()`, `.isInteger()`, `.isNaN()`, `.isSafeInteger()`
  - [x] `Number.MAX_SAFE_INTEGER`, `.MIN_SAFE_INTEGER`, `.MAX_VALUE`, `.MIN_VALUE`, `.EPSILON`, `.NaN`, `.POSITIVE_INFINITY`, `.NEGATIVE_INFINITY`
  - [x] `Number.parseFloat()`, `.parseInt()` (attached to Number constructor)
  - [x] `Number.prototype.toExponential()`, `.toFixed()`, `.toPrecision()`, `.toString()`, `.valueOf()`, `.toLocaleString()` (with RangeError validation, spec-compliant formatting)
- [x] **BigInt** (§21.2)
  - [x] `BigInt()` function (not constructor)
  - [ ] `BigInt.asIntN()`, `BigInt.asUintN()`
  - [x] `BigInt.prototype.toString()`, `.valueOf()`, `.toLocaleString()`
- [x] **Math** (§21.3)
  - [x] All constants: `E`, `LN10`, `LN2`, `LOG10E`, `LOG2E`, `PI`, `SQRT1_2`, `SQRT2`
  - [x] Most methods: `abs`, `acos`, `acosh`, `asin`, `asinh`, `atan`, `atanh`, `atan2`, `cbrt`, `ceil`, `clz32`, `cos`, `cosh`, `exp`, `expm1`, `floor`, `fround`, `hypot`, `imul`, `log`, `log1p`, `log10`, `log2`, `max`, `min`, `pow`, `random`, `round`, `sign`, `sin`, `sinh`, `sqrt`, `tan`, `tanh`, `trunc`
  - [ ] `f16round`, `sumPrecise`
  - [ ] `Math[@@toStringTag]` = `"Math"`
- [ ] **Date** (§21.4) — **NOT IMPLEMENTED, 594 tests**
  - [ ] `Date()` constructor (multiple overloads)
  - [ ] `Date.now()`, `Date.parse()`, `Date.UTC()`
  - [ ] All prototype get/set methods (getFullYear, getMonth, getDate, getHours, getMinutes, getSeconds, getMilliseconds, getDay, getTime, getTimezoneOffset, and all `setX`/`getUTCX` variants)
  - [ ] `Date.prototype.toDateString()`, `.toTimeString()`, `.toISOString()`, `.toJSON()`, `.toLocaleDateString()`, `.toLocaleString()`, `.toLocaleTimeString()`, `.toString()`, `.toUTCString()`
  - [ ] `Date.prototype.valueOf()`, `[@@toPrimitive]`

**Tests:** `built-ins/Number/` (335), `built-ins/BigInt/` (75), `built-ins/Math/` (327), `built-ins/Date/` (594)

### 9.4 Text Processing (§22)
- [x] **String** (§22.1) — 24% pass rate (294/1,215 tests)
  - [x] `String()` constructor
  - [x] `String.fromCharCode()`, `String.fromCodePoint()`, `String.raw()`
  - [x] `String.prototype` methods: `at`, `charAt`, `charCodeAt`, `codePointAt`, `concat`, `endsWith`, `includes`, `indexOf`, `lastIndexOf`, `match`, `padEnd`, `padStart`, `repeat`, `replace`, `replaceAll`, `search`, `slice`, `split`, `startsWith`, `substring`, `toLowerCase`, `toString`, `toUpperCase`, `trim`, `trimEnd`, `trimStart`, `valueOf`
  - [ ] `isWellFormed`, `toWellFormed`, `normalize`, `localeCompare`, `matchAll`, `toLocaleLowerCase`, `toLocaleUpperCase`
  - [ ] `String.prototype[@@iterator]`
  - [ ] String HTML methods (Annex B): `anchor`, `big`, `blink`, `bold`, `fixed`, `fontcolor`, `fontsize`, `italics`, `link`, `small`, `strike`, `sub`, `sup`
- [x] **RegExp** (§22.2) — partial
  - [x] `RegExp()` constructor
  - [x] `RegExp.prototype.exec()`
  - [x] `RegExp.prototype.test()`
  - [x] `RegExp.prototype.toString()`
  - [ ] `RegExp.prototype[@@match]`, `[@@matchAll]`, `[@@replace]`, `[@@search]`, `[@@split]`
  - [x] Basic flags: `g`, `i`, `m`, `y`
  - [ ] Advanced flags: `d`, `s`, `u`, `v`
  - [x] Flag properties: `global`, `ignoreCase`, `multiline`, `source`, `flags`, `sticky`
  - [ ] `dotAll`, `hasIndices`, `unicode`, `unicodeSets`
  - [ ] Named capture groups
  - [ ] Lookbehind assertions
  - [ ] Unicode property escapes
  - [ ] Set notation (`v` flag)
  - [ ] `lastIndex` handling
  - [ ] `RegExp.$1`–`$9` and legacy features (Annex B)

**Tests:** `built-ins/String/` (1,215), `built-ins/RegExp/` (1,879), `built-ins/StringIteratorPrototype/` (7), `built-ins/RegExpStringIteratorPrototype/` (17)

### 9.5 Indexed Collections (§23)
- [x] **Array** (§23.1) — 25% pass rate (736/2,989 tests)
  - [x] `Array()` constructor
  - [x] `Array.from()`, `Array.isArray()`, `Array.of()`
  - [x] `Array.prototype` methods: `at`, `concat`, `every`, `fill`, `filter`, `find`, `findIndex`, `findLast`, `findLastIndex`, `flat`, `flatMap`, `forEach`, `includes`, `indexOf`, `join`, `lastIndexOf`, `map`, `pop`, `push`, `reduce`, `reduceRight`, `reverse`, `shift`, `slice`, `some`, `sort`, `splice`, `toString`, `unshift`
  - [ ] `copyWithin`, `entries`, `keys`, `values`, `toLocaleString`, `toReversed`, `toSorted`, `toSpliced`, `with`
  - [ ] `Array.prototype[@@iterator]`, `[@@unscopables]`
  - [x] Array length semantics
  - [ ] Array species (`@@species`)
  - [ ] Array groupBy
- [ ] **TypedArray** (§23.2) — **NOT IMPLEMENTED**
  - [ ] `%TypedArray%` intrinsic (abstract base)
  - [ ] All concrete constructors: `Int8Array`, `Uint8Array`, `Uint8ClampedArray`, `Int16Array`, `Uint16Array`, `Int32Array`, `Uint32Array`, `Float16Array`, `Float32Array`, `Float64Array`, `BigInt64Array`, `BigUint64Array`
  - [ ] `TypedArray.from()`, `TypedArray.of()`
  - [ ] All prototype methods (similar to Array but with typed semantics)
  - [ ] Buffer, byteOffset, byteLength, length
  - [ ] `Uint8Array.fromBase64()`, `Uint8Array.fromHex()`, `.toBase64()`, `.toHex()`, `.setFromBase64()`, `.setFromHex()`

**Tests:** `built-ins/Array/` (3,079), `built-ins/ArrayIteratorPrototype/` (27), `built-ins/TypedArray/` (1,438), `built-ins/TypedArrayConstructors/` (736), `built-ins/Uint8Array/` (68)

### 9.6 Keyed Collections (§24) — **NOT IMPLEMENTED, 587 tests**
- [ ] **Map** (§24.1) — 0% (0/204 tests)
  - [ ] `Map()` constructor
  - [ ] `Map.prototype`: `clear`, `delete`, `entries`, `forEach`, `get`, `has`, `keys`, `set`, `size`, `values`, `[@@iterator]`, `[@@toStringTag]`
  - [ ] `Map.groupBy()`
- [ ] **Set** (§24.2) — 0% (0/383 tests)
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
- [x] **JSON** (§25.5)
  - [x] `JSON.parse()` (with reviver)
  - [x] `JSON.stringify()` (with replacer, space)
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
- [ ] **Iterator** (§27.1) — **BLOCKER: 2% pass rate (8/510 tests)**
  - [ ] `Iterator()` constructor
  - [ ] `Iterator.from()`
  - [ ] `Iterator.prototype`: `drop`, `every`, `filter`, `find`, `flatMap`, `forEach`, `map`, `reduce`, `some`, `take`, `toArray`, `[@@iterator]`, `[@@toStringTag]`
- [ ] **AsyncIteratorPrototype** (§27.1.4)
- [ ] **Promise** (§27.2) — **BLOCKER: 0% pass rate (0/281 tests)**
  - [ ] `Promise()` constructor
  - [ ] `Promise.all()`, `Promise.allSettled()`, `Promise.any()`, `Promise.race()`, `Promise.try()`
  - [ ] `Promise.reject()`, `Promise.resolve()`, `Promise.withResolvers()`
  - [ ] `Promise.prototype.then()`, `.catch()`, `.finally()`
  - [ ] NewPromiseReactionJob, NewPromiseResolveThenableJob
  - [ ] PromiseResolve, PerformPromiseThen
  - [ ] PromiseCapability Records
- [ ] **GeneratorFunction** (§27.3) — depends on generator runtime
- [ ] **AsyncGeneratorFunction** (§27.4) — depends on async + generators
- [ ] **Generator** prototype (§27.5) — depends on generator runtime
- [ ] **AsyncGenerator** prototype (§27.6) — depends on async + generators
- [ ] **AsyncFunction** (§27.7) — depends on Promise

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
