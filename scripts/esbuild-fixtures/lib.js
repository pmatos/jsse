'use strict';
const fs = require('fs');
const path = require('path');
const crypto = require('crypto');

const root = process.cwd();
const scrub = (text) => text.split(root).join('<cwd>');

exports.esbuild = require(process.env.ESBUILD_DIR);

exports.sha = (data) => crypto.createHash('sha256').update(data).digest('hex');

exports.show = (label, value) => console.log(label, scrub(JSON.stringify(value)));

// Every file under dir as [path, size, sha256], sorted.
exports.tree = (dir) => {
  const files = [];
  const walk = (current) => {
    for (const name of fs.readdirSync(current).sort()) {
      const full = path.join(current, name);
      if (fs.statSync(full).isDirectory()) walk(full);
      else files.push([path.relative(dir, full), fs.statSync(full).size, exports.sha(fs.readFileSync(full))]);
    }
  };
  walk(dir);
  return files;
};

exports.text = (file) => fs.readFileSync(file, 'utf8');

exports.run = (main) => {
  main().catch((e) => {
    console.log('UNEXPECTED', scrub(String(e && e.stack || e)));
    process.exitCode = 1;
  });
};
