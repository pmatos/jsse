// Derived from Node.js lib/path.js (v26.8.2); MIT license, see internal/path/common.js.
'use strict';

const {
  FunctionPrototypeBind,
  StringPrototypeCharCodeAt,
  StringPrototypeSlice,
} = primordials;
const {
  CHAR_DOT,
  CHAR_COLON,
} = require('internal/constants');
const {
  validateString,
} = require('internal/validators');
const {
  isPathSeparator,
  isWindowsDeviceRoot,
  _format,
  lazyMatchGlobPattern,
} = require('internal/path/common');

module.exports = function installWin32Parts(win32) {
  return {
    dirname(path) {
      validateString(path, 'path');
      const len = path.length;
      if (len === 0)
        return '.';
      let rootEnd = -1;
      let offset = 0;
      const code = StringPrototypeCharCodeAt(path, 0);

      if (len === 1) {
        return isPathSeparator(code) ? path : '.';
      }

      if (isPathSeparator(code)) {

        rootEnd = offset = 1;

        if (isPathSeparator(StringPrototypeCharCodeAt(path, 1))) {
          let j = 2;
          let last = j;
          while (j < len &&
                 !isPathSeparator(StringPrototypeCharCodeAt(path, j))) {
            j++;
          }
          if (j < len && j !== last) {
            last = j;
            while (j < len &&
                   isPathSeparator(StringPrototypeCharCodeAt(path, j))) {
              j++;
            }
            if (j < len && j !== last) {
              last = j;
              while (j < len &&
                     !isPathSeparator(StringPrototypeCharCodeAt(path, j))) {
                j++;
              }
              if (j === len) {
                return path;
              }
              if (j !== last) {

                rootEnd = offset = j + 1;
              }
            }
          }
        }
      } else if (isWindowsDeviceRoot(code) &&
                 StringPrototypeCharCodeAt(path, 1) === CHAR_COLON) {
        rootEnd =
          len > 2 && isPathSeparator(StringPrototypeCharCodeAt(path, 2)) ? 3 : 2;
        offset = rootEnd;
      }

      let end = -1;
      let matchedSlash = true;
      for (let i = len - 1; i >= offset; --i) {
        if (isPathSeparator(StringPrototypeCharCodeAt(path, i))) {
          if (!matchedSlash) {
            end = i;
            break;
          }
        } else {
          matchedSlash = false;
        }
      }

      if (end === -1) {
        if (rootEnd === -1)
          return '.';

        end = rootEnd;
      }
      return StringPrototypeSlice(path, 0, end);
    },

    basename(path, suffix) {
      if (suffix !== undefined)
        validateString(suffix, 'suffix');
      validateString(path, 'path');
      let start = 0;
      let end = -1;
      let matchedSlash = true;

      if (path.length >= 2 &&
          isWindowsDeviceRoot(StringPrototypeCharCodeAt(path, 0)) &&
          StringPrototypeCharCodeAt(path, 1) === CHAR_COLON) {
        start = 2;
      }

      if (suffix !== undefined && suffix.length > 0 && suffix.length <= path.length) {
        if (suffix === path)
          return '';
        let extIdx = suffix.length - 1;
        let firstNonSlashEnd = -1;
        for (let i = path.length - 1; i >= start; --i) {
          const code = StringPrototypeCharCodeAt(path, i);
          if (isPathSeparator(code)) {
            if (!matchedSlash) {
              start = i + 1;
              break;
            }
          } else {
            if (firstNonSlashEnd === -1) {
              matchedSlash = false;
              firstNonSlashEnd = i + 1;
            }
            if (extIdx >= 0) {
              if (code === StringPrototypeCharCodeAt(suffix, extIdx)) {
                if (--extIdx === -1) {
                  end = i;
                }
              } else {
                extIdx = -1;
                end = firstNonSlashEnd;
              }
            }
          }
        }

        if (start === end)
          end = firstNonSlashEnd;
        else if (end === -1)
          end = path.length;
        return StringPrototypeSlice(path, start, end);
      }
      for (let i = path.length - 1; i >= start; --i) {
        if (isPathSeparator(StringPrototypeCharCodeAt(path, i))) {
          if (!matchedSlash) {
            start = i + 1;
            break;
          }
        } else if (end === -1) {
          matchedSlash = false;
          end = i + 1;
        }
      }

      if (end === -1)
        return '';
      return StringPrototypeSlice(path, start, end);
    },

    extname(path) {
      validateString(path, 'path');
      let start = 0;
      let startDot = -1;
      let startPart = 0;
      let end = -1;
      let matchedSlash = true;
      let preDotState = 0;

      if (path.length >= 2 &&
          StringPrototypeCharCodeAt(path, 1) === CHAR_COLON &&
          isWindowsDeviceRoot(StringPrototypeCharCodeAt(path, 0))) {
        start = startPart = 2;
      }

      for (let i = path.length - 1; i >= start; --i) {
        const code = StringPrototypeCharCodeAt(path, i);
        if (isPathSeparator(code)) {
          if (!matchedSlash) {
            startPart = i + 1;
            break;
          }
          continue;
        }
        if (end === -1) {
          matchedSlash = false;
          end = i + 1;
        }
        if (code === CHAR_DOT) {
          if (startDot === -1)
            startDot = i;
          else if (preDotState !== 1)
            preDotState = 1;
        } else if (startDot !== -1) {
          preDotState = -1;
        }
      }

      if (startDot === -1 ||
          end === -1 ||
          preDotState === 0 ||
          (preDotState === 1 &&
           startDot === end - 1 &&
           startDot === startPart + 1)) {
        return '';
      }
      return StringPrototypeSlice(path, startDot, end);
    },

    format: FunctionPrototypeBind(_format, null, '\\'),

    parse(path) {
      validateString(path, 'path');

      const ret = { root: '', dir: '', base: '', ext: '', name: '' };
      if (path.length === 0)
        return ret;

      const len = path.length;
      let rootEnd = 0;
      let code = StringPrototypeCharCodeAt(path, 0);

      if (len === 1) {
        if (isPathSeparator(code)) {
          ret.root = ret.dir = path;
          return ret;
        }
        ret.base = ret.name = path;
        return ret;
      }
      if (isPathSeparator(code)) {

        rootEnd = 1;
        if (isPathSeparator(StringPrototypeCharCodeAt(path, 1))) {
          let j = 2;
          let last = j;
          while (j < len &&
                 !isPathSeparator(StringPrototypeCharCodeAt(path, j))) {
            j++;
          }
          if (j < len && j !== last) {
            last = j;
            while (j < len &&
                   isPathSeparator(StringPrototypeCharCodeAt(path, j))) {
              j++;
            }
            if (j < len && j !== last) {
              last = j;
              while (j < len &&
                     !isPathSeparator(StringPrototypeCharCodeAt(path, j))) {
                j++;
              }
              if (j === len) {
                rootEnd = j;
              } else if (j !== last) {
                rootEnd = j + 1;
              }
            }
          }
        }
      } else if (isWindowsDeviceRoot(code) &&
                 StringPrototypeCharCodeAt(path, 1) === CHAR_COLON) {
        if (len <= 2) {
          ret.root = ret.dir = path;
          return ret;
        }
        rootEnd = 2;
        if (isPathSeparator(StringPrototypeCharCodeAt(path, 2))) {
          if (len === 3) {
            ret.root = ret.dir = path;
            return ret;
          }
          rootEnd = 3;
        }
      }
      if (rootEnd > 0)
        ret.root = StringPrototypeSlice(path, 0, rootEnd);

      let startDot = -1;
      let startPart = rootEnd;
      let end = -1;
      let matchedSlash = true;
      let i = path.length - 1;

      let preDotState = 0;

      for (; i >= rootEnd; --i) {
        code = StringPrototypeCharCodeAt(path, i);
        if (isPathSeparator(code)) {
          if (!matchedSlash) {
            startPart = i + 1;
            break;
          }
          continue;
        }
        if (end === -1) {
          matchedSlash = false;
          end = i + 1;
        }
        if (code === CHAR_DOT) {
          if (startDot === -1)
            startDot = i;
          else if (preDotState !== 1)
            preDotState = 1;
        } else if (startDot !== -1) {
          preDotState = -1;
        }
      }

      if (end !== -1) {
        if (startDot === -1 ||
            preDotState === 0 ||
            (preDotState === 1 &&
             startDot === end - 1 &&
             startDot === startPart + 1)) {
          ret.base = ret.name = StringPrototypeSlice(path, startPart, end);
        } else {
          ret.name = StringPrototypeSlice(path, startPart, startDot);
          ret.base = StringPrototypeSlice(path, startPart, end);
          ret.ext = StringPrototypeSlice(path, startDot, end);
        }
      }

      if (startPart > 0 && startPart !== rootEnd)
        ret.dir = StringPrototypeSlice(path, 0, startPart - 1);
      else
        ret.dir = ret.root;

      return ret;
    },

    matchesGlob(path, pattern) {
      return lazyMatchGlobPattern()(path, pattern, true);
    },

    sep: '\\',
    delimiter: ';',
    win32: null,
    posix: null,
  };
};
