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
- [x] `decodeURI()` / `decodeURIComponent()` — 121/173 passing (70%)
- [x] `encodeURI()` / `encodeURIComponent()` — 121/173 passing (70%)

**Tests:** `built-ins/global/` (29), `built-ins/eval/` (10), `built-ins/isFinite/` (15), `built-ins/isNaN/` (15), `built-ins/parseFloat/` (54), `built-ins/parseInt/` (55), `built-ins/decodeURI/` (55), `built-ins/decodeURIComponent/` (56), `built-ins/encodeURI/` (31), `built-ins/encodeURIComponent/` (31), `built-ins/Infinity/` (6), `built-ins/NaN/` (6), `built-ins/undefined/` (8)

### 9.2 Fundamental Objects (§20)
- [x] **Object** (§20.1) — 94% pass rate (6,407/6,802 scenarios)
  - [x] `Object()` constructor
  - [x] `Object.assign()`, `Object.create()`, `Object.defineProperty()`, `Object.defineProperties()`
  - [x] `Object.entries()`, `Object.fromEntries()`
  - [x] `Object.freeze()`, `Object.isFrozen()`
  - [x] `Object.getOwnPropertyDescriptor()`, `Object.getOwnPropertyDescriptors()`
  - [x] `Object.getOwnPropertyNames()`, `Object.getOwnPropertySymbols()`
  - [x] `Object.getPrototypeOf()`, `Object.setPrototypeOf()`
  - [x] `Object.groupBy()`
  - [x] `Object.hasOwn()`
  - [x] `Object.is()`
  - [x] `Object.isExtensible()`, `Object.preventExtensions()`
  - [x] `Object.keys()`, `Object.values()`
  - [x] `Object.seal()`, `Object.isSealed()`
  - [x] `Object.prototype.hasOwnProperty()`, `Object.prototype.isPrototypeOf()`
  - [x] `Object.prototype.propertyIsEnumerable()`
  - [x] `Object.prototype.toLocaleString()`, `Object.prototype.toString()`, `Object.prototype.valueOf()`
  - [x] `Object.prototype.__proto__` (Annex B)
  - [x] `Object.prototype.__defineGetter__`, `__defineSetter__`, `__lookupGetter__`, `__lookupSetter__` (Annex B)
- [x] **Function** (§20.2) — 94% pass rate (839/893 scenarios)
  - [x] `Function()` constructor (sloppy closure env, ToString coercion)
  - [x] `Function.prototype.apply()` (CreateListFromArrayLike, getter-aware, arity=2)
  - [x] `Function.prototype.bind()` (bound_target_function, HasOwnProperty length, getter-aware name)
  - [x] `Function.prototype.call()`
  - [x] `Function.prototype.toString()` (Proxy callable check)
  - [x] `Function.prototype[@@hasInstance]` (bound function chain)
  - [x] `Function.prototype.constructor`
  - [x] `name`, `length` properties
  - [x] Class constructor callable check, derived constructor return order
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
  - [x] `Error.isError()`
  - [ ] `Error.prototype.stack` (implementation-defined)
  - [x] Native error types: `EvalError`, `RangeError`, `ReferenceError`, `SyntaxError`, `TypeError`, `URIError`
  - [x] `AggregateError` — ✅ 14/25 passing
  - [x] `SuppressedError` — ✅ 13/22 passing

**Tests:** `built-ins/Object/` (3,411), `built-ins/Function/` (509), `built-ins/Boolean/` (51), `built-ins/Symbol/` (94), `built-ins/Error/` (53), `built-ins/NativeErrors/` (92), `built-ins/AggregateError/` (25), `built-ins/SuppressedError/` (22)

### 9.3 Numbers & Dates (§21)
- [x] **Number** (§21.1) — 98% pass rate (331/335 tests)
  - [x] `Number()` constructor
  - [x] `Number.isFinite()`, `.isInteger()`, `.isNaN()`, `.isSafeInteger()`
  - [x] `Number.MAX_SAFE_INTEGER`, `.MIN_SAFE_INTEGER`, `.MAX_VALUE`, `.MIN_VALUE`, `.EPSILON`, `.NaN`, `.POSITIVE_INFINITY`, `.NEGATIVE_INFINITY`
  - [x] `Number.parseFloat()`, `.parseInt()` (attached to Number constructor)
  - [x] `Number.prototype.toExponential()`, `.toFixed()`, `.toPrecision()`, `.toString()`, `.valueOf()`, `.toLocaleString()` (with RangeError validation, spec-compliant formatting)
- [x] **BigInt** (§21.2)
  - [x] `BigInt()` function (not constructor)
  - [x] `BigInt.asIntN()`, `BigInt.asUintN()`
  - [x] `BigInt.prototype.toString()`, `.valueOf()`, `.toLocaleString()`
- [x] **Math** (§21.3)
  - [x] All constants: `E`, `LN10`, `LN2`, `LOG10E`, `LOG2E`, `PI`, `SQRT1_2`, `SQRT2`
  - [x] Most methods: `abs`, `acos`, `acosh`, `asin`, `asinh`, `atan`, `atanh`, `atan2`, `cbrt`, `ceil`, `clz32`, `cos`, `cosh`, `exp`, `expm1`, `floor`, `fround`, `hypot`, `imul`, `log`, `log1p`, `log10`, `log2`, `max`, `min`, `pow`, `random`, `round`, `sign`, `sin`, `sinh`, `sqrt`, `tan`, `tanh`, `trunc`
  - [x] `f16round`, `sumPrecise`
  - [x] `Math[@@toStringTag]` = `"Math"`
- [x] **Date** (§21.4) — 95% pass rate (1,124/1,188 scenarios)
  - [x] `Date()` constructor (multiple overloads)
  - [x] `Date.now()`, `Date.parse()`, `Date.UTC()`
  - [x] All prototype get/set methods (getFullYear, getMonth, getDate, getHours, getMinutes, getSeconds, getMilliseconds, getDay, getTime, getTimezoneOffset, and all `setX`/`getUTCX` variants)
  - [x] `Date.prototype.toDateString()`, `.toTimeString()`, `.toISOString()`, `.toJSON()`, `.toLocaleDateString()`, `.toLocaleString()`, `.toLocaleTimeString()`, `.toString()`, `.toUTCString()`
  - [x] `Date.prototype.valueOf()`, `[@@toPrimitive]`

**Tests:** `built-ins/Number/` (335), `built-ins/BigInt/` (75), `built-ins/Math/` (327), `built-ins/Date/` (594)

### 9.4 Text Processing (§22)
- [x] **String** (§22.1) — 92% pass rate (1,120/1,215 tests)
  - [x] `String()` constructor (spec-compliant ToString via ToPrimitive, Symbol coercion)
  - [x] `String.fromCharCode()`, `String.fromCodePoint()`, `String.raw()`
  - [x] `String.prototype` methods: `at`, `charAt`, `charCodeAt`, `codePointAt`, `concat`, `endsWith`, `includes`, `indexOf`, `lastIndexOf`, `match`, `matchAll`, `padEnd`, `padStart`, `repeat`, `replace`, `replaceAll`, `search`, `slice`, `split`, `startsWith`, `substring`, `toLowerCase`, `toLocaleLowerCase`, `toLocaleUpperCase`, `toString`, `toUpperCase`, `trim`, `trimEnd`, `trimStart`, `valueOf`, `normalize`, `localeCompare`, `isWellFormed`, `toWellFormed`
  - [x] RequireObjectCoercible on `this` for all methods, UTF-16 code unit indexing, proper argument coercion via ToPrimitive
  - [ ] `String.prototype[@@iterator]`
  - [ ] String HTML methods (Annex B): `anchor`, `big`, `blink`, `bold`, `fixed`, `fontcolor`, `fontsize`, `italics`, `link`, `small`, `strike`, `sub`, `sup`
- [x] **RegExp** (§22.2) — 3,481/3,756 (92.7%)
  - [x] `RegExp()` constructor
  - [x] `RegExp.prototype.exec()` (TypeError for non-object `this`)
  - [x] `RegExp.prototype.test()` (TypeError for non-object `this`)
  - [x] `RegExp.prototype.toString()`
  - [x] `RegExp.prototype[@@match]`, `[@@matchAll]`, `[@@replace]`, `[@@search]`, `[@@split]`
  - [x] Basic flags: `g`, `i`, `m`, `s`, `y`, `d`
  - [ ] Advanced flags: `u`, `v` (partial — property escapes work via fancy-regex)
  - [x] Flag property getters on prototype: `global`, `ignoreCase`, `multiline`, `source`, `flags`, `sticky`, `dotAll`, `hasIndices`, `unicode`, `unicodeSets`
  - [x] `RegExp.prototype.flags` getter (spec-compliant accessor)
  - [x] ToString coercion for Symbol method arguments
  - [x] `lastIndex` handling
  - [x] `RegExp.escape()` static method (TC39 stage 4)
  - [x] JS→Rust pattern translation (`fancy-regex` backend with `regex` fallback)
  - [x] Named capture groups (groups property, `$<name>` replacements, functional replacer groups arg)
  - [x] Match indices (`d` flag / `hasIndices`) with `indices` and `indices.groups`
  - [x] Lookbehind assertions (fixed-length; variable-length limited by fancy-regex)
  - [x] Unicode property escapes (`\p{...}` / `\P{...}` via fancy-regex)
  - [ ] Set notation (`v` flag)
  - [ ] `RegExp.$1`–`$9` and legacy features (Annex B)

**Tests:** `built-ins/String/` (1,215), `built-ins/RegExp/` (1,947), `built-ins/StringIteratorPrototype/` (7), `built-ins/RegExpStringIteratorPrototype/` (17)

### 9.5 Indexed Collections (§23)
- [x] **Array** (§23.1) — 89% pass rate (2,734/3,079 tests)
  - [x] `Array()` constructor
  - [x] `Array.from()`, `Array.isArray()`, `Array.of()`
  - [x] `Array.prototype` methods: `at`, `concat`, `copyWithin`, `entries`, `every`, `fill`, `filter`, `find`, `findIndex`, `findLast`, `findLastIndex`, `flat`, `flatMap`, `forEach`, `includes`, `indexOf`, `join`, `keys`, `lastIndexOf`, `map`, `pop`, `push`, `reduce`, `reduceRight`, `reverse`, `shift`, `slice`, `some`, `sort`, `splice`, `toReversed`, `toSorted`, `toSpliced`, `toString`, `unshift`, `values`, `with`
  - [x] `Array.prototype[@@iterator]`
  - [x] Spec-compliant: ToObject(this), LengthOfArrayLike, IsCallable validation, thisArg support, property-based access for array-like objects
  - [ ] `toLocaleString`, `[@@unscopables]`
  - [ ] Array species (`@@species`)
  - [ ] Array groupBy
- [x] **TypedArray** (§23.2) — **IMPLEMENTED** (2,380/2,860 = 83.2%, 1,116/1,442 constructors = 77.4%)
  - [x] `%TypedArray%` intrinsic (abstract base) with shared prototype methods
  - [x] `%TypedArray%` constructor wiring: `.prototype`, `.constructor`, `@@species`, prototype chain inheritance
  - [x] All concrete constructors: `Int8Array`, `Uint8Array`, `Uint8ClampedArray`, `Int16Array`, `Uint16Array`, `Int32Array`, `Uint32Array`, `Float32Array`, `Float64Array`, `BigInt64Array`, `BigUint64Array`
  - [x] `TypedArray.from()`, `TypedArray.of()`
  - [x] Prototype methods: `at`, `set`, `subarray`, `slice`, `copyWithin`, `fill`, `indexOf`, `lastIndexOf`, `includes`, `find`, `findIndex`, `findLast`, `findLastIndex`, `forEach`, `map`, `filter`, `reduce`, `reduceRight`, `every`, `some`, `reverse`, `sort`, `join`, `toString`, `toReversed`, `toSorted`, `with`, `entries`, `keys`, `values`
  - [x] Buffer, byteOffset, byteLength, length getters
  - [x] `Uint8Array.fromBase64()`, `Uint8Array.fromHex()`, `.toBase64()`, `.toHex()`, `.setFromBase64()`, `.setFromHex()`
  - [ ] `Float16Array` (not yet implemented)

**Tests:** `built-ins/Array/` (3,079), `built-ins/ArrayIteratorPrototype/` (27), `built-ins/TypedArray/` (1,438), `built-ins/TypedArrayConstructors/` (736), `built-ins/Uint8Array/` (68)

### 9.6 Keyed Collections (§24) — **NOT IMPLEMENTED, 587 tests**
- [x] **Map** (§24.1) — 99% (203/204 tests)
  - [x] `Map()` constructor
  - [x] `Map.prototype`: `clear`, `delete`, `entries`, `forEach`, `get`, `has`, `keys`, `set`, `size`, `values`, `[@@iterator]`, `[@@toStringTag]`
  - [x] `Map.groupBy()`
  - [x] `Map.prototype.getOrInsert()`, `Map.prototype.getOrInsertComputed()`
- [x] **Set** (§24.2) — 95% (365/383 tests)
  - [x] `Set()` constructor
  - [x] `Set.prototype`: `add`, `clear`, `delete`, `difference`, `entries`, `forEach`, `has`, `intersection`, `isDisjointFrom`, `isSubsetOf`, `isSupersetOf`, `keys`, `size`, `symmetricDifference`, `union`, `values`, `[@@iterator]`, `[@@toStringTag]`
- [x] **WeakMap** (§24.3) — ✅ 72/141 passing
  - [x] `WeakMap()` constructor
  - [x] `delete`, `get`, `has`, `set`
- [x] **WeakSet** (§24.4) — ✅ 50/85 passing
  - [x] `WeakSet()` constructor
  - [x] `add`, `delete`, `has`

**Tests:** `built-ins/Map/` (204), `built-ins/MapIteratorPrototype/` (11), `built-ins/Set/` (383), `built-ins/SetIteratorPrototype/` (11), `built-ins/WeakMap/` (141), `built-ins/WeakSet/` (85)

### 9.7 Structured Data (§25)
- [x] **ArrayBuffer** (§25.1) — **IMPLEMENTED** (136/196 = 69.4%)
  - [x] `ArrayBuffer()` constructor (NewTarget, OrdinaryCreateFromConstructor, ToIndex)
  - [x] `ArrayBuffer.isView()`
  - [x] `ArrayBuffer.prototype`: `byteLength`, `slice`
  - [x] `ArrayBuffer.prototype`: `transfer`, `transferToFixedLength`
  - [ ] `ArrayBuffer.prototype`: `detached`, `maxByteLength`, `resizable`, `resize`
- [ ] **SharedArrayBuffer** (§25.2)
  - [ ] `SharedArrayBuffer()` constructor
  - [ ] `grow`, `growable`, `byteLength`, `maxByteLength`, `slice`
- [x] **DataView** (§25.3) — **IMPLEMENTED** (476/561 = 84.8%)
  - [x] `DataView()` constructor
  - [x] All get/set methods for each numeric type (Int8 through BigUint64, with endianness)
- [ ] **Atomics** (§25.4)
  - [ ] `add`, `and`, `compareExchange`, `exchange`, `isLockFree`, `load`, `or`, `pause`, `store`, `sub`, `wait`, `waitAsync`, `notify`, `xor`
- [x] **JSON** (§25.5)
  - [x] `JSON.parse()` (with reviver)
  - [x] `JSON.stringify()` (with replacer, space)
  - [x] `JSON.isRawJSON()`, `JSON.rawJSON()`
  - [x] `JSON[@@toStringTag]`

**Tests:** `built-ins/ArrayBuffer/` (196), `built-ins/SharedArrayBuffer/` (104), `built-ins/DataView/` (561), `built-ins/Atomics/` (382), `built-ins/JSON/` (165)

### 9.8 Managing Memory (§26)
- [x] **WeakRef** (§26.1) — ✅ 28/29 passing (97%)
  - [x] `WeakRef()` constructor (CanBeHeldWeakly validation)
  - [x] `WeakRef.prototype.deref()`
  - [x] OrdinaryCreateFromConstructor for NewTarget prototype
- [x] **FinalizationRegistry** (§26.2) — ✅ 46/47 passing (98%)
  - [x] `FinalizationRegistry()` constructor
  - [x] `register()`, `unregister()` (CanBeHeldWeakly validation, symbol tokens)
  - [x] OrdinaryCreateFromConstructor for NewTarget prototype

**Tests:** `built-ins/WeakRef/` (28/29), `built-ins/FinalizationRegistry/` (46/47)

### 9.9 Control Abstraction Objects (§27)
- [x] **Iterator** (§27.1) — ✅ 436/510 (85%). Constructor, helpers (toArray/forEach/reduce/some/every/find/map/filter/take/drop/flatMap), Iterator.from, Iterator.concat, Iterator.zip, Iterator.zipKeyed, Symbol.dispose. Getter-aware iterator protocol, IteratorCloseAll, GetIteratorFlattenable.
- [x] **AsyncIteratorPrototype** (§27.1.4) — ✅ [Symbol.asyncIterator] returns this
- [x] **Promise** (§27.2) — ✅ 599/639 tests passing (94%)
  - [x] `Promise()` constructor
  - [x] `Promise.all()`, `Promise.allSettled()`, `Promise.any()`, `Promise.race()`
  - [x] `Promise.reject()`, `Promise.resolve()`
  - [x] `Promise.try()`, `Promise.withResolvers()`
  - [x] `Promise.prototype.then()`, `.catch()`, `.finally()`
  - [x] NewPromiseReactionJob, NewPromiseResolveThenableJob
  - [x] PromiseResolve, PerformPromiseThen
  - [x] PromiseCapability Records (via create_resolving_functions)
  - [x] Microtask queue with synchronous drain
- [x] **GeneratorFunction** (§27.3) — ✅ 18/23 passing (78%)
- [x] **AsyncGeneratorFunction** (§27.4) — ✅ async function* dispatch, AsyncGeneratorFunction.prototype chain, 9/23 (39%)
- [x] **Generator** prototype (§27.5) — ✅ 49/61 passing (80%)
- [x] **AsyncGenerator** prototype (§27.6) — ✅ next/return/throw returning promises, rejected promises for type errors, yield* async delegation, 30/48 (63%)
- [x] **AsyncFunction** (§27.7) — ✅ Basic async/await works

**Tests:** `built-ins/Iterator/` (510), `built-ins/AsyncIteratorPrototype/` (13), `built-ins/Promise/` (639), `built-ins/GeneratorFunction/` (23), `built-ins/AsyncGeneratorFunction/` (23), `built-ins/GeneratorPrototype/` (61), `built-ins/AsyncGeneratorPrototype/` (48), `built-ins/AsyncFunction/` (18), `built-ins/AsyncFromSyncIteratorPrototype/` (38)

### 9.10 Reflection (§28)
- [x] **Reflect** (§28.1) — ✅ 153/153 passing (100%)
  - [x] `Reflect.apply()`, `.construct()`, `.defineProperty()`, `.deleteProperty()`, `.get()`, `.getOwnPropertyDescriptor()`, `.getPrototypeOf()`, `.has()`, `.isExtensible()`, `.ownKeys()`, `.preventExtensions()`, `.set()`, `.setPrototypeOf()`
  - [x] Proxy trap delegation from Reflect methods
  - [x] `Reflect[Symbol.toStringTag]` = "Reflect"
  - [x] OrdinaryOwnPropertyKeys ordering (integer indices → strings → symbols)
  - [x] CreateListFromArrayLike validation in apply/construct
  - [x] ToPropertyKey error propagation
  - [x] setPrototypeOf returns false (not throws) per spec
- [x] **Proxy** (§28.2) — ✅ 231/311 passing (74%)
  - [x] `Proxy()` constructor
  - [x] `Proxy.revocable()`
  - [x] All 13 proxy handler traps
  - [x] Proxy invariant enforcement (get/set/has/delete/defineProperty/getOwnPropertyDescriptor/ownKeys/getPrototypeOf/setPrototypeOf/isExtensible/preventExtensions)

**Tests:** `built-ins/Reflect/` (153/153), `built-ins/Proxy/` (231/311)

### 9.11 Resource Management
- [x] **DisposableStack** (§Disposable) — ✅ 71/91 passing
  - [x] `DisposableStack()` constructor
  - [x] `adopt()`, `defer()`, `dispose()`, `move()`, `use()`, `disposed`
- [x] **AsyncDisposableStack** — ✅ 35/52 passing
  - [x] Same methods as DisposableStack, async variants
- [x] `Symbol.dispose`, `Symbol.asyncDispose`

**Tests:** `built-ins/DisposableStack/` (71/91), `built-ins/AsyncDisposableStack/` (35/52)

### 9.12 ShadowRealm
- [ ] `ShadowRealm()` constructor
- [ ] `evaluate()`, `importValue()`

**Tests:** `built-ins/ShadowRealm/` (67)

### 9.13 Temporal — ✅ 100% (8,964/8,964 scenarios)
- [x] All Temporal types: `Instant`, `ZonedDateTime`, `PlainDateTime`, `PlainDate`, `PlainTime`, `PlainYearMonth`, `PlainMonthDay`, `Duration`
- [x] Full arithmetic, comparison, formatting, rounding
- [x] Timezone support (IANA via ICU4X): DST transitions, disambiguation, offset handling
- [x] Calendar support (ISO8601 + non-ISO via ICU4X): Hebrew, Buddhist, Coptic, Ethiopian, Indian, Islamic, Japanese, Persian, ROC, Chinese, Dangi, Gregory
- [x] `Temporal.Now` namespace
- [x] intl402/Temporal: 3,838/3,838 (100.00%)

**Tests:** `built-ins/Temporal/` (8,964/8,964), `intl402/Temporal/` (3,838/3,838)

### 9.14 ThrowTypeError (§10.2.4)
- [x] `%ThrowTypeError%` intrinsic function — ✅ 13/14 passing (93%)

**Tests:** `built-ins/ThrowTypeError/` (14)
