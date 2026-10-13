# Array/object literals, regular expression literals and closures in the bytecode VM

Status: accepted (#959); amends ADR-2026-10-12-2214 (decision 4, "Name
inference") and its "native lowerings" list.

## Context

After per-node fallback (ADR-2026-10-12-2214) and block environments
(ADR-2026-10-13-0632), the constructs that kept compiled bodies on the
tree-walker were array literals, object literals, function/arrow expressions
(and the assignments/declarators that name them) and regular expression
literals: `FALLBACK expression:Array` / `expression:Object` /
`expression:Function` / `expression:ArrowFunction`, `anonymous function
assignment`, `anonymous function initializer` and `literal`. In splay one
object literal accounted for 756k hand-offs and 11.4M tree-walker work units;
in gbemu, Box2D and earley-boyer whole bodies were `fallback-only body`
because a literal was the only thing they did.

## Decision

1. **Build once, from operands.** An array literal's elements and an object
   literal's property values are evaluated left to right onto the operand
   stack (each already rooted in `gc_bytecode_roots`), and one op allocates
   the result from them: `NewArray n`, `NewObject shape`. Nothing observes a
   half-built literal, so no write barrier is involved and the same code as
   the tree-walker (`define_literal_property`, `finish_object_literal`) does the
   defining. Operands are popped (unrooted) LIFO right before the allocation,
   and nothing collects at allocation time.
2. **Chunking.** At most 256 values sit above a literal on the operand stack:
   a longer array continues with `ArrayAppend n`, a longer object with
   `ObjectAppend shape` on the object the first chunk allocated (a `__proto__`
   or method in a later chunk is applied to that object; the
   `ObjectAppend`/`ArrayAppend` write goes through `borrow_mut`, which is the
   generational barrier).
3. **Constant arrays are templates.** An array literal whose elements are all
   number/string/boolean/`null`/BigInt literals (a negated number literal
   included) and elisions is `NewArrayConst idx`: each evaluation builds a
   fresh array from the per-chunk template. Elisions are only supported there
   (a non-constant literal with a hole, a spread element, a computed key, an
   object spread keep the literal on the tree-walker, with their own
   `FALLBACK` reason: `array literal: elision`, `array literal: spread`,
   `object literal: computed key`, `object literal: spread`).
4. **Shapes.** `NewObject`'s per-chunk `ObjectShape` lists the statically keyed
   properties (identifier, string and number keys, shorthand, methods,
   accessors, `__proto__: v`) with their interned keys and a
   `LiteralPropertySpec` (kind, "plain init" for `__proto__`, method, "name the
   value afterwards"). The tree-walker's own loop builds the same spec per
   property, so both paths share one definition of the semantics (accessor
   merging, duplicate keys, prototype setting, home-object wrapper
   environments).
5. **Closures from templates.** `MakeClosure idx flags` creates a closure of a
   function or arrow literal from a `FunctionTemplate`: the literal's
   `Rc<FunctionLiteralInfo>` computed once at compile time on the original
   node (a cloned AST node would have an empty info cell), the shared `Body`,
   flags, source text and the NamedEvaluation name. The tree-walker's two
   `Expression::Function`/`ArrowFunction` arms call the same
   `Interpreter::make_closure(ClosureSpec, env, is_method, inferred_name)`.
   The closure's environment is the VM's current (innermost block)
   environment, so `CopyIterEnv`'s capture test still sees the closure's
   reference.
6. **NamedEvaluation.** An assignment (`=` only), `var`/`let`/`const`
   declarator or static object property whose value is an anonymous function
   or arrow literal passes the name to `MakeClosure` (accessors pass `get x` /
   `set x`), which stores it as the function's `name` property at creation,
   the value `SetFunctionName` would have written afterwards. The literal's
   own `JsFunction::User.name` stays `None`, as before. An anonymous *class*
   value keeps the whole assignment/declaration on the tree-walker.
7. **Regular expression literals.** `NewRegExp idx` allocates a fresh RegExp
   object from a template holding the pre-converted `source` and `flags`
   strings (sec-regular-expression-literals-runtime-semantics-evaluation:
   every evaluation creates a new object).
8. **Script goal.** The new lowerings are goal-independent, so a Script-goal
   body that only held literals/closures now also compiles instead of
   bailing whole.

## Consequences

- `FALLBACK expression:Array|Object|Function|ArrowFunction`, `anonymous
  function assignment` and `anonymous function initializer` read 0 on the
  JetStream drivers (see `docs/perf/2026-10-13/959-*.md`); bodies whose only
  content was a literal are no longer `fallback-only`.
- Object literals with computed keys or spread, array literals with spread
  or (non-constant) elisions, and class expressions still hand the literal to
  the tree-walker.
- Arrows capture `new.target` exactly as before (`make_closure` reads
  `interp.new_target`), and template literals are unaffected.
- `create_array_with_holes` (the tree-walker's array literal) now appends
  `length` through the same fresh-length path as `create_array`.
