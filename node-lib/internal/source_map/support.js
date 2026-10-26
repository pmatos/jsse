'use strict';

// Source map support (#1220), after Node's
// lib/internal/source_map/source_map_cache.js. jsse does not rewrite stack
// traces, so the flags only decide which loaded CommonJS files
// `findSourceMap` answers for: a file counts as compiled "while enabled" when
// it first shows up in `Module._cache` with the flag on (see the ADR).

const {
  JSONParse,
  ObjectCreate,
  ObjectKeys,
  RegExpPrototypeExec,
  StringPrototypeEndsWith,
  StringPrototypeIncludes,
  StringPrototypeSlice,
  StringPrototypeSplit,
  StringPrototypeStartsWith,
} = primordials;

const fs = require('fs');
const path = require('path');
const { validateBoolean, validateObject } = require('internal/validators');

const state = { enabled: false, nodeModules: false, generatedCode: false };
const known = ObjectCreate(null);
const eligible = ObjectCreate(null);
const found = ObjectCreate(null);

const kLineBreak = new RegExp('\\r\\n|[\\n\\r' + String.fromCharCode(0x2028, 0x2029) + ']');
const kSourceMappingURL = /\/\/[#@]\s+sourceMappingURL=([^\s'"]+)/g;

function scanCache() {
  const { Module } = require('internal/modules/cjs/loader');
  const cache = Module._cache;
  const gone = ObjectKeys(known);
  for (let i = 0; i < gone.length; i++) {
    const file = gone[i];
    if (cache[file] === undefined) {
      delete known[file];
      delete eligible[file];
      delete found[file];
    }
  }
  const files = ObjectKeys(cache);
  for (let i = 0; i < files.length; i++) {
    const file = files[i];
    if (known[file]) continue;
    known[file] = true;
    if (!state.enabled || StringPrototypeEndsWith(file, '.json')) continue;
    if (!state.nodeModules && StringPrototypeIncludes(file, '/node_modules/')) continue;
    eligible[file] = true;
  }
}

function getSourceMapsSupport() {
  return { __proto__: null, enabled: state.enabled, nodeModules: state.nodeModules, generatedCode: state.generatedCode };
}

function setSourceMapsSupport(enabled, options = { __proto__: null }) {
  validateBoolean(enabled, 'enabled');
  validateObject(options, 'options');
  const { nodeModules = false, generatedCode = false } = options;
  validateBoolean(nodeModules, 'options.nodeModules');
  validateBoolean(generatedCode, 'options.generatedCode');
  scanCache();
  state.enabled = enabled;
  state.nodeModules = nodeModules;
  state.generatedCode = generatedCode;
}

function setSourceMapsEnabled(enabled) {
  validateBoolean(enabled, 'enabled');
  setSourceMapsSupport(enabled, { __proto__: null, nodeModules: enabled, generatedCode: enabled });
}

function sourcesToAbsolute(baseURL, data) {
  const { URL } = require('internal/url');
  data.sources = data.sources.map((source) => new URL((data.sourceRoot || '') + source, baseURL).href);
  data.sourceRoot = '';
  return data;
}

function lastSourceMappingURL(content) {
  let url;
  let match;
  while ((match = RegExpPrototypeExec(kSourceMappingURL, content)) !== null) url = match[1];
  return url;
}

function mapFromDataUrl(sourceURL, url) {
  const parts = StringPrototypeSplit(StringPrototypeSlice(url, 5), ',');
  const format = StringPrototypeSplit(parts[0], ';');
  if (format[0] !== 'application/json') return undefined;
  const base64 = format[format.length - 1] === 'base64';
  const text = base64 ? require('internal/host/buffer').Buffer.from(parts[1], 'base64').toString('utf8') : parts[1];
  return sourcesToAbsolute(sourceURL, JSONParse(text));
}

function loadSourceMap(file) {
  const { pathToFileURL, fileURLToPath, URL } = require('internal/url');
  const content = fs.readFileSync(file, 'utf8');
  const mapURL = lastSourceMappingURL(content);
  if (mapURL === undefined) return undefined;
  const sourceURL = pathToFileURL(file).href;
  let data;
  if (StringPrototypeStartsWith(mapURL, 'data:')) {
    data = mapFromDataUrl(sourceURL, mapURL);
  } else {
    const resolved = new URL(mapURL, sourceURL);
    if (resolved.protocol !== 'file:') return undefined;
    data = sourcesToAbsolute(resolved.href, JSONParse(fs.readFileSync(fileURLToPath(resolved), 'utf8')));
  }
  if (data === undefined) return undefined;
  const lineLengths = [];
  const lines = StringPrototypeSplit(content, kLineBreak);
  for (let i = 0; i < lines.length; i++) lineLengths[i] = lines[i].length;
  const { SourceMap } = require('internal/source_map/source_map');
  return new SourceMap(data, { lineLengths });
}

function findSourceMap(sourceURL) {
  if (typeof sourceURL !== 'string') return undefined;
  scanCache();
  let file = sourceURL;
  if (StringPrototypeStartsWith(file, 'file://')) {
    file = require('internal/url').fileURLToPath(file);
  } else {
    file = path.resolve(file);
  }
  if (!eligible[file]) return undefined;
  if (file in found) return found[file];
  let map;
  try {
    map = loadSourceMap(file);
  } catch {
    map = undefined;
  }
  return (found[file] = map);
}

module.exports = {
  findSourceMap,
  getSourceMapsSupport,
  setSourceMapsEnabled,
  setSourceMapsSupport,
};
