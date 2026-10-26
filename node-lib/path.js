'use strict';

const { ObjectAssign } = primordials;
const { isWindows } = require('internal/util');

const win32 = {};
ObjectAssign(
  win32,
  require('internal/path/win32_a')(win32),
  require('internal/path/win32_b')(win32),
);
const posix = require('internal/path/posix');

posix.win32 = win32.win32 = win32;
posix.posix = win32.posix = posix;

win32._makeLong = win32.toNamespacedPath;
posix._makeLong = posix.toNamespacedPath;

module.exports = isWindows ? win32 : posix;
