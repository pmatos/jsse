// Derived from Node.js lib/path.js (v26.8.2); MIT license, see internal/path/common.js.
'use strict';

const {
  ArrayPrototypeJoin,
  ArrayPrototypePush,
  ArrayPrototypeSlice,
  StringPrototypeCharCodeAt,
  StringPrototypeIncludes,
  StringPrototypeIndexOf,
  StringPrototypeRepeat,
  StringPrototypeReplace,
  StringPrototypeSlice,
  StringPrototypeSplit,
  StringPrototypeToLowerCase,
} = primordials;
const {
  CHAR_DOT,
  CHAR_BACKWARD_SLASH,
  CHAR_COLON,
  CHAR_QUESTION_MARK,
} = require('internal/constants');
const {
  validateString,
} = require('internal/validators');
const {
  isPathSeparator,
  isPosixPathSeparator,
  isWindowsReservedName,
  isWindowsDeviceRoot,
  normalizeString,
  forwardSlashRegExp,
} = require('internal/path/common');
const {
  isWindows,
} = require('internal/util');

module.exports = function installWin32Core(win32) {
  return {
    resolve(...args) {
      let resolvedDevice = '';
      let resolvedTail = '';
      let resolvedAbsolute = false;

      for (let i = args.length - 1; i >= -1; i--) {
        let path;
        if (i >= 0) {
          path = args[i];
          validateString(path, `paths[${i}]`);

          if (path.length === 0) {
            continue;
          }
        } else if (resolvedDevice.length === 0) {
          path = process.cwd();
          if (args.length === 0 || ((args.length === 1 && (args[0] === '' || args[0] === '.')) &&
              isPathSeparator(StringPrototypeCharCodeAt(path, 0)))) {
            if (!isWindows) {
              path = StringPrototypeReplace(path, forwardSlashRegExp, '\\');
            }
            return path;
          }
        } else {
          path = process.env[`=${resolvedDevice}`] || process.cwd();

          if (path === undefined ||
              (StringPrototypeToLowerCase(StringPrototypeSlice(path, 0, 2)) !==
              StringPrototypeToLowerCase(resolvedDevice) &&
              StringPrototypeCharCodeAt(path, 2) === CHAR_BACKWARD_SLASH)) {
            path = `${resolvedDevice}\\`;
          }
        }

        const len = path.length;
        let rootEnd = 0;
        let device = '';
        let isAbsolute = false;
        const code = StringPrototypeCharCodeAt(path, 0);

        if (len === 1) {
          if (isPathSeparator(code)) {
            rootEnd = 1;
            isAbsolute = true;
          }
        } else if (isPathSeparator(code)) {

          isAbsolute = true;

          if (isPathSeparator(StringPrototypeCharCodeAt(path, 1))) {
            let j = 2;
            let last = j;
            while (j < len &&
                   !isPathSeparator(StringPrototypeCharCodeAt(path, j))) {
              j++;
            }
            if (j < len && j !== last) {
              const firstPart = StringPrototypeSlice(path, last, j);
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
                if (j === len || j !== last) {
                  if (firstPart !== '.' && firstPart !== '?') {
                    device =
                      `\\\\${firstPart}\\${StringPrototypeSlice(path, last, j)}`;
                    rootEnd = j;
                  } else {
                    device = `\\\\${firstPart}`;
                    rootEnd = 4;
                  }
                }
              }
            }
          } else {
            rootEnd = 1;
          }
        } else if (isWindowsDeviceRoot(code) &&
                    StringPrototypeCharCodeAt(path, 1) === CHAR_COLON) {
          device = StringPrototypeSlice(path, 0, 2);
          rootEnd = 2;
          if (len > 2 && isPathSeparator(StringPrototypeCharCodeAt(path, 2))) {
            isAbsolute = true;
            rootEnd = 3;
          }
        }

        if (device.length > 0) {
          if (resolvedDevice.length > 0) {
            if (StringPrototypeToLowerCase(device) !==
                StringPrototypeToLowerCase(resolvedDevice))
              continue;
          } else {
            resolvedDevice = device;
          }
        }

        if (resolvedAbsolute) {
          if (resolvedDevice.length > 0)
            break;
        } else {
          resolvedTail =
            `${StringPrototypeSlice(path, rootEnd)}\\${resolvedTail}`;
          resolvedAbsolute = isAbsolute;
          if (isAbsolute && resolvedDevice.length > 0) {
            break;
          }
        }
      }

      resolvedTail = normalizeString(resolvedTail, !resolvedAbsolute, '\\',
                                     isPathSeparator);

      return resolvedAbsolute ?
        `${resolvedDevice}\\${resolvedTail}` :
        `${resolvedDevice}${resolvedTail}` || '.';
    },

    normalize(path) {
      validateString(path, 'path');
      const len = path.length;
      if (len === 0)
        return '.';
      let rootEnd = 0;
      let device;
      let isAbsolute = false;
      const code = StringPrototypeCharCodeAt(path, 0);

      if (len === 1) {
        return isPosixPathSeparator(code) ? '\\' : path;
      }
      if (isPathSeparator(code)) {

        isAbsolute = true;

        if (isPathSeparator(StringPrototypeCharCodeAt(path, 1))) {
          let j = 2;
          let last = j;
          while (j < len &&
                 !isPathSeparator(StringPrototypeCharCodeAt(path, j))) {
            j++;
          }
          if (j < len && j !== last) {
            const firstPart = StringPrototypeSlice(path, last, j);
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
              if (j === len || j !== last) {
                if (firstPart === '.' || firstPart === '?') {
                  device = `\\\\${firstPart}`;
                  rootEnd = 4;
                  const colonIndex = StringPrototypeIndexOf(path, ':');
                  const possibleDevice = StringPrototypeSlice(path, 4, colonIndex + 1);
                  if (isWindowsReservedName(possibleDevice, possibleDevice.length - 1)) {
                    device = `\\\\?\\${possibleDevice}`;
                    rootEnd = 4 + possibleDevice.length;
                  }
                } else if (j === len) {
                  return `\\\\${firstPart}\\${StringPrototypeSlice(path, last)}\\`;
                } else {
                  device =
                    `\\\\${firstPart}\\${StringPrototypeSlice(path, last, j)}`;
                  rootEnd = j;
                }
              }
            }
          }
        } else {
          rootEnd = 1;
        }
      } else {
        const colonIndex = StringPrototypeIndexOf(path, ':');
        if (colonIndex > 0) {
          if (isWindowsDeviceRoot(code) && colonIndex === 1) {
            device = StringPrototypeSlice(path, 0, 2);
            rootEnd = 2;
            if (len > 2 && isPathSeparator(StringPrototypeCharCodeAt(path, 2))) {
              isAbsolute = true;
              rootEnd = 3;
            }
          } else if (isWindowsReservedName(path, colonIndex)) {
            device = StringPrototypeSlice(path, 0, colonIndex + 1);
            rootEnd = colonIndex + 1;

          }
        }
      }

      let tail = rootEnd < len ?
        normalizeString(StringPrototypeSlice(path, rootEnd),
                        !isAbsolute, '\\', isPathSeparator) :
        '';
      if (tail.length === 0 && !isAbsolute)
        tail = '.';
      if (tail.length > 0 &&
          isPathSeparator(StringPrototypeCharCodeAt(path, len - 1)))
        tail += '\\';
      if (!isAbsolute && device === undefined && StringPrototypeIncludes(path, ':')) {
        if (tail.length >= 2 &&
            isWindowsDeviceRoot(StringPrototypeCharCodeAt(tail, 0)) &&
            StringPrototypeCharCodeAt(tail, 1) === CHAR_COLON) {
          return `.\\${tail}`;
        }
        let index = StringPrototypeIndexOf(path, ':');

        do {
          if (index === len - 1 || isPathSeparator(StringPrototypeCharCodeAt(path, index + 1))) {
            return `.\\${tail}`;
          }
        } while ((index = StringPrototypeIndexOf(path, ':', index + 1)) !== -1);
      }
      const colonIndex = StringPrototypeIndexOf(path, ':');
      if (isWindowsReservedName(path, colonIndex)) {
        return `.\\${device ?? ''}${tail}`;
      }
      if (device === undefined) {
        return isAbsolute ? `\\${tail}` : tail;
      }
      return isAbsolute ? `${device}\\${tail}` : `${device}${tail}`;
    },

    isAbsolute(path) {
      validateString(path, 'path');
      const len = path.length;
      if (len === 0)
        return false;

      const code = StringPrototypeCharCodeAt(path, 0);
      return isPathSeparator(code) ||
        (len > 2 &&
        isWindowsDeviceRoot(code) &&
        StringPrototypeCharCodeAt(path, 1) === CHAR_COLON &&
        isPathSeparator(StringPrototypeCharCodeAt(path, 2)));
    },

    join(...args) {
      if (args.length === 0)
        return '.';

      const path = [];
      for (let i = 0; i < args.length; ++i) {
        const arg = args[i];
        validateString(arg, 'path');
        if (arg.length > 0) {
          ArrayPrototypePush(path, arg);
        }
      }

      if (path.length === 0)
        return '.';

      const firstPart = path[0];
      let joined = ArrayPrototypeJoin(path, '\\');

      let needsReplace = true;
      let slashCount = 0;
      if (isPathSeparator(StringPrototypeCharCodeAt(firstPart, 0))) {
        ++slashCount;
        const firstLen = firstPart.length;
        if (firstLen > 1 &&
            isPathSeparator(StringPrototypeCharCodeAt(firstPart, 1))) {
          ++slashCount;
          if (firstLen > 2) {
            if (isPathSeparator(StringPrototypeCharCodeAt(firstPart, 2)))
              ++slashCount;
            else {
              needsReplace = false;
            }
          }
        }
      }
      if (needsReplace) {
        while (slashCount < joined.length &&
               isPathSeparator(StringPrototypeCharCodeAt(joined, slashCount))) {
          slashCount++;
        }

        if (slashCount >= 2)
          joined = `\\${StringPrototypeSlice(joined, slashCount)}`;
      }

      const parts = [];
      let part = '';

      for (let i = 0; i < joined.length; i++) {
        if (joined[i] === '\\') {
          if (part) parts.push(part);
          part = '';
          while (i + 1 < joined.length && joined[i + 1] === '\\') i++;
        } else {
          part += joined[i];
        }
      }
      if (part) parts.push(part);

      if (parts.some((p) => {
        const colonIndex = StringPrototypeIndexOf(p, ':');
        return colonIndex !== -1 && isWindowsReservedName(p, colonIndex);
      })) {
        let result = '';
        for (let i = 0; i < joined.length; i++) {
          result += joined[i] === '/' ? '\\' : joined[i];
        }
        return result;
      }

      return win32.normalize(joined);
    },

    relative(from, to) {
      validateString(from, 'from');
      validateString(to, 'to');

      if (from === to)
        return '';

      const fromOrig = win32.resolve(from);
      const toOrig = win32.resolve(to);

      if (fromOrig === toOrig)
        return '';

      from = StringPrototypeToLowerCase(fromOrig);
      to = StringPrototypeToLowerCase(toOrig);

      if (from === to)
        return '';

      if (fromOrig.length !== from.length || toOrig.length !== to.length) {
        const fromSplit = StringPrototypeSplit(fromOrig, '\\');
        const toSplit = StringPrototypeSplit(toOrig, '\\');
        if (fromSplit[fromSplit.length - 1] === '') {
          fromSplit.pop();
        }
        if (toSplit[toSplit.length - 1] === '') {
          toSplit.pop();
        }

        const fromLen = fromSplit.length;
        const toLen = toSplit.length;
        const length = fromLen < toLen ? fromLen : toLen;

        let i;
        for (i = 0; i < length; i++) {
          if (StringPrototypeToLowerCase(fromSplit[i]) !== StringPrototypeToLowerCase(toSplit[i])) {
            break;
          }
        }

        if (i === 0) {
          return toOrig;
        } else if (i === length) {
          if (toLen > length) {
            return ArrayPrototypeJoin(ArrayPrototypeSlice(toSplit, i), '\\');
          }
          if (fromLen > length) {
            return StringPrototypeRepeat('..\\', fromLen - 1 - i) + '..';
          }
          return '';
        }

        return StringPrototypeRepeat('..\\', fromLen - i) + ArrayPrototypeJoin(ArrayPrototypeSlice(toSplit, i), '\\');
      }

      let fromStart = 0;
      while (fromStart < from.length &&
             StringPrototypeCharCodeAt(from, fromStart) === CHAR_BACKWARD_SLASH) {
        fromStart++;
      }
      let fromEnd = from.length;
      while (
        fromEnd - 1 > fromStart &&
        StringPrototypeCharCodeAt(from, fromEnd - 1) === CHAR_BACKWARD_SLASH
      ) {
        fromEnd--;
      }
      const fromLen = fromEnd - fromStart;

      let toStart = 0;
      while (toStart < to.length &&
             StringPrototypeCharCodeAt(to, toStart) === CHAR_BACKWARD_SLASH) {
        toStart++;
      }
      let toEnd = to.length;
      while (toEnd - 1 > toStart &&
             StringPrototypeCharCodeAt(to, toEnd - 1) === CHAR_BACKWARD_SLASH) {
        toEnd--;
      }
      const toLen = toEnd - toStart;

      const length = fromLen < toLen ? fromLen : toLen;
      let lastCommonSep = -1;
      let i = 0;
      for (; i < length; i++) {
        const fromCode = StringPrototypeCharCodeAt(from, fromStart + i);
        if (fromCode !== StringPrototypeCharCodeAt(to, toStart + i))
          break;
        else if (fromCode === CHAR_BACKWARD_SLASH)
          lastCommonSep = i;
      }

      if (i !== length) {
        if (lastCommonSep === -1)
          return toOrig;
      } else {
        if (toLen > length) {
          if (StringPrototypeCharCodeAt(to, toStart + i) ===
              CHAR_BACKWARD_SLASH) {
            return StringPrototypeSlice(toOrig, toStart + i + 1);
          }
          if (i === 2) {
            return StringPrototypeSlice(toOrig, toStart + i);
          }
        }
        if (fromLen > length) {
          if (StringPrototypeCharCodeAt(from, fromStart + i) ===
              CHAR_BACKWARD_SLASH) {
            lastCommonSep = i;
          } else if (i === 2) {
            lastCommonSep = 3;
          }
        }
        if (lastCommonSep === -1)
          lastCommonSep = 0;
      }

      let out = '';
      for (i = fromStart + lastCommonSep + 1; i <= fromEnd; ++i) {
        if (i === fromEnd ||
            StringPrototypeCharCodeAt(from, i) === CHAR_BACKWARD_SLASH) {
          out += out.length === 0 ? '..' : '\\..';
        }
      }

      toStart += lastCommonSep;

      if (out.length > 0)
        return `${out}${StringPrototypeSlice(toOrig, toStart, toEnd)}`;

      if (StringPrototypeCharCodeAt(toOrig, toStart) === CHAR_BACKWARD_SLASH)
        ++toStart;
      return StringPrototypeSlice(toOrig, toStart, toEnd);
    },

    toNamespacedPath(path) {
      if (typeof path !== 'string' || path.length === 0)
        return path;

      const resolvedPath = win32.resolve(path);

      if (resolvedPath.length <= 2)
        return path;

      if (StringPrototypeCharCodeAt(resolvedPath, 0) === CHAR_BACKWARD_SLASH) {
        if (StringPrototypeCharCodeAt(resolvedPath, 1) === CHAR_BACKWARD_SLASH) {
          const code = StringPrototypeCharCodeAt(resolvedPath, 2);
          if (code !== CHAR_QUESTION_MARK && code !== CHAR_DOT) {
            return `\\\\?\\UNC\\${StringPrototypeSlice(resolvedPath, 2)}`;
          }
        }
      } else if (
        isWindowsDeviceRoot(StringPrototypeCharCodeAt(resolvedPath, 0)) &&
        StringPrototypeCharCodeAt(resolvedPath, 1) === CHAR_COLON &&
        StringPrototypeCharCodeAt(resolvedPath, 2) === CHAR_BACKWARD_SLASH
      ) {
        return `\\\\?\\${resolvedPath}`;
      }

      return resolvedPath;
    },
  };
};
