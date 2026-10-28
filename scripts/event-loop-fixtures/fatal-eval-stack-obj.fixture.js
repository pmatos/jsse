// RUN: 1
// MODE: eval
// An object with a string stack is decorated like an error, and then reported
// as a plain object.
throw { stack: 'custom' };
