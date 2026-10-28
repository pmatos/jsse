// RUN: 1
// ENV: FORCE_COLOR=1
// FORCE_COLOR colours the inspected error: values, [cause], and the grey
// frames of Node's own modules.
const cause = new TypeError('inner');
const error = new RangeError('outer', { cause });
error.code = 'E_COLOR';
error.count = 5;
error.nested = { list: [1, 2], missing: undefined, tag: Symbol('t') };
throw error;
