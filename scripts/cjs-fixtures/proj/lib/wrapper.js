'use strict';
module.exports = {
  thisIsExports: this === exports,
  thisIsModuleExports: this === module.exports,
  args: arguments.length,
  strict: (function () { return this === undefined; })(),
  names: [typeof exports, typeof require, typeof module, typeof __filename, typeof __dirname],
  ret: typeof (() => 1),
};
