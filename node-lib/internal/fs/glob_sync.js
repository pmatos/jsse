'use strict';

const {
  ArrayFrom,
  ArrayIsArray,
  ArrayPrototypeAt,
  ArrayPrototypeFlatMap,
  ArrayPrototypeMap,
  ArrayPrototypePop,
  ArrayPrototypePush,
  ArrayPrototypeSome,
  Promise,
  PromisePrototypeThen,
  SafeMap,
  SafeSet,
  StringPrototypeEndsWith,
} = primordials;

const {
  lstatSync,
  readdirSync,
  realpathSync,
  statSync,
} = require('fs');
const {
  lstat,
  readdir,
  realpath,
  stat,
} = require('fs/promises');
const { join, resolve, basename, isAbsolute, dirname } = require('path');
const { kEmptyObject, isWindows } = require('internal/util');
const {
  validateBoolean,
  validateObject,
  validateString,
  validateStringArray,
} = require('internal/validators');
const { support } = require('internal/host/fs');
const {
  codes: {
    ERR_INVALID_ARG_TYPE,
  },
} = require('internal/errors');
const assert = require('internal/assert');
const { createMatcher } = require('internal/path/glob');
const { toPathIfFileURL } = require('internal/url');
const DirentFromStats = support.stats.direntFromStats;

let minimatch;
function lazyMinimatch() {
  minimatch ??= require('internal/deps/minimatch/index');
  return minimatch;
}

async function getDirent(path) {
  let stat;
  try {
    stat = await lstat(path);
  } catch {
    return null;
  }
  return DirentFromStats(basename(path), stat, dirname(path));
}

function getDirentSync(path) {
  let stat;
  try {
    stat = lstatSync(path);
  } catch {
    return null;
  }
  return DirentFromStats(basename(path), stat, dirname(path));
}

function validateStringArrayOrFunction(value, name) {
  if (ArrayIsArray(value)) {
    for (let i = 0; i < value.length; ++i) {
      if (typeof value[i] !== 'string') {
        throw new ERR_INVALID_ARG_TYPE(`${name}[${i}]`, 'string', value[i]);
      }
    }
    return;
  }
  if (typeof value !== 'function') {
    throw new ERR_INVALID_ARG_TYPE(name, ['string[]', 'function'], value);
  }
}

function cloneSet(values) {
  const cloned = new SafeSet();
  for (const value of values) {
    cloned.add(value);
  }
  return cloned;
}

class Cache {
  #cache = new SafeMap();
  #statsCache = new SafeMap();
  #followStatsCache = new SafeMap();
  #readdirCache = new SafeMap();
  #realpathCache = new SafeMap();

  stat(path) {
    const cached = this.#statsCache.get(path);
    if (cached) {
      return cached;
    }
    const promise = getDirent(path);
    this.#statsCache.set(path, promise);
    return promise;
  }
  statSync(path) {
    const cached = this.#statsCache.get(path);
    // Do not return a promise from a sync function.
    if (cached && !(cached instanceof Promise)) {
      return cached;
    }
    const val = getDirentSync(path);
    this.#statsCache.set(path, val);
    return val;
  }
  followStat(path) {
    const cached = this.#followStatsCache.get(path);
    if (cached) {
      return cached;
    }
    const promise = PromisePrototypeThen(stat(path), null, () => null);
    this.#followStatsCache.set(path, promise);
    return promise;
  }
  followStatSync(path) {
    const cached = this.#followStatsCache.get(path);
    if (cached && !(cached instanceof Promise)) {
      return cached;
    }
    let val;
    try {
      val = statSync(path);
    } catch {
      val = null;
    }
    this.#followStatsCache.set(path, val);
    return val;
  }
  realpath(path) {
    const cached = this.#realpathCache.get(path);
    if (cached) {
      return cached;
    }
    const promise = PromisePrototypeThen(realpath(path), null, () => null);
    this.#realpathCache.set(path, promise);
    return promise;
  }
  realpathSync(path) {
    const cached = this.#realpathCache.get(path);
    if (cached && !(cached instanceof Promise)) {
      return cached;
    }
    let val;
    try {
      val = realpathSync(path);
    } catch {
      val = null;
    }
    this.#realpathCache.set(path, val);
    return val;
  }
  addToStatCache(path, val) {
    this.#statsCache.set(path, val);
  }
  async readdir(path) {
    const cached = this.#readdirCache.get(path);
    if (cached) {
      return cached;
    }
    const promise = PromisePrototypeThen(readdir(path, { __proto__: null, withFileTypes: true }), null, () => []);
    this.#readdirCache.set(path, promise);
    return promise;
  }
  readdirSync(path) {
    const cached = this.#readdirCache.get(path);
    if (cached) {
      return cached;
    }
    let val;
    try {
      val = readdirSync(path, { __proto__: null, withFileTypes: true });
    } catch {
      val = [];
    }
    this.#readdirCache.set(path, val);
    return val;
  }
  add(path, pattern) {
    let cache = this.#cache.get(path);
    if (!cache) {
      cache = new SafeSet();
      this.#cache.set(path, cache);
    }
    const originalSize = cache.size;
    pattern.indexes.forEach((index) => cache.add(pattern.cacheKey(index)));
    return cache.size !== originalSize + pattern.indexes.size;
  }
  seen(path, pattern, index) {
    return this.#cache.get(path)?.has(pattern.cacheKey(index));
  }
}

class Pattern {
  #pattern;
  #globStrings;
  indexes;
  symlinks;
  realpaths;
  last;

  constructor(pattern, globStrings, indexes, symlinks, realpaths = new SafeSet()) {
    this.#pattern = pattern;
    this.#globStrings = globStrings;
    this.indexes = indexes;
    this.symlinks = symlinks;
    this.realpaths = realpaths;
    this.last = pattern.length - 1;
  }

  isLast(isDirectory) {
    return this.indexes.has(this.last) ||
      (this.at(-1) === '' && isDirectory &&
      this.indexes.has(this.last - 1) && this.at(-2) === lazyMinimatch().GLOBSTAR);
  }
  isFirst() {
    return this.indexes.has(0);
  }
  get hasSeenSymlinks() {
    return ArrayPrototypeSome(ArrayFrom(this.indexes), (i) => !this.symlinks.has(i));
  }
  at(index) {
    return ArrayPrototypeAt(this.#pattern, index);
  }
  child(indexes, symlinks = new SafeSet(), realpaths = this.realpaths) {
    return new Pattern(this.#pattern, this.#globStrings, indexes, symlinks, realpaths);
  }
  test(index, path) {
    if (index > this.#pattern.length) {
      return false;
    }
    const pattern = this.#pattern[index];
    if (pattern === lazyMinimatch().GLOBSTAR) {
      return true;
    }
    if (typeof pattern === 'string') {
      return pattern === path;
    }
    if (typeof pattern?.test === 'function') {
      return pattern.test(path);
    }
    return false;
  }

  cacheKey(index) {
    let key = '';
    for (let i = index; i < this.#globStrings.length; i++) {
      key += this.#globStrings[i];
      if (i !== this.#globStrings.length - 1) {
        key += '/';
      }
    }
    return key;
  }
}

class ResultSet extends SafeSet {
  #root = '.';
  #isExcluded = () => false;

  setup(root, isExcludedFn) {
    this.#root = root;
    this.#isExcluded = isExcludedFn;
  }

  add(value) {
    if (this.#isExcluded(resolve(this.#root, value))) {
      return false;
    }
    super.add(value);
    return true;
  }
}

class GlobSync {
  _root;
  _exclude;
  _cache = new Cache();
  _results = new ResultSet();
  _queue = [];
  _subpatterns = new SafeMap();
  _patterns;
  _withFileTypes;
  _followSymlinks = false;
  _isExcluded = () => false;
  constructor(pattern, options = kEmptyObject) {
    validateObject(options, 'options');
    const { exclude, cwd, followSymlinks, withFileTypes } = options;
    this._root = toPathIfFileURL(cwd) ?? '.';
    if (followSymlinks != null) {
      validateBoolean(followSymlinks, 'options.followSymlinks');
      this._followSymlinks = followSymlinks;
    }
    this._withFileTypes = !!withFileTypes;
    if (exclude != null) {
      validateStringArrayOrFunction(exclude, 'options.exclude');
      if (ArrayIsArray(exclude)) {
        assert(typeof this._root === 'string');
        // Convert the path part of exclude patterns to absolute paths for
        // consistent comparison before instantiating matchers.
        const matchers = exclude
          .map((pattern) => resolve(this._root, pattern))
          .map((pattern) => createMatcher(pattern));
        this._isExcluded = (value) =>
          matchers.some((matcher) => matcher.match(value));
        this._results.setup(this._root, this._isExcluded);
      } else {
        this._exclude = exclude;
      }
    }
    let patterns;
    if (typeof pattern === 'object') {
      validateStringArray(pattern, 'patterns');
      patterns = pattern;
    } else {
      validateString(pattern, 'patterns');
      patterns = [pattern];
    }
    this.matchers = ArrayPrototypeMap(patterns, (pattern) => createMatcher(pattern));
    this._patterns = ArrayPrototypeFlatMap(this.matchers, (matcher) => ArrayPrototypeMap(matcher.set,
                                                                                         (pattern, i) => new Pattern(
                                                                                           pattern,
                                                                                           matcher.globParts[i],
                                                                                           new SafeSet().add(0),
                                                                                           new SafeSet(),
                                                                                         )));
  }

  globSync() {
    ArrayPrototypePush(this._queue, { __proto__: null, path: '.', patterns: this._patterns });
    while (this._queue.length > 0) {
      const item = ArrayPrototypePop(this._queue);
      for (let i = 0; i < item.patterns.length; i++) {
        this._addSubpatterns(item.path, item.patterns[i]);
      }
      this._subpatterns
        .forEach((patterns, path) => ArrayPrototypePush(this._queue, { __proto__: null, path, patterns }));
      this._subpatterns.clear();
    }
    return ArrayFrom(
      this._results,
      this._withFileTypes ? (path) => this._cache.statSync(
        isAbsolute(path) ?
          path :
          join(this._root, path),
      ) : undefined,
    );
  }
  _isDirectorySync(path, stat, pattern) {
    if (stat?.isDirectory()) {
      return true;
    }
    if (!stat?.isSymbolicLink()) {
      return false;
    }
    if (this._followSymlinks) {
      return !!this._cache.followStatSync(path)?.isDirectory();
    }
    return pattern.hasSeenSymlinks;
  }
  _nextRealpathsSync(path, isDirectory, pattern) {
    if (!this._followSymlinks || !isDirectory) {
      return pattern.realpaths;
    }
    const real = this._cache.realpathSync(path);
    if (real === null) {
      return pattern.realpaths;
    }
    const realpaths = cloneSet(pattern.realpaths);
    realpaths.add(real);
    return realpaths;
  }
  _isCyclicSync(path, isDirectory, pattern) {
    if (!this._followSymlinks || !isDirectory) {
      return false;
    }
    const real = this._cache.realpathSync(path);
    return real !== null && pattern.realpaths.has(real);
  }
  _addSubpattern(path, pattern) {
    if (this._isExcluded(path)) {
      return;
    }
    const fullpath = resolve(this._root, path);

    // If path is a directory, add trailing slash and test patterns again.
    if (this._isExcluded(`${fullpath}/`) && this._cache.statSync(fullpath).isDirectory()) {
      return;
    }

    if (this._exclude) {
      if (this._withFileTypes) {
        const stat = this._cache.statSync(path);
        if (stat !== null) {
          if (this._exclude(stat)) {
            return;
          }
        }
      } else if (this._exclude(path)) {
        return;
      }
    }
    if (!this._subpatterns.has(path)) {
      this._subpatterns.set(path, [pattern]);
    } else {
      ArrayPrototypePush(this._subpatterns.get(path), pattern);
    }
  }
  _addSubpatterns(path, pattern) {
    const seen = this._cache.add(path, pattern);
    if (seen) {
      return;
    }
    const fullpath = resolve(this._root, path);
    const stat = this._cache.statSync(fullpath);
    const last = pattern.last;
    const isDirectory = this._isDirectorySync(fullpath, stat, pattern);
    const isLast = pattern.isLast(isDirectory);
    const isFirst = pattern.isFirst();

    if (this._isExcluded(fullpath)) {
      return;
    }
    if (isFirst && isWindows && typeof pattern.at(0) === 'string' && StringPrototypeEndsWith(pattern.at(0), ':')) {
      // Absolute path, go to root
      this._addSubpattern(`${pattern.at(0)}\\`, pattern.child(new SafeSet().add(1)));
      return;
    }
    if (isFirst && pattern.at(0) === '') {
      // Absolute path, go to root
      this._addSubpattern('/', pattern.child(new SafeSet().add(1)));
      return;
    }
    if (isFirst && pattern.at(0) === '..') {
      // Start with .., go to parent
      this._addSubpattern('../', pattern.child(new SafeSet().add(1)));
      return;
    }
    if (isFirst && pattern.at(0) === '.') {
      // Start with ., proceed
      this._addSubpattern('.', pattern.child(new SafeSet().add(1)));
      return;
    }

    if (isLast && typeof pattern.at(-1) === 'string') {
      // Add result if it exists
      const p = pattern.at(-1);
      const stat = this._cache.statSync(join(fullpath, p));
      if (stat && (p || isDirectory)) {
        this._results.add(join(path, p));
      }
      if (pattern.indexes.size === 1 && pattern.indexes.has(last)) {
        return;
      }
    } else if (isLast && pattern.at(-1) === lazyMinimatch().GLOBSTAR &&
     (path !== '.' || pattern.at(0) === '.' || (last === 0 && stat))) {
      // If pattern ends with **, add to results
      // if path is ".", add it only if pattern starts with "." or pattern is exactly "**"
      this._results.add(path);
    }

    if (!isDirectory || this._isCyclicSync(fullpath, isDirectory, pattern)) {
      return;
    }

    const nextRealpaths = this._nextRealpathsSync(fullpath, isDirectory, pattern);

    let children;
    const firstPattern = pattern.indexes.size === 1 && pattern.at(pattern.indexes.values().next().value);
    if (typeof firstPattern === 'string') {
      const stat = this._cache.statSync(join(fullpath, firstPattern));
      if (stat) {
        stat.name = firstPattern;
        children = [stat];
      } else {
        return;
      }
    } else {
      children = this._cache.readdirSync(fullpath);
    }

    for (let i = 0; i < children.length; i++) {
      const entry = children[i];
      const entryPath = join(path, entry.name);
      const entryFullpath = join(fullpath, entry.name);
      this._cache.addToStatCache(entryFullpath, entry);
      const entryIsDirectory = entry.isDirectory() ||
        (this._followSymlinks && entry.isSymbolicLink() && !!this._cache.followStatSync(entryFullpath)?.isDirectory());

      const subPatterns = new SafeSet();
      const nSymlinks = new SafeSet();
      for (const index of pattern.indexes) {
        // For each child, check potential patterns
        const current = pattern.at(index);
        const nextIndex = index + 1;
        const next = pattern.at(nextIndex);
        const fromSymlink = !this._followSymlinks && pattern.symlinks.has(index);

        if (current === lazyMinimatch().GLOBSTAR) {
          const isDot = entry.name[0] === '.';
          const nextMatches = pattern.test(nextIndex, entry.name);

          let nextNonGlobIndex = nextIndex;
          while (pattern.at(nextNonGlobIndex) === lazyMinimatch().GLOBSTAR) {
            nextNonGlobIndex++;
          }

          const matchesDot = isDot && pattern.test(nextNonGlobIndex, entry.name);

          if ((isDot && !matchesDot) ||
              (this._exclude && this._exclude(this._withFileTypes ? entry : entry.name))) {
            continue;
          }
          if (!fromSymlink && entryIsDirectory) {
            // If directory, add ** to its potential patterns
            subPatterns.add(index);
          } else if (!fromSymlink && index === last) {
            // If ** is last, add to results
            this._results.add(entryPath);
          }

          // Any pattern after ** is also a potential pattern
          // so we can already test it here
          if (nextMatches && nextIndex === last && !isLast) {
            // If next pattern is the last one, add to results
            this._results.add(entryPath);
          } else if (nextMatches && entryIsDirectory) {
            // Pattern matched, meaning two patterns forward
            // are also potential patterns
            // e.g **/b/c when entry is a/b - add c to potential patterns
            subPatterns.add(index + 2);
          }
          if ((nextMatches || pattern.at(0) === '.') &&
           (entryIsDirectory || entry.isSymbolicLink()) && !fromSymlink) {
            // If pattern after ** matches, or pattern starts with "."
            // and entry is a directory or symlink, add to potential patterns
            subPatterns.add(nextIndex);
          }

          if (!this._followSymlinks && entry.isSymbolicLink()) {
            nSymlinks.add(index);
          }

          if (next === '..' && entryIsDirectory) {
            // In case pattern is "**/..",
            // both parent and current directory should be added to the queue
            // if this is the last pattern, add to results instead
            const parent = join(path, '..');
            if (nextIndex < last) {
              if (!this._subpatterns.has(path) && !this._cache.seen(path, pattern, nextIndex + 1)) {
                this._subpatterns.set(path, [pattern.child(new SafeSet().add(nextIndex + 1))]);
              }
              if (!this._subpatterns.has(parent) && !this._cache.seen(parent, pattern, nextIndex + 1)) {
                this._subpatterns.set(parent, [pattern.child(new SafeSet().add(nextIndex + 1))]);
              }
            } else {
              if (!this._cache.seen(path, pattern, nextIndex)) {
                this._cache.add(path, pattern.child(new SafeSet().add(nextIndex)));
                this._results.add(path);
              }
              if (!this._cache.seen(path, pattern, nextIndex) || !this._cache.seen(parent, pattern, nextIndex)) {
                this._cache.add(parent, pattern.child(new SafeSet().add(nextIndex)));
                this._results.add(parent);
              }
            }
          }
        }
        if (typeof current === 'string') {
          if (pattern.test(index, entry.name) && index !== last) {
            // If current pattern matches entry name
            // the next pattern is a potential pattern
            subPatterns.add(nextIndex);
          } else if (current === '.' && pattern.test(nextIndex, entry.name)) {
            // If current pattern is ".", proceed to test next pattern
            if (nextIndex === last) {
              this._results.add(entryPath);
            } else {
              subPatterns.add(nextIndex + 1);
            }
          }
        }
        if (typeof current === 'object' && pattern.test(index, entry.name)) {
          // If current pattern is a regex that matches entry name (e.g *.js)
          // add next pattern to potential patterns, or to results if it's the last pattern
          if (index === last) {
            this._results.add(entryPath);
          } else if (entryIsDirectory) {
            subPatterns.add(nextIndex);
          }
        }
      }
      if (subPatterns.size > 0) {
        // If there are potential patterns, add to queue
        this._addSubpattern(entryPath, pattern.child(subPatterns, nSymlinks, nextRealpaths));
      }
    }
  }


}

module.exports = {
  __proto__: null,
  GlobSync,
  validateStringArrayOrFunction,
  lazyMinimatch,
};
