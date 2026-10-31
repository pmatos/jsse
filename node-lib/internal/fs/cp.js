'use strict';

// The asynchronous `fs.cp` / `fs.promises.cp` (issue #1503): lib/internal/fs/cp/cp.js
// of Node, run on the promise API. Every stat/copyfile/mkdir/symlink/utimes/chmod
// is a request to the worker pool and each directory entry is awaited, so the
// event loop keeps turning during a large copy; `filter` may return a promise.
// Its error shapes are those of Node's asynchronous copy, which differ from
// `cpSync`'s (the synchronous copy is a C++ walk in Node).

const { support: S } = require('internal/host/fs');
const { dirname, isAbsolute, join, parse, resolve, sep } = require('path');

const EEXIST = 17;
const EISDIR = 21;
const EINVAL = 22;
const ENOTDIR = 20;

let fsp;
function api() {
  if (fsp === undefined) fsp = require('internal/fs/promises').exports;
  return fsp;
}

function cpError(code, prefix, message, path, errno, errname) {
  return S.systemError(code, prefix, { message, path, syscall: 'cp', errno, code: errname });
}

const eexist = (dest) => cpError(
  'ERR_FS_CP_EEXIST', 'Target already exists', `${dest} already exists`, dest, EEXIST, 'EEXIST');
const einval = (message, dest) => cpError(
  'ERR_FS_CP_EINVAL', 'Invalid src or dest', message, dest, EINVAL, 'EINVAL');

async function cpFn(src, dest, opts) {
  const { skipped, srcStat, destStat } = await checkPaths(src, dest, opts);
  if (skipped) return;
  await checkParentPaths(src, srcStat, dest);
  return checkParentDir(destStat, src, dest, opts);
}

async function checkPaths(src, dest, opts) {
  if (opts.filter && !(await opts.filter(src, dest))) return { skipped: true };
  const [srcStat, destStat] = await getStats(src, dest, opts);
  if (destStat) {
    if (areIdentical(srcStat, destStat)) {
      throw einval('src and dest cannot be the same', dest);
    }
    if (srcStat.isDirectory() && !destStat.isDirectory()) {
      throw cpError('ERR_FS_CP_DIR_TO_NON_DIR', 'Cannot overwrite non-directory with directory',
                    `cannot overwrite non-directory ${dest} with directory ${src}`,
                    dest, EISDIR, 'EISDIR');
    }
    if (!srcStat.isDirectory() && destStat.isDirectory()) {
      throw cpError('ERR_FS_CP_NON_DIR_TO_DIR', 'Cannot overwrite directory with non-directory',
                    `cannot overwrite directory ${dest} with non-directory ${src}`,
                    dest, ENOTDIR, 'ENOTDIR');
    }
  }
  if (srcStat.isDirectory() && isSrcSubdir(src, dest)) {
    throw einval(`cannot copy ${src} to a subdirectory of self ${dest}`, dest);
  }
  return { skipped: false, srcStat, destStat };
}

function areIdentical(srcStat, destStat) {
  return destStat.ino && destStat.dev && destStat.ino === srcStat.ino &&
    destStat.dev === srcStat.dev;
}

function getStats(src, dest, opts) {
  const { stat, lstat } = api();
  const statFunc = opts.dereference ?
    (file) => stat(file, { bigint: true }) :
    (file) => lstat(file, { bigint: true });
  return Promise.all([
    statFunc(src),
    statFunc(dest).then(undefined, (err) => {
      if (err.code === 'ENOENT') return null;
      throw err;
    }),
  ]);
}

async function checkParentDir(destStat, src, dest, opts) {
  const destParent = dirname(dest);
  const dirExists = await pathExists(destParent);
  if (!dirExists) await api().mkdir(destParent, { recursive: true });
  return getStatsForCopy(destStat, src, dest, opts);
}

function pathExists(dest) {
  return api().stat(dest).then(
    () => true,
    (err) => (err.code === 'ENOENT' ? false : Promise.reject(err)));
}

// Recursively check if dest parent is a subdirectory of src, by inode.
async function checkParentPaths(src, srcStat, dest) {
  const srcParent = resolve(dirname(src));
  const destParent = resolve(dirname(dest));
  if (destParent === srcParent || destParent === parse(destParent).root) return;
  let destStat;
  try {
    destStat = await api().stat(destParent, { bigint: true });
  } catch (err) {
    if (err.code === 'ENOENT') return;
    throw err;
  }
  if (areIdentical(srcStat, destStat)) {
    throw einval(`cannot copy ${src} to a subdirectory of self ${dest}`, dest);
  }
  return checkParentPaths(src, srcStat, destParent);
}

const normalizePathToArray = (path) => resolve(path).split(sep).filter(Boolean);

// Whether dest is a subdirectory of src, comparing the path strings only.
function isSrcSubdir(src, dest) {
  const destArr = normalizePathToArray(dest);
  return normalizePathToArray(src).every((cur, i) => destArr[i] === cur);
}

async function getStatsForCopy(destStat, src, dest, opts) {
  const srcStat = await (opts.dereference ? api().stat : api().lstat)(src);
  if (srcStat.isDirectory() && opts.recursive) {
    return onDir(srcStat, destStat, src, dest, opts);
  } else if (srcStat.isDirectory()) {
    throw S.systemError('ERR_FS_EISDIR', 'Path is a directory', {
      message: `${src} is a directory (not copied)`,
      path: src,
      syscall: 'cp',
      errno: EISDIR,
      code: 'EISDIR',
    });
  } else if (srcStat.isFile() || srcStat.isCharacterDevice() || srcStat.isBlockDevice()) {
    return onFile(srcStat, destStat, src, dest, opts);
  } else if (srcStat.isSymbolicLink()) {
    return onLink(destStat, src, dest, opts);
  } else if (srcStat.isSocket()) {
    throw cpError('ERR_FS_CP_SOCKET', 'Cannot copy a socket file',
                  `cannot copy a socket file: ${dest}`, dest, EINVAL, 'EINVAL');
  } else if (srcStat.isFIFO()) {
    throw cpError('ERR_FS_CP_FIFO_PIPE', 'Cannot copy a FIFO pipe',
                  `cannot copy a FIFO pipe: ${dest}`, dest, EINVAL, 'EINVAL');
  }
  throw cpError('ERR_FS_CP_UNKNOWN', 'Cannot copy an unknown file type',
                `cannot copy an unknown file type: ${dest}`, dest, EINVAL, 'EINVAL');
}

function onFile(srcStat, destStat, src, dest, opts) {
  if (!destStat) return copyOne(srcStat, src, dest, opts);
  return mayCopyFile(srcStat, src, dest, opts);
}

async function mayCopyFile(srcStat, src, dest, opts) {
  if (opts.force) {
    await api().unlink(dest);
    return copyOne(srcStat, src, dest, opts);
  } else if (opts.errorOnExist) {
    throw eexist(dest);
  }
}

async function copyOne(srcStat, src, dest, opts) {
  const { chmod, copyFile } = api();
  await copyFile(src, dest, opts.mode);
  if (opts.preserveTimestamps) {
    // The file must be writable before its timestamps are set.
    if ((srcStat.mode & 0o200) === 0) await chmod(dest, srcStat.mode | 0o200);
    await setDestTimestamps(src, dest);
  }
  return chmod(dest, srcStat.mode);
}

async function setDestTimestamps(src, dest) {
  // The atime of the first stat was changed by reading the file.
  const updated = await api().stat(src);
  return api().utimes(dest, updated.atime, updated.mtime);
}

async function onDir(srcStat, destStat, src, dest, opts) {
  if (!destStat) return mkDirAndCopy(srcStat.mode, src, dest, opts);
  if (opts.errorOnExist && !opts.force) throw eexist(dest);
  return copyDir(src, dest, opts);
}

async function mkDirAndCopy(srcMode, src, dest, opts) {
  await api().mkdir(dest);
  await copyDir(src, dest, opts);
  return api().chmod(dest, srcMode);
}

async function copyDir(src, dest, opts) {
  const dir = await api().opendir(src);
  for await (const { name } of dir) {
    const srcItem = join(src, name);
    const destItem = join(dest, name);
    const { destStat, skipped } = await checkPaths(srcItem, destItem, opts);
    if (!skipped) await getStatsForCopy(destStat, srcItem, destItem, opts);
  }
}

function isDirectory(path) {
  try {
    return (S.host.stat(path, 0, false)[1] & S.S_IFMT) === S.hostConstants.S_IFDIR;
  } catch {
    return false;
  }
}

async function onLink(destStat, src, dest, opts) {
  const { readlink, stat, symlink } = api();
  let resolvedSrc = await readlink(src);
  if (!opts.verbatimSymlinks && !isAbsolute(resolvedSrc)) {
    resolvedSrc = resolve(dirname(src), resolvedSrc);
  }
  const srcIsDir = isDirectory(src);
  const symlinkType = srcIsDir ? 'dir' : 'file';
  if (!destStat) return symlink(resolvedSrc, dest, symlinkType);
  let resolvedDest;
  try {
    resolvedDest = await readlink(dest);
  } catch (err) {
    // Dest exists and is a regular file or directory.
    if (err.code === 'EINVAL' || err.code === 'UNKNOWN') {
      return symlink(resolvedSrc, dest, symlinkType);
    }
    throw err;
  }
  if (!isAbsolute(resolvedDest)) resolvedDest = resolve(dirname(dest), resolvedDest);
  if (srcIsDir && isSrcSubdir(resolvedSrc, resolvedDest)) {
    throw einval(`cannot copy ${resolvedSrc} to a subdirectory of self ${resolvedDest}`, dest);
  }
  // Unlinking dest would remove src's contents if src is inside dest.
  const srcStat = await stat(src);
  if (srcStat.isDirectory() && isSrcSubdir(resolvedDest, resolvedSrc)) {
    throw cpError('ERR_FS_CP_SYMLINK_TO_SUBDIRECTORY', 'Cannot overwrite symlink in subdirectory of self',
                  `cannot overwrite ${resolvedDest} with ${resolvedSrc}`, dest, EINVAL, 'EINVAL');
  }
  await api().unlink(dest);
  return symlink(resolvedSrc, dest, symlinkType);
}

// fs.promises.cp.
async function cp(src, dest, options) {
  const prepared = S.cpPrepare(src, dest, options);
  return cpFn(prepared.src, prepared.dest, prepared.options);
}

// fs.cp: the promise settles through the callback.
let callbackified;
function cpCallback(src, dest, options, callback) {
  const prepared = S.cpPrepare(src, dest, options);
  if (callbackified === undefined) callbackified = require('util').callbackify(cpFn);
  callbackified(prepared.src, prepared.dest, prepared.options, callback);
}

module.exports = { cp, cpCallback, cpFn };
