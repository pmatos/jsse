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

const { join, resolve, isAbsolute, dirname } = require('path');
const { isWindows } = require('internal/util');
const { GlobSync, lazyMinimatch } = require('internal/fs/glob_sync');

class Glob extends GlobSync {
  async _isDirectory(path, stat, pattern) {
    if (stat?.isDirectory()) {
      return true;
    }
    if (!stat?.isSymbolicLink()) {
      return false;
    }
    if (this._followSymlinks) {
      return !!(await this._cache.followStat(path))?.isDirectory();
    }
    return pattern.hasSeenSymlinks;
  }
  async _nextRealpaths(path, isDirectory, pattern) {
    if (!this._followSymlinks || !isDirectory) {
      return pattern.realpaths;
    }
    const real = await this._cache.realpath(path);
    if (real === null) {
      return pattern.realpaths;
    }
    const realpaths = cloneSet(pattern.realpaths);
    realpaths.add(real);
    return realpaths;
  }
  async _isCyclic(path, isDirectory, pattern) {
    if (!this._followSymlinks || !isDirectory) {
      return false;
    }
    const real = await this._cache.realpath(path);
    return real !== null && pattern.realpaths.has(real);
  }
  async* glob() {
    ArrayPrototypePush(this._queue, { __proto__: null, path: '.', patterns: this._patterns });
    while (this._queue.length > 0) {
      const item = ArrayPrototypePop(this._queue);
      for (let i = 0; i < item.patterns.length; i++) {
        yield* this._iterateSubpatterns(item.path, item.patterns[i]);
      }
      this._subpatterns
        .forEach((patterns, path) => ArrayPrototypePush(this._queue, { __proto__: null, path, patterns }));
      this._subpatterns.clear();
    }
  }
  async* _iterateSubpatterns(path, pattern) {
    const seen = this._cache.add(path, pattern);
    if (seen) {
      return;
    }
    const fullpath = resolve(this._root, path);
    const stat = await this._cache.stat(fullpath);
    const last = pattern.last;
    const isDirectory = await this._isDirectory(fullpath, stat, pattern);
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
      const stat = await this._cache.stat(join(fullpath, p));
      if (stat && (p || isDirectory)) {
        const result = join(path, p);
        if (!this._results.has(result)) {
          if (this._results.add(result)) {
            yield this._withFileTypes ? stat : result;
          }
        }
      }
      if (pattern.indexes.size === 1 && pattern.indexes.has(last)) {
        return;
      }
    } else if (isLast && pattern.at(-1) === lazyMinimatch().GLOBSTAR &&
     (path !== '.' || pattern.at(0) === '.' || (last === 0 && stat))) {
      // If pattern ends with **, add to results
      // if path is ".", add it only if pattern starts with "." or pattern is exactly "**"
      if (!this._results.has(path)) {
        if (this._results.add(path)) {
          yield this._withFileTypes ? stat : path;
        }
      }
    }

    if (!isDirectory || await this._isCyclic(fullpath, isDirectory, pattern)) {
      return;
    }

    const nextRealpaths = await this._nextRealpaths(fullpath, isDirectory, pattern);

    let children;
    const firstPattern = pattern.indexes.size === 1 && pattern.at(pattern.indexes.values().next().value);
    if (typeof firstPattern === 'string') {
      const stat = await this._cache.stat(join(fullpath, firstPattern));
      if (stat) {
        stat.name = firstPattern;
        children = [stat];
      } else {
        return;
      }
    } else {
      children = await this._cache.readdir(fullpath);
    }

    for (let i = 0; i < children.length; i++) {
      const entry = children[i];
      const entryPath = join(path, entry.name);
      const entryFullpath = join(fullpath, entry.name);
      this._cache.addToStatCache(entryFullpath, entry);
      const entryIsDirectory = entry.isDirectory() ||
        (this._followSymlinks &&
         entry.isSymbolicLink() &&
         !!(await this._cache.followStat(entryFullpath))?.isDirectory());

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
            if (!this._results.has(entryPath) && this._results.add(entryPath)) {
              yield this._withFileTypes ? entry : entryPath;
            }
          }

          // Any pattern after ** is also a potential pattern
          // so we can already test it here
          if (nextMatches && nextIndex === last && !isLast) {
            // If next pattern is the last one, add to results
            if (!this._results.has(entryPath) && this._results.add(entryPath)) {
              yield this._withFileTypes ? entry : entryPath;
            }
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
                if (!this._results.has(path)) {
                  if (this._results.add(path)) {
                    yield this._withFileTypes ? this._cache.statSync(fullpath) : path;
                  }
                }
              }
              if (!this._cache.seen(path, pattern, nextIndex) || !this._cache.seen(parent, pattern, nextIndex)) {
                this._cache.add(parent, pattern.child(new SafeSet().add(nextIndex)));
                if (!this._results.has(parent)) {
                  if (this._results.add(parent)) {
                    yield this._withFileTypes ? this._cache.statSync(join(this._root, parent)) : parent;
                  }
                }
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
              if (!this._results.has(entryPath)) {
                if (this._results.add(entryPath)) {
                  yield this._withFileTypes ? entry : entryPath;
                }
              }
            } else {
              subPatterns.add(nextIndex + 1);
            }
          }
        }
        if (typeof current === 'object' && pattern.test(index, entry.name)) {
          // If current pattern is a regex that matches entry name (e.g *.js)
          // add next pattern to potential patterns, or to results if it's the last pattern
          if (index === last) {
            if (!this._results.has(entryPath)) {
              if (this._results.add(entryPath)) {
                yield this._withFileTypes ? entry : entryPath;
              }
            }
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

async function arrayFromAsync(iterable) {
  const result = [];
  for await (const item of iterable) result.push(item);
  return result;
}

function glob(pattern, options, callback) {
  if (typeof options === 'function') {
    callback = options;
    options = undefined;
  }
  if (typeof callback !== 'function') {
    const { codes } = require('internal/errors');
    throw new codes.ERR_INVALID_ARG_TYPE('cb', 'Function', callback);
  }
  PromisePrototypeThen(arrayFromAsync(new Glob(pattern, options).glob()),
                       (res) => callback(null, res), callback);
}

function globSync(pattern, options) {
  return new Glob(pattern, options).globSync();
}

module.exports = { __proto__: null, Glob, glob, globSync };
