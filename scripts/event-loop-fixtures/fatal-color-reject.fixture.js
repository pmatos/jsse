// RUN: 1
// ENV: FORCE_COLOR=1
// The fixed UnhandledPromiseRejection error is coloured like any other.
Promise.reject({ reason: 'nope' });
