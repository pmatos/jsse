// RUN: 1
// MODE: eval
// ENV: FORCE_COLOR=1
const error = new SyntaxError('colour eval');
error.detail = { n: 1 };
throw error;
