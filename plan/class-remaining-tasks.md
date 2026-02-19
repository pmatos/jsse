# Class Element Remaining Tasks — Detailed Plans

502 scenarios remain failing across class tests. This document details plans for the 3 remaining tasks from the class element improvement plan, now informed by actual failure analysis.

## Task 6: Field Initialization Order (4 scenarios)

**Tests:** `intercalated-static-non-static-computed-fields.js` (expr + decl × sloppy + strict)

**Root cause:** Computed property names for class fields must ALL be evaluated in declaration order (step 28 of ClassDefinitionEvaluation) BEFORE any static field initializers run (step 34). Currently, static field initializers run immediately when the element is encountered.

**Example:** `class C { [i++] = i++; static [i++] = i++; [i++] = i++; }` expects:
- Computed keys: i=0 (instance), i=1 (static), i=2 (instance) — all in declaration order
- Static init: i=3 (for static field with key "1")
- Instance init on `new C()`: i=4 (for field "0"), i=5 (for field "2")
- Total: i=6

**Changes needed in `eval_class_inner()` (eval.rs):**
1. Split class element processing into two passes:
   - **Pass 1:** Iterate all elements in source order. For each element with a computed key, evaluate the key expression NOW. Store `(resolved_key, initializer_expr)` pairs for both static and instance fields separately, preserving declaration order.
   - **Pass 2:** After all computed keys are resolved, run static field initializers in declaration order. Store instance field definitions (with pre-resolved keys) for later use in `initialize_instance_elements()`.
2. Change instance field storage from `(PropertyKey, Option<Expression>)` to `(String, Option<Expression>)` where key is pre-resolved.
3. Keep method installations inline (they don't have ordering issues).

**Estimated impact:** 4 scenarios directly, possible bonus from other computed-key ordering tests.

---

## Task 9: Async Generator yield* Delegation (64 scenarios)

### 9a: yield-star-async-next/return/throw — thenable mock iterators (48 scenarios)

**Tests:** `yield-star-async-next.js`, `yield-star-async-return.js`, `yield-star-async-throw.js` across 8 method contexts (expr/decl × static/instance × gen-method/private-gen-method).

**Root cause:** These tests use mock async iterators that return thenable objects (objects with a `get then()` getter) from their `next()`/`return()`/`throw()` methods. The spec requires:
1. `innerResult = Invoke(iterator, "next", ...)` — returns a thenable
2. `innerResult = Await(innerResult)` — resolves the thenable
3. Await triggers Promise resolution which calls `Get(resolution, "then")` — must invoke the `then` getter
4. The `then` function is called with resolve/reject, which eventually resolves to the actual `{done, value}` result
5. `done` and `value` are accessed via getters on the resolved result

The core issue is that our `Await` implementation doesn't properly handle thenable resolution in the yield* delegation path. Specifically:
- When `next()` returns a thenable object (not a real Promise), `Await` must wrap it via `PromiseResolve`, which detects the `.then` getter and enqueues a `PromiseResolveThenableJob`
- The thenable's `then` function must be called with the promise's resolve/reject callbacks
- The resolved value then goes through the normal done/value extraction

**Changes needed:**
1. In `eval.rs` async generator yield* delegation path: ensure `Await(innerResult)` properly resolves thenables by going through full Promise resolution machinery
2. The `await_promise_value` / promise resolution logic needs to handle thenable objects — when the resolved value has a `.then` that is callable, it must enqueue PromiseResolveThenableJob
3. Check that the promise microtask queue processes PromiseResolveThenableJob before extracting done/value
4. Verify getter invocation order matches spec (the tests verify exact operation ordering via a log array)

**Key spec references:**
- §27.6.3.8 (yield*) step 7: `Await(innerResult)`
- §27.2.1.3.2 (Promise Resolve Functions) steps 8-12: `Get(resolution, "then")`, if callable, `EnqueueJob(PromiseResolveThenableJob)`
- §27.2.2.1 (PromiseResolveThenableJob): calls `then.call(resolution, resolve, reject)`

### 9b: yield-star-next-then-get-abrupt (16 scenarios)

**Tests:** `yield-star-next-then-get-abrupt.js` across 8 method contexts.

**Root cause:** When `next()` returns an object whose `get then()` throws, the Await should detect this as a thenable resolution failure and reject the promise. The generator should then receive this rejection and propagate it.

**Changes needed:**
- Same thenable resolution infrastructure as 9a
- The `Get(resolution, "then")` abrupt completion must trigger promise rejection
- Generator must receive the rejected value and propagate it properly

**Estimated impact:** 64 scenarios from thenable fixes alone.

---

## Task 10: Remaining Edge Cases (434 scenarios)

These group into distinct sub-categories:

### 10a: Forbidden extensions — caller/arguments on methods (80 scenarios)

**Tests:** `forbidden-ext-direct-access-prop-arguments.js`, `forbidden-ext-direct-access-prop-caller.js`, etc., across all method types (async, generator, async-gen, static variants).

**Root cause:** Strict-mode class methods should NOT have own `arguments` or `caller` properties. Our engine creates `arguments` or `caller` own properties on some function objects. Per spec, methods defined via MethodDefinition, async/generator functions etc. must not have these.

**Fix:** In `create_function()` or method-specific setup, ensure methods created from class bodies do NOT get `arguments`/`caller` own properties. This is related to the `%ThrowTypeError%` work done previously — need to verify the property stripping applies to all class method forms (async gen, private methods, static methods).

### 10b: Decorators (40 scenarios)

**Tests:** `decorator/syntax/valid/*.js` and `decorator/syntax/class-valid/*.js`.

**Root cause:** Decorator syntax (`@expr class ...`, `@expr method()`) is not parsed at all. This is a Stage 3 feature (ES2025+).

**Fix:** Implement decorator parsing in `parse_class_element()` and `parse_class_declaration()`/`parse_class_expression()`. At minimum, parse the syntax without runtime semantics to avoid SyntaxError. Full decorator semantics is a large feature.

### 10c: Super property edge cases (36 scenarios)

**Tests:** Various `super.property` tests in constructor/field contexts, `fields-run-once-on-double-super.js`, `prod-private-getter-before-super-return-in-field-initializer.js`, etc.

**Root cause:** Multiple issues:
- `super()` called twice should throw (TDZ re-initialization)
- `super.property` before `super()` in constructors should be allowed per spec (it's `this` that's restricted, not `super`)
- Field initializers that access private fields before super returns
- Super property evaluation in constructors with specific ordering expectations

**Fix:** Case-by-case investigation. Main themes:
1. Track whether `super()` has been called; throw on second call
2. Allow `super.prop` access (uses constructor's `[[HomeObject]]`) even before `super()` binds `this`
3. Field init ordering relative to super() return

### 10d: yield as identifier in strict mode (32 scenarios)

**Tests:** `yield-identifier-strict.js`, `yield-identifier-spread-strict.js` across all generator method types.

**Root cause:** In strict mode within a generator, `yield` is a reserved word and cannot be used as an identifier. These tests verify the SyntaxError. Our parser may be allowing `yield` as identifier in some generator/strict contexts.

**Fix:** Check parser's `yield` identifier handling — in strict mode or generator context, `yield` as BindingIdentifier should be SyntaxError.

### 10e: Subclass/constructor edge cases (25 scenarios)

**Tests:** `subclass-SharedArrayBuffer.js`, `default-constructor-2.js`, `default-constructor-spread-override.js`, `derived-class-return-override-for-of.js`, `superclass-async-function.js`, `superclass-generator-function.js`, etc.

**Root cause:** Multiple issues:
- SharedArrayBuffer not implemented (requires it as constructor)
- Default derived constructor must spread arguments to super: `constructor(...args) { super(...args); }`
- Return override in derived constructors (returning non-undefined object)
- Using async/generator functions as superclass
- Proxy with no `.prototype` as superclass should throw

### 10f: Await handling (22 scenarios)

**Tests:** `class-name-ident-await-escaped.js`, `class-name-ident-await-module.js`, `cpn-class-expr-*-computed-property-name-from-await-expression.js`.

**Root cause:** Escaped `await` (`\u0061wait`) used as class name identifier, `await` as class name in module context (should be SyntaxError), and computed property names involving await expressions in async contexts.

### 10g: Static initializer restrictions (17 scenarios)

**Tests:** `static-init-arguments-functions.js`, `static-init-arguments-methods.js`, `static-init-await-binding-invalid.js`, `static-init-invalid-arguments.js`, `static-init-invalid-label-dup.js`, `static-init-invalid-lex-dup.js`, `static-init-invalid-lex-var.js`, `static-init-super-property.js`, `class-name-static-initializer-anonymous.js`.

**Root cause:** Various static block restrictions not fully enforced:
- `arguments` reference in static blocks should resolve to the static block's own arguments object (empty), not an outer function's
- Duplicate labels, duplicate let/var bindings within static blocks
- `await` as binding identifier (SyntaxError)
- `super.prop` in static blocks should work (evaluates using class as home object)
- Static initializer ClassName binding

### 10h: Private accessor static/non-static mismatch early error (8 scenarios)

**Tests:** `private-non-static-getter-static-setter-early-error.js`, etc.

**Root cause:** A private name with a non-static getter and a static setter (or vice versa) should be an early SyntaxError. Our parser doesn't detect this cross-static-boundary mismatch.

**Fix:** In class element parsing, track whether each private accessor name is static or non-static. If getter and setter for the same `#name` disagree on staticness, throw SyntaxError.

### 10i: Private method double initialization (8 scenarios)

**Tests:** `private-method-double-initialisation*.js`.

**Root cause:** When a constructor returns a different object (`return o`), private methods/accessors get installed on that object. If the same object is returned by two `new` calls, the second should throw TypeError (PrivateMethodOrAccessorAdd step 3: if entry is not empty, throw).

### 10j: Scope edge cases (12 scenarios)

**Tests:** `scope-gen-meth-paramsbody-var-open.js`, `scope-name-lex-open-heritage.js`, `scope-static-gen-meth-paramsbody-var-open.js`.

**Root cause:** Generator method parameter/body var scoping, class name binding scope in heritage expressions.

### 10k: Miscellaneous (remaining ~54 scenarios)

- Computed accessor name with `yield`/`in` expressions (12)
- Non-configurable method defineProperty error (8)
- Default param `arguments` ref in generator methods (8)
- Field/generator ASI grammar (8)
- Getter with default parameter parse error (4)
- `static` as valid instance field name (8)
- Static field anonymous function name (4)
- Heritage expression arrow/async-arrow early errors (8)
- Private name in eval early errors (8)
- Cross-realm private brand checks (18) — requires `$262.createRealm()`
- Duplicate binding early error, strict arguments.callee, constructor-strict-by-default, numeric-property-names, prototype getter/setter, constructable-but-no-prototype (14)

---

## Priority Order

1. **Task 10a** (forbidden extensions, 80 scenarios) — likely a small fix in function creation
2. **Task 9** (yield* thenable delegation, 64 scenarios) — core promise resolution fix
3. **Task 10d** (yield-identifier-strict, 32 scenarios) — parser fix
4. **Task 10c** (super property, 36 scenarios) — mixed parser/runtime
5. **Task 10e** (subclass, 25 scenarios) — mixed
6. **Task 10f** (await, 22 scenarios) — parser fix
7. **Task 10g** (static init, 17 scenarios) — parser/runtime
8. **Task 10h** (private accessor mismatch, 8 scenarios) — parser fix
9. **Task 10i** (private double init, 8 scenarios) — runtime fix
10. **Task 6** (field init order, 4 scenarios) — complex refactor
11. **Task 10b** (decorators, 40 scenarios) — large feature, lowest priority
12. **Task 10k** (misc, ~54 scenarios) — case-by-case

Cross-realm tests (18 scenarios) require `$262.createRealm()` which is a test harness feature not yet implemented. These are lower priority.
