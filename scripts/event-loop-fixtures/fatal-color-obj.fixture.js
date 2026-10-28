// RUN: 1
// ENV: FORCE_COLOR=1
// A thrown plain value is inspected with colours; the header and the
// trace hint of a primitive stay plain.
throw { a: 1, b: 'x', c: [null, true, undefined] };
