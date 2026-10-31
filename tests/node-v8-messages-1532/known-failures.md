# Known failures: V8 messages under `--node` (#1532, #1512, #1511)

Differences from Node 26.8 that the fixtures here do not cover. Fixtures compare
stdout of `node` and `jsse --node`.

| Case | Difference | Issue |
| --- | --- | --- |
| operator (`1 in 1`, `x instanceof 1`, `Symbol() + ''`, `BigInt(1) + 1`), identifier (`x is not defined`), TDZ, const-assignment (`c = 2`), class heritage, `o.x.y += 1`, `o.x.y++`, `delete null.x`, `new a.b.c()` and `o?.a.b` positions | frame at the statement or call, V8 at the operator, identifier or sub-expression | #1511 |
| position of call spread, array pattern, nested pattern and `for-of` destructuring errors | frame at the statement, V8 at the operand or initializer | #1511 |
| `for (const [a] of [5])`, `var { a: [b] } = { a: 5 }`, `var { a: [b] } = { a: null }` | V8 prints `.for is not iterable` and `{(intermediate value)} is not iterable ...` (its CallPrinter state at that point) | #1545 |

## Why #1511 is open

A position needs a source offset on the node that throws. `Expression::Binary`,
`Assign`, `Update`, `Identifier` and the class heritage have none; adding a field
touches about 170 pattern sites of `Expression::Identifier` alone (83 of them in
`generator_transform.rs`) in files the token ratchet forbids growing, and the
bytecode VM would need a pc-keyed position for ops (`Add`, `In`, `InstanceOf`)
whose hot path cannot take a table lookup. Member accesses, calls and tagged
templates already carry one (`PropSiteId`, `CallSiteId`, `TemplateLiteral::id`).
Options: an `ExprBox` that holds the offset beside the `Expression` (+8 bytes
per boxed node) with the operator's offset on its right operand, or a
`Spanned<Expression>` wrapper introduced in one mechanical PR that moves the
oversized files' contents out first.
