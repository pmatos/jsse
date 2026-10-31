# Node host mode: V8 early-error wording, export-from link errors, retried loads

Status: accepted (#1484, #1523, #1524, #1487), epic #1214. Extends
`2026-10-31-1600-node-host-native-frames-import-sites.md`.

## Context

The parser words only "this token is not allowed here" like V8. Every other
early error keeps jsse's text (`Expected function name`, `Unterminated string
literal`, `Numeric separator must be exactly one underscore`), `eval` and
`new Function` print `SyntaxError: ` twice, `export { x } from` link errors
have the old text and no source line, and a module that failed to load returns
the first caller's error object to every later `import()`.

## Decision

1. **One table, three entry points.** V8's text for a message that needs no
   parser state lives in `parser/v8_messages.rs::static_text`. `Parser::error`
   applies it first; `ParseError::js` / `js_bare` apply it where a `ParseError`
   becomes a JavaScript `SyntaxError` (`eval`, `Function`, `$262.evalScript`, the
   module loader), which also covers errors built without a `Parser` (the lexer,
   regexp literals); `vm.Script` applies it to its failure. It is not applied in
   `create_error`: the same raw string can mean something else for a builtin
   (`new RegExp("a", "gg")` is `Invalid flags supplied to RegExp constructor`).
   Messages that depend on the offending token are reworded by
   `Parser::v8_text`, which reads the current token; the other `Expected ...`
   messages become `Unexpected <token>`. A table keyed on the message leaves the
   ~230 `self.error` sites, and the size-capped parser files, untouched.
2. **Default mode is untouched**, including the doubled prefix of `eval` and
   `Function` messages: `ParseError::js` strips it only when the stack hooks are
   installed. Changing default mode text needs a ruling.
3. **Export-from link errors.** `ExportSpecifier` records the offset and the
   width of `local as exported`; `link_reexport` runs the existing resolution
   under `with_import_name`, so the error shares the import path
   (`no_export_error`, `ambiguous_export_error`, `link_error`).
4. **Retried loads.** A module that failed with an error that is not a link
   error is removed from the registry (`forget_failed_load`), so the next
   `import()` resolves and parses again and builds a fresh error with the new
   caller's frames, as Node does. Link errors are marked with a hidden slot and
   keep the cached error object; so do evaluation errors.
5. **`import(spec)` position.** A specifier that is a variable or a dotted
   property chain makes the call report that load (V8's latest emitted
   position), and `await import(spec)` no longer retargets it to the `await`.

## Known gaps

See `docs/agents/node-host-syntax-link-1523.md` and
`ts-suite/node-syntax-link-1523-known-failures.txt`: rest-element messages
inside declarations, a few `Unexpected token` positions inside object patterns,
`import()` nested in other expressions, the source-line header of an uncaught
error raised inside a builtin, and parser bugs that accept invalid sources.
