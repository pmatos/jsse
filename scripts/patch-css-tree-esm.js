// Replace css-tree's createRequire(import.meta.url) JSON loading with static
// ESM imports, so esbuild can bundle it for a single filesystem-free IIFE.
//
// lib/version.js, lib/data.js, and lib/data-patch.js each load a JSON file
// through Node's CJS-in-ESM interop (createRequire + require('*.json')).
// esbuild bundles that pattern as a literal runtime require() call rather
// than inlining the JSON, which jsse cannot execute (there is no module
// system in the bundled IIFE). Static `import x from '*.json'` hits esbuild's
// built-in JSON loader and inlines the same data at build time. Same JSON
// content either way — this only changes how it's loaded.

"use strict";

const fs = require("fs");

function replaceExact(file, before, after) {
  const source = fs.readFileSync(file, "utf8");
  const first = source.indexOf(before);

  if (first === -1) {
    throw new Error(`patch-css-tree-esm: no match in ${file}`);
  }
  if (source.indexOf(before, first + before.length) !== -1) {
    throw new Error(`patch-css-tree-esm: ambiguous match in ${file}`);
  }

  fs.writeFileSync(
    file,
    source.slice(0, first) + after + source.slice(first + before.length)
  );
}

replaceExact(
  "lib/version.js",
  `import { createRequire } from 'module';

const require = createRequire(import.meta.url);

export const { version } = require('../package.json');`,
  `import pkg from '../package.json';

export const { version } = pkg;`
);

replaceExact(
  "lib/data-patch.js",
  `import { createRequire } from 'module';

const require = createRequire(import.meta.url);
const patch = require('../data/patch.json');

export default patch;`,
  `import patch from '../data/patch.json';

export default patch;`
);

replaceExact(
  "lib/data.js",
  `import { createRequire } from 'module';
import patch from './data-patch.js';

const require = createRequire(import.meta.url);
const mdnAtrules = require('mdn-data/css/at-rules.json');
const mdnProperties = require('mdn-data/css/properties.json');
const mdnSyntaxes = require('mdn-data/css/syntaxes.json');`,
  `import mdnAtrules from 'mdn-data/css/at-rules.json';
import mdnProperties from 'mdn-data/css/properties.json';
import mdnSyntaxes from 'mdn-data/css/syntaxes.json';
import patch from './data-patch.js';`
);

console.log("patch-css-tree-esm: rewrote createRequire JSON loads to static imports");
