// RUN: 1
// ENV: FORCE_COLOR=0
// ENV: NO_COLOR=1
// FORCE_COLOR=0 keeps the report plain.
const error = new Error('plain');
error.code = 'E_PLAIN';
throw error;
