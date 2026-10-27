// RUN: 1
// Only an own stack makes a rejection reason error-like.
Promise.reject(Object.create(Error.prototype));
