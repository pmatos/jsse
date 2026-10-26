// Copyright Joyent, Inc. and other Node contributors.
//
// Permission is hereby granted, free of charge, to any person obtaining a
// copy of this software and associated documentation files (the
// "Software"), to deal in the Software without restriction, including
// without limitation the rights to use, copy, modify, merge, publish,
// distribute, sublicense, and/or sell copies of the Software, and to permit
// persons to whom the Software is furnished to do so, subject to the
// following conditions:
//
// The above copyright notice and this permission notice shall be included
// in all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS
// OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN
// NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM,
// DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR
// OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE
// USE OR OTHER DEALINGS IN THE SOFTWARE.

'use strict';

// Derived from Node v26.8.2 lib/os.js (MIT; license above). `internalBinding('os')`
// is the native `__host_os` object, seeded as `internal/host/os`. Deviations:
// HOME/USERPROFILE and TMPDIR/TMP/TEMP are read from `process.env` at call time
// (the OS environment is never mutated, see the ADR), and platform/arch come
// from the binding, which `process.platform`/`arch` share (#1227).

const {
  ArrayPrototypePush,
  ObjectDefineProperties,
  ObjectDefineProperty,
  ObjectFreeze,
  StringPrototypeEndsWith,
  StringPrototypeSlice,
  SymbolToPrimitive,
} = primordials;

const binding = require('internal/host/os');
const {
  codes: {
    ERR_SYSTEM_ERROR,
  },
} = require('internal/errors');
const { getCIDR } = require('internal/os/cidr');
const { validateInt32 } = require('internal/validators');

const platformName = binding.getPlatform();
const archName = binding.getArch();
const isWindows = platformName === 'win32';

const {
  getAvailableParallelism,
  getFreeMem,
  getTotalMem,
  getUserInfo,
  getPriority: _getPriority,
  getOSInformation: _getOSInformation,
  setPriority: _setPriority,
} = binding;

function getCheckedFunction(fn) {
  function wrappedFn() {
    const ctx = {};
    const ret = fn(ctx);
    if (ret === undefined) {
      throw new ERR_SYSTEM_ERROR(ctx);
    }
    return ret;
  }
  wrappedFn.withoutStackTrace = wrappedFn;
  return wrappedFn;
}

const {
  0: type,
  1: version,
  2: release,
  3: machine,
} = _getOSInformation();

const getHomeDirectory = getCheckedFunction((ctx) => {
  const home = process.env[isWindows ? 'USERPROFILE' : 'HOME'];
  return home !== undefined ? home : binding.getHomeDirectory(ctx);
});
const getHostname = getCheckedFunction(binding.getHostname);
const getInterfaceAddresses = getCheckedFunction(binding.getInterfaceAddresses);
const getUptime = getCheckedFunction(binding.getUptime);

/**
 * @returns {string}
 */
const getOSRelease = () => release;
/**
 * @returns {string}
 */
const getOSType = () => type;
/**
 * @returns {string}
 */
const getOSVersion = () => version;
/**
 * @returns {string}
 */
const getMachine = () => machine;

getAvailableParallelism[SymbolToPrimitive] = () => getAvailableParallelism();
getFreeMem[SymbolToPrimitive] = () => getFreeMem();
getHostname[SymbolToPrimitive] = () => getHostname();
getOSVersion[SymbolToPrimitive] = () => getOSVersion();
getOSType[SymbolToPrimitive] = () => getOSType();
getOSRelease[SymbolToPrimitive] = () => getOSRelease();
getMachine[SymbolToPrimitive] = () => getMachine();
getHomeDirectory[SymbolToPrimitive] = () => getHomeDirectory();
getTotalMem[SymbolToPrimitive] = () => getTotalMem();
getUptime[SymbolToPrimitive] = () => getUptime();

const kEndianness = binding.isBigEndian ? 'BE' : 'LE';

/**
 * @returns {[number, number, number]}
 */
function loadavg() {
  const avg = binding.getLoadAvg();
  return [avg[0], avg[1], avg[2]];
}

/**
 * Returns an array of objects containing information about each
 * logical CPU core.
 * @returns {Array<{
 *  model: string,
 *  speed: number,
 *  times: {
 *    user: number,
 *    nice: number,
 *    sys: number,
 *    idle: number,
 *    irq: number,
 *  },
 * }>}
 */
function cpus() {
  const data = binding.getCPUs() || [];
  const result = [];
  let i = 0;
  while (i < data.length) {
    ArrayPrototypePush(result, {
      model: data[i++],
      speed: data[i++],
      times: {
        user: data[i++],
        nice: data[i++],
        sys: data[i++],
        idle: data[i++],
        irq: data[i++],
      },
    });
  }
  return result;
}

/**
 * @returns {string}
 */
function arch() {
  return archName;
}
arch[SymbolToPrimitive] = () => archName;

/**
 * @returns {string}
 */
function platform() {
  return platformName;
}
platform[SymbolToPrimitive] = () => platformName;

/**
 * @returns {string}
 */
function tmpdir() {
  if (isWindows) {
    const path = process.env.TEMP ||
           process.env.TMP ||
           (process.env.SystemRoot || process.env.windir) + '\\temp';

    if (path.length > 1 && path[path.length - 1] === '\\' && path[path.length - 2] !== ':') {
      return StringPrototypeSlice(path, 0, -1);
    }

    return path;
  }

  const path = process.env.TMPDIR || process.env.TMP || process.env.TEMP || '/tmp';
  if (path.length > 1 && StringPrototypeEndsWith(path, '/')) {
    return StringPrototypeSlice(path, 0, -1);
  }
  return path;
}
tmpdir[SymbolToPrimitive] = () => tmpdir();

/**
 * @returns {'BE' | 'LE'}
 */
function endianness() {
  return kEndianness;
}
endianness[SymbolToPrimitive] = () => kEndianness;

/**
 * @returns {Record<string, Array<{
 *  address: string,
 *  netmask: string,
 *  family: 'IPv4' | 'IPv6',
 *  mac: string,
 *  internal: boolean,
 *  scopeid: number,
 *  cidr: string | null,
 * }>>}
 */
function networkInterfaces() {
  const data = getInterfaceAddresses();
  const result = {};

  if (data === undefined)
    return result;
  for (let i = 0; i < data.length; i += 7) {
    const name = data[i];
    const entry = {
      address: data[i + 1],
      netmask: data[i + 2],
      family: data[i + 3],
      mac: data[i + 4],
      internal: data[i + 5],
      cidr: getCIDR(data[i + 1], data[i + 2], data[i + 3]),
    };
    const scopeid = data[i + 6];
    if (scopeid !== -1)
      entry.scopeid = scopeid;

    const existing = result[name];
    if (existing !== undefined)
      ArrayPrototypePush(existing, entry);
    else
      result[name] = [entry];
  }

  return result;
}

/**
 * @param {number} [pid]
 * @param {number} [priority]
 * @returns {void}
 */
function setPriority(pid, priority) {
  if (priority === undefined) {
    priority = pid;
    pid = 0;
  }

  validateInt32(pid, 'pid');
  validateInt32(priority, 'priority', -20, 19);

  const ctx = {};

  if (_setPriority(pid, priority, ctx) !== 0)
    throw new ERR_SYSTEM_ERROR(ctx);
}

/**
 * @param {number} [pid]
 * @returns {number}
 */
function getPriority(pid) {
  if (pid === undefined)
    pid = 0;
  else
    validateInt32(pid, 'pid');

  const ctx = {};
  const priority = _getPriority(pid, ctx);

  if (priority === undefined)
    throw new ERR_SYSTEM_ERROR(ctx);

  return priority;
}

/**
 * @param {{ encoding?: string }} [options] If `encoding` is set to
 *   `'buffer'`, the `username`, `shell`, and `homedir` values will
 *   be `Buffer` instances.
 * @returns {{
 *   uid: number,
 *   gid: number,
 *   username: string,
 *   homedir: string,
 *   shell: string | null,
 * }}
 */
function userInfo(options) {
  if (typeof options !== 'object')
    options = null;

  const asBuffer = options !== null && options.encoding === 'buffer';
  const ctx = {};
  const user = getUserInfo(ctx, asBuffer);

  if (user === undefined)
    throw new ERR_SYSTEM_ERROR(ctx);

  if (asBuffer) {
    const { Buffer } = binding;
    user.username = Buffer.from(user.username);
    user.homedir = Buffer.from(user.homedir);
    if (user.shell !== null)
      user.shell = Buffer.from(user.shell);
  }

  return user;
}

function constantTable(flat) {
  const table = { __proto__: null };
  for (let i = 0; i < flat.length; i += 2) {
    ObjectDefineProperty(table, flat[i], {
      __proto__: null,
      value: flat[i + 1],
      writable: false,
      enumerable: true,
      configurable: false,
    });
  }
  return table;
}

const hostConstants = binding.getConstants();
const constants = constantTable([
  'UV_UDP_REUSEADDR', 4,
]);
ObjectDefineProperties(constants, {
  dlopen: { __proto__: null, value: constantTable(hostConstants[2]), enumerable: true, writable: true, configurable: true },
  errno: { __proto__: null, value: constantTable(hostConstants[1]), enumerable: true, writable: true, configurable: true },
  signals: { __proto__: null, value: constantTable(hostConstants[0]), enumerable: true, writable: true, configurable: true },
  priority: {
    __proto__: null,
    value: constantTable([
      'PRIORITY_LOW', 19,
      'PRIORITY_BELOW_NORMAL', 10,
      'PRIORITY_NORMAL', 0,
      'PRIORITY_ABOVE_NORMAL', -7,
      'PRIORITY_HIGH', -14,
      'PRIORITY_HIGHEST', -20,
    ]),
    enumerable: true,
    writable: true,
    configurable: true,
  },
});

module.exports = {
  arch,
  availableParallelism: getAvailableParallelism,
  cpus,
  endianness,
  freemem: getFreeMem,
  getPriority,
  homedir: getHomeDirectory,
  hostname: getHostname,
  loadavg,
  networkInterfaces,
  platform,
  release: getOSRelease,
  setPriority,
  tmpdir,
  totalmem: getTotalMem,
  type: getOSType,
  userInfo,
  uptime: getUptime,
  version: getOSVersion,
  machine: getMachine,
};

ObjectFreeze(constants.signals);

ObjectDefineProperties(module.exports, {
  constants: {
    __proto__: null,
    configurable: false,
    enumerable: true,
    value: constants,
  },

  EOL: {
    __proto__: null,
    configurable: true,
    enumerable: true,
    writable: false,
    value: isWindows ? '\r\n' : '\n',
  },

  devNull: {
    __proto__: null,
    configurable: true,
    enumerable: true,
    writable: false,
    value: isWindows ? '\\\\.\\nul' : '/dev/null',
  },
});
