# Direct UTF-16 String Substring Slicing

## Context

`JsString` stores ECMAScript strings as `Arc<Vec<u16>>`, but
`String.prototype.substring` currently converts its complete receiver to a Rust
UTF-8 `String`, encodes that value back to UTF-16, slices the requested range,
decodes the range to UTF-8, and finally encodes the result as a `JsString`.
Token-sized calls on a large receiver therefore do work proportional to the
receiver length and also replace any sliced lone surrogate with U+FFFD.

`String.prototype.substr` has the same receiver round trip. The adjacent
`String.prototype.slice` implementation demonstrates the intended local seam:
obtain a `JsString`, calculate UTF-16 code-unit indices, and copy only the result
range into `JsString::from_vec`.

## Design

Both `substring` and Annex B `substr` will obtain the receiver through
`this_js_string`, preserving `RequireObjectCoercible`/`ToString` behavior while
retaining the raw code units for primitive strings and String wrappers. Each
method will continue coercing its arguments in specification order with
`ToIntegerOrInfinity`:

- `substring` coerces `start`, then `end` when supplied; clamps both indices to
  `[0, len]`; swaps them when necessary; and copies `units[from..to]`.
- `substr` coerces `start`, adjusts negative and infinite starts as specified,
  then coerces `length` when supplied; clamps the length; and copies
  `units[start..end]`.

The result remains an independently allocated `JsString`. Shared substring
views or a rope representation are deliberately excluded because they would
change the engine-wide string representation and could retain large source
strings for tiny token results.

## Audit Scope

The remaining `this_string_value` users fall into search/comparison,
case/normalization/locale, trimming/formatting, repetition/padding/concatenation,
and Annex B HTML operations. Several could later benefit from broader raw
UTF-16 work, but only `substr` is the same index-only extraction pattern. This
change leaves the other methods untouched to keep the performance fix narrow.

## Validation

- Add a `test262-extra` regression that slices both halves of a surrogate pair
  with `substring`, checks swapped indices, and checks String-wrapper receivers.
- Run the existing test262 `substring` directory and Annex B `substr` directory;
  the latter already verifies surrogate-pair code-unit slicing.
- Use a release-mode microbenchmark that repeatedly takes token-sized
  substrings from a large receiver. Compare identical workloads before and
  after the change.
- Run the repository formatting, lint, release build/test, custom-test, and full
  test262 gates required by the project instructions.
