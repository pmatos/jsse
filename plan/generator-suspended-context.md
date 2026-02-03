# Generator Suspended Context: True State Machine Implementation

## Problem Statement

The current generator implementation uses a **replay-based approach**:
1. On each `.next()` call, re-execute the function body from the start
2. Fast-forward through yields by incrementing `target_yield` counter
3. Return the value at the Nth yield point

**Limitations of replay approach:**
- Side effects before yields re-execute on every `.next()` call
- Control flow context is lost (e.g., being inside a try block)
- `throw()` and `return()` cannot execute finally blocks properly
- Expensive for generators with many yields or expensive pre-yield computations

**Example of broken behavior:**
```javascript
let count = 0;
function* g() {
    count++;      // Re-executes on every .next()!
    yield 1;
    count++;      // Re-executes on every .next()!
    yield 2;
}
let it = g();
it.next(); // count = 1 (correct)
it.next(); // count = 3 (wrong, should be 2)
it.next(); // count = 6 (wrong, should be 3)
```

## Solution: AST-to-State-Machine Transformation

Transform generator function bodies into state machines **before execution**. Each yield point becomes a state boundary, and local variables become persistent fields.

### High-Level Architecture

```
┌──────────────────┐     ┌─────────────────────┐     ┌──────────────────┐
│  Generator AST   │────▶│  State Machine      │────▶│  State Machine   │
│  (function* g)   │     │  Transformer        │     │  Execution       │
└──────────────────┘     └─────────────────────┘     └──────────────────┘
                                   │
                                   ▼
                         ┌─────────────────────┐
                         │ GeneratorStateMachine│
                         │ - states: Vec<State> │
                         │ - locals: Vec<String>│
                         │ - try_contexts: ...  │
                         └─────────────────────┘
```

### Phase 1: Analysis - Identify Yield Points and Local Variables

**Goal:** Scan the generator body to find:
1. All yield expressions and their positions
2. All local variable declarations
3. try/catch/finally block boundaries
4. Loop boundaries (for break/continue handling)

**Data Structures:**
```rust
struct GeneratorAnalysis {
    yield_points: Vec<YieldPoint>,
    local_vars: HashSet<String>,
    try_contexts: Vec<TryContext>,
    loop_contexts: Vec<LoopContext>,
}

struct YieldPoint {
    id: usize,
    location: AstLocation,
    inside_try: Option<usize>,  // Index into try_contexts
    inside_loop: Option<usize>, // Index into loop_contexts
}

struct TryContext {
    id: usize,
    has_catch: bool,
    has_finally: bool,
    finally_statements: Option<Vec<Statement>>,
    contains_yields: Vec<usize>, // YieldPoint ids inside this try
}

struct LoopContext {
    id: usize,
    loop_type: LoopType,        // For, While, DoWhile, ForIn, ForOf
    contains_yields: Vec<usize>,
}
```

**Implementation:** Walk the AST recursively, tracking context as we descend.

### Phase 2: Variable Lifting

**Goal:** Move all local variable declarations to the generator's persistent state.

**Transformation:**
```javascript
// Before:
function* g() {
    let x = 1;
    yield x;
    let y = x + 1;
    yield y;
}

// Conceptually becomes:
function* g() {
    // All vars lifted to persistent state
    this.$state = 0;
    this.$x = undefined;
    this.$y = undefined;
    // Body becomes state switch
}
```

**Challenges:**
- Block-scoped variables (`let`/`const`) need proper TDZ semantics
- Shadowing within blocks
- Destructuring declarations

**Data Structure:**
```rust
struct LiftedVariable {
    name: String,
    scope_id: usize,           // Which block scope it belongs to
    needs_tdz: bool,           // let/const need TDZ check
    initialized_in_state: Option<usize>, // Which state initializes it
}
```

### Phase 3: State Machine Representation

**Goal:** Represent the generator as a sequence of states with transitions.

**Data Structure:**
```rust
struct GeneratorStateMachine {
    states: Vec<GeneratorState>,
    local_vars: Vec<LiftedVariable>,
    params: Vec<Pattern>,
    try_finally_handlers: Vec<FinallyHandler>,
}

struct GeneratorState {
    id: usize,
    statements: Vec<Statement>,
    terminator: StateTerminator,
}

enum StateTerminator {
    Yield {
        value: Expression,
        resume_state: usize,
        resume_binding: Option<String>, // Where to store sent value
    },
    Return(Expression),
    Throw(Expression),
    Goto(usize),
    ConditionalGoto {
        condition: Expression,
        true_state: usize,
        false_state: usize,
    },
    LoopBack {
        condition: Expression,
        body_state: usize,
        exit_state: usize,
    },
    Switch {
        discriminant: Expression,
        cases: Vec<(Option<Expression>, usize)>, // None = default
        exit_state: usize,
    },
    EnterTry {
        try_state: usize,
        catch_state: Option<usize>,
        finally_state: Option<usize>,
        exit_state: usize,
    },
    ExitTry,
    Completed,
}

struct FinallyHandler {
    try_context_id: usize,
    finally_state: usize,
    statements: Vec<Statement>,
}
```

### Phase 4: AST-to-State Transformation Algorithm

**Goal:** Convert sequential statements with yields into state machine.

**Core Algorithm:**
```
function transform_statements(stmts, current_state, exit_state, try_stack):
    for each stmt in stmts:
        if contains_yield(stmt):
            split at yield points
            for each segment:
                if segment ends with yield:
                    add statements to current_state
                    create yield terminator -> new_state
                    current_state = new_state
                else:
                    add statements to current_state
        else:
            add stmt to current_state

    set current_state terminator -> exit_state
```

**Statement Transformations:**

1. **Variable Declaration with Yield:**
   ```javascript
   // Before:
   let x = yield 1;

   // After (two states):
   // State N: yield 1, resume to N+1
   // State N+1: x = $sent_value, continue
   ```

2. **If Statement with Yield:**
   ```javascript
   // Before:
   if (cond) { yield 1; } else { yield 2; }

   // After:
   // State N: ConditionalGoto(cond, N+1, N+2)
   // State N+1: yield 1, resume to N+3
   // State N+2: yield 2, resume to N+3
   // State N+3: continue
   ```

3. **While Loop with Yield:**
   ```javascript
   // Before:
   while (cond) { yield i; i++; }

   // After:
   // State N: ConditionalGoto(cond, N+1, N+2)
   // State N+1: yield i, resume to N+3
   // State N+2: exit loop
   // State N+3: i++, Goto N (loop back)
   ```

4. **Try/Catch/Finally with Yield:**
   ```javascript
   // Before:
   try { yield 1; } finally { cleanup(); }

   // After:
   // State N: EnterTry(try=N+1, finally=N+2, exit=N+3)
   // State N+1: yield 1, resume to N+2 (or N+3?)
   // State N+2: cleanup(), ExitTry
   // State N+3: continue
   ```

### Phase 5: Try/Finally Handling

**The Hard Part:** When `.throw()` or `.return()` is called on a suspended generator, we need to execute finally blocks.

**Solution: Finally Chain Tracking**

```rust
struct GeneratorInstance {
    state_machine: Rc<GeneratorStateMachine>,
    current_state: usize,
    local_values: HashMap<String, JsValue>,

    // Stack of try contexts we're currently inside
    try_stack: Vec<ActiveTryContext>,
}

struct ActiveTryContext {
    try_context_id: usize,
    entered_at_state: usize,
    finally_state: usize,
}
```

**Algorithm for `.return(value)`:**
```
1. If try_stack is empty:
   - Set state to Completed
   - Return {value, done: true}

2. Otherwise:
   - Pop top try_context from stack
   - Save return_value for after finally
   - Jump to finally_state
   - After finally completes:
     - If more try_contexts on stack, continue to their finally
     - Otherwise, set Completed, return saved value
```

**Algorithm for `.throw(error)`:**
```
1. If try_stack is empty:
   - Set state to Completed
   - Throw the error

2. If top try_context has catch:
   - Jump to catch_state with error
   - Catch may yield (becomes normal execution)

3. If top try_context has finally (no catch or catch didn't handle):
   - Jump to finally_state
   - After finally, propagate error to next try_context or throw
```

### Phase 6: yield* Delegation

**Challenge:** `yield* iterable` must:
1. Get iterator from iterable
2. Forward all yields to outer caller
3. Forward `.next(value)` to inner iterator
4. Forward `.throw(e)` to inner iterator (if it has `.throw`)
5. Forward `.return(v)` to inner iterator (if it has `.return`)
6. Return inner iterator's final value

**Solution: Delegation State**
```rust
enum GeneratorDelegation {
    None,
    Active {
        inner_iterator: JsValue,
        inner_next: JsValue,
        inner_return: Option<JsValue>,
        inner_throw: Option<JsValue>,
        resume_state: usize,
        result_binding: Option<String>,
    },
}
```

**State Machine for `let x = yield* iter`:**
```
// State N:
//   - Get iterator from iter
//   - Store in delegation
//   - Set resume_state = N+1, result_binding = "x"
//   - Enter delegation mode

// Delegation handling in generator_next:
if delegation.is_active:
    result = call inner_next with sent_value
    if result.done:
        exit delegation
        local_values[result_binding] = result.value
        jump to resume_state
    else:
        return result (yield to outer)
```

### Phase 7: Implementation Order

**Step 1: Generator Analysis Pass (1-2 days)**
- Implement `analyze_generator_body()`
- Identify yield points, local vars, try contexts
- Add tests for analysis correctness

**Step 2: Variable Lifting (1 day)**
- Transform declarations to use lifted storage
- Handle TDZ for let/const
- Handle destructuring

**Step 3: Basic State Machine Transform (2-3 days)**
- Sequential statements with yields
- Yield as terminator
- Resume from yield
- Test: side effects don't re-execute

**Step 4: Control Flow Transformation (2-3 days)**
- If/else with yields
- While/for loops with yields
- Break/continue in loops with yields
- Switch with yields

**Step 5: Try/Finally Handling (2-3 days)**
- EnterTry/ExitTry states
- Finally execution on normal exit
- Finally execution on throw
- Finally execution on return
- Test: try-catch-before-try.js and similar

**Step 6: yield* Delegation (1-2 days)**
- Delegation state tracking
- Forward next/throw/return to inner iterator
- Handle inner iterator completion

**Step 7: Async Generators (1-2 days)**
- Apply same transformation
- Handle await interleaving
- Promise wrapping at yield points

### Phase 8: Testing Strategy

**Unit Tests for Each Phase:**
```javascript
// Side effects don't re-execute
let count = 0;
function* g() { count++; yield 1; yield 2; }
let it = g(); it.next(); it.next(); it.next();
assert(count === 1);

// Local variables persist
function* g() { let x = 1; yield x; x++; yield x; }
let it = g();
assert(it.next().value === 1);
assert(it.next().value === 2);

// Try/finally on throw
let cleaned = false;
function* g() { try { yield 1; } finally { cleaned = true; } }
let it = g(); it.next(); it.throw(new Error());
assert(cleaned === true);

// Try/finally on return
let cleaned = false;
function* g() { try { yield 1; } finally { cleaned = true; } }
let it = g(); it.next(); it.return(42);
assert(cleaned === true);

// yield* delegation
function* inner() { yield 1; yield 2; return 3; }
function* outer() { let x = yield* inner(); yield x; }
let it = outer();
assert(it.next().value === 1);
assert(it.next().value === 2);
assert(it.next().value === 3);
```

**test262 Regression Testing:**
- Run after each phase
- Track generator test directories specifically
- Zero regressions allowed

### Phase 9: Complexity Analysis

**Transformation Complexity:**
- O(N) where N = number of AST nodes in generator body
- Single pass analysis + single pass transformation

**Runtime Complexity:**
- O(1) per state transition (no replay)
- Memory: O(V) where V = number of local variables

**Code Size Impact:**
- Estimate: +1000-1500 lines for transformation
- New file: `src/interpreter/generator_transform.rs`
- Modifications to `eval.rs` for execution

### Phase 10: Alternative Approaches Considered

**1. Bytecode Compilation**
- Pro: Industry standard approach
- Con: Massive undertaking, requires new VM architecture
- Decision: Not viable for tree-walking interpreter

**2. Continuation-Passing Style (CPS)**
- Pro: Mathematically elegant
- Con: All code must be CPS-transformed, complex
- Decision: Over-engineered for our needs

**3. Coroutine Libraries (stackful)**
- Pro: Language-level support exists (Rust async)
- Con: Unsafe, platform-dependent, doesn't match JS semantics
- Decision: Not suitable

**4. Improved Replay (current approach, enhanced)**
- Pro: Simpler, working now
- Con: Fundamental limitations remain
- Decision: Good enough for many cases, but not spec-compliant

**Recommendation:** AST-to-state-machine transformation is the right balance of:
- Correctness (handles all cases)
- Complexity (manageable implementation)
- Performance (no replay overhead)

## Success Criteria

1. **Side effects execute once:** Code before yields runs exactly once
2. **Variables persist:** Local state maintained without replay
3. **try/finally works:** throw() and return() execute finally blocks
4. **yield* works:** Proper delegation with next/throw/return forwarding
5. **No regressions:** All currently passing tests still pass
6. **Performance:** At least as fast as replay approach

## Timeline Estimate

| Phase | Duration | Cumulative |
|-------|----------|------------|
| Analysis | 2 days | 2 days |
| Variable Lifting | 1 day | 3 days |
| Basic State Machine | 3 days | 6 days |
| Control Flow | 3 days | 9 days |
| Try/Finally | 3 days | 12 days |
| yield* | 2 days | 14 days |
| Async Generators | 2 days | 16 days |
| Buffer | 2 days | 18 days |

**Total: ~3 weeks of focused work**

## Files to Create/Modify

**New Files:**
- `src/interpreter/generator_transform.rs` - State machine transformation
- `src/interpreter/generator_analysis.rs` - AST analysis pass

**Modified Files:**
- `src/interpreter/types.rs` - GeneratorStateMachine, GeneratorState types
- `src/interpreter/eval.rs` - Generator creation, execution engine
- `src/interpreter/gc.rs` - GC for state machine instances

## References

- [Regenerator](https://github.com/facebook/regenerator) - Babel's generator transform (JS→JS)
- [V8 Generator Implementation](https://v8.dev/blog/fast-async) - Bytecode approach
- [ECMA-262 §27.5](https://tc39.es/ecma262/#sec-generator-objects) - Generator Objects spec
- [ECMA-262 §15.5](https://tc39.es/ecma262/#sec-generator-function-definitions) - Generator definitions
