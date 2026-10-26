'use strict';

function getLazy(initializer) {
  let value;
  let initialized = false;
  return function () {
    if (!initialized) {
      value = initializer();
      initialized = true;
    }
    return value;
  };
}

module.exports = {
  getLazy,
  isMacOS: process.platform === 'darwin',
  isWindows: process.platform === 'win32',
};
