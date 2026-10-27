"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
var _dot = require("./dot.cjs");
Object.keys(_dot).forEach(function (key) {
  if (key === "default" || key === "__esModule") return;
  if (key in exports && exports[key] === _dot[key]) return;
  Object.defineProperty(exports, key, {
    enumerable: true,
    get: function () {
      return _dot[key];
    }
  });
});
exports.own = 'own';
