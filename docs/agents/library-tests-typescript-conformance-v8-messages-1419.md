# V8 messages and engine-thrown error positions (#1419)

Epic #1214. Design: `docs/adr/2026-10-31-1200-node-host-v8-messages.md`.

## Running

- Oracle (needs `node` 26.8): `./scripts/run-node-v8-messages-oracle.sh`
  (`--no-bytecode` for the tree-walker, `--no-cross-check` to skip Node,
  `--node` to regenerate the expected files from Node).
- Without Node: `cargo test --profile release-nolto --test node_v8_messages_1419`
  (VM, tree-walker, `JSSE_GC_STRESS=7`, and the default-mode wording).
- Fixtures and expectations: `tests/node-v8-messages-1419/`. `messages.js`
  prints each case's message and top frame to stdout (`messages.out`); the
  other `*.js` are uncaught-error fixtures whose fatal report is compared
  (`*.stderr`); `eval-*.js` run as `-e`.
- Remaining differences: `tests/node-v8-messages-1419/known-failures.md`.

## Rules

- Everything is gated on the stack hooks (`node_host.stack.installed`); the
  bytecode of a default chunk and `Chunk::member_pos` stay unchanged/empty
  (`bytecode/tests_member_pos.rs`).
- A new message case goes into `messages.js`, then regenerate `messages.out`
  with `--node`; add it to `messageOnly` if its position is a known failure.
