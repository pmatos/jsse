# Node host mode: V8 message text and engine-thrown error positions

Status: accepted (#1419), epic #1214. Extends
`2026-10-26-1000-node-host-v8-errors.md` and
`2026-10-27-0540-node-host-fatal-report.md`, whose "Known differences" named
this gap.

## Context

An error the engine throws itself (`null.x`, `o.f()` on a missing method) read
differently from Node's: `undefined is not a function` for `o.f is not a
function`, `(reading property)` for `(reading 'kk')`, and the frame (so the
fatal report's `file:line` header and caret) pointed at the throwing frame's
latest call or the function start instead of the access. Tools that print or
match these messages, and the oracle diffs of TypeScript's own output, see the
difference. The default environment is what test262 measures, so everything
here is `--node` only; a message is built from V8's wording only once the stack
hooks are installed (`NodeStackState::installed`).

## Decision

1. **Callee text for `is not a function` / `is not a constructor`.** V8
   reprints the callee from its AST. jsse reads it back from the source text at
   the call position the hooks already record (`node_host.call_pos`): the
   tree-walker and the VM both fail in `call_function` and
   `construct_from_evaluated`, where no AST is at hand, and the bytecode does
   not change. `builtins/node_host_callee.rs` scans the callee left from the
   call's position (or right from a `new`): names, `this`, `.`, `?.`,
   `[literal]`, call parentheses (`f(...)`) and a parenthesised chain, with a
   string key printed as `.key` as V8 does. Anything else (a callee that is
   not a name chain, a tagged template) returns `None` and keeps the engine's
   wording. `class_call_error` names the class (`Class constructor A cannot be
   invoked without 'new'`).
2. **Positions of member accesses.** V8 reports a failed `o.x` at the property
   name, a failed `o[k]` at the `[`; once a chain contains a call (`m.get(1).x`)
   a `.x` is reported at the `.`; a failed store `o.x = v` at the `=`. The
   parser records that as the second field of `PropSiteId` (offset plus one,
   like `CallSiteId`), so `Expression` does not grow. The tree-walker sets
   `node_host.call_pos` from it just before it throws (`note_member_pos`). The
   bytecode compiler records `(op offset, position)` per member op in
   `Chunk::member_pos` only while the hooks are on, so a default chunk keeps
   its bytes and an empty table; the VM looks the position up only when the
   op's base is null or undefined (`vm/member_pos.rs`), so the hot path pays
   one emptiness check and no extra op (cf. #1492). Both engines then capture
   the frame through the existing `call_pos` path, and the fatal report's
   header and caret follow.
3. **Value-based messages.** `Cannot read properties of null (reading 'kk')`
   names a primitive key (an object key is never converted, so it stays
   `(reading property)`); `Cannot use 'in' operator to search for 'a' in 1`,
   `Right-hand side of 'instanceof' is not an object` and `Class extends value
   1 is not a constructor or null` are V8's text (`node_host_messages.rs`). The
   tree-walker's compound assignment to a member of a null base now names the
   property in every mode, as the VM did.

## Known differences

See `tests/node-v8-messages-1419/known-failures.md`: builtin frames and the
wording of errors builtins throw, positions of operator, identifier and
heritage errors, destructuring and iteration messages, and callees that are not
a name chain.

## Verification

`scripts/run-node-v8-messages-oracle.sh` runs every fixture in
`tests/node-v8-messages-1419` on Node and on `jsse --node` (`--no-bytecode` for
the tree-walker) and compares each with the output Node produced (`--node`
rewrites it). `tests/node_v8_messages_1419.rs` replays them without Node under
the VM, the tree-walker and `JSSE_GC_STRESS=7`, and pins the default-mode
wording. `bytecode/tests_member_pos.rs` pins the empty table without hooks.
