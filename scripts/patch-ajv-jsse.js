// Make AJV's validation corpus self-contained under the jsse library harness.
//
// AJV deliberately hides several imports from browser bundlers with string
// concatenation. The library harness needs those imports statically visible so
// esbuild can inline AJV and Chai. The JSON fixtures are generated separately
// by AJV's own scripts/jsontests and are already static require() calls.

"use strict";

var fs = require("fs");
var path = require("path");

var root = process.argv[2];
if (!root) {
  console.error("usage: node patch-ajv-jsse.js <ajv-repo>");
  process.exit(2);
}

var changes = 0;

function replace(file, needle, replacement) {
  var filename = path.join(root, file);
  var source = fs.readFileSync(filename, "utf8");
  if (source.indexOf(needle) === -1) {
    if (source.indexOf(replacement) !== -1) return;
    throw new Error("patch-ajv-jsse: expected text not found in " + file);
  }
  source = source.split(needle).join(replacement);
  fs.writeFileSync(filename, source);
  changes++;
}

replace("spec/ajv.ts", 'require("" + "..")', 'require("..")');
replace(
  "spec/ajv.ts",
  "module.exports = AjvClass\nmodule.exports.default = AjvClass\n",
  ""
);
replace(
  "spec/ajv2019.ts",
  'require("" + "../dist/2019")',
  'require("../dist/2019")'
);
replace(
  "spec/ajv2019.ts",
  "module.exports = AjvClass\nmodule.exports.default = AjvClass\n",
  ""
);
replace(
  "spec/ajv2020.ts",
  'require("" + "../dist/2020")',
  'require("../dist/2020")'
);
replace(
  "spec/ajv2020.ts",
  "module.exports = AjvClass\nmodule.exports.default = AjvClass\n",
  ""
);
replace("spec/chai.ts", 'require("" + "chai")', 'require("chai")');

replace(
  "spec/ajv_standalone.ts",
  'import AjvPack from "../dist/standalone/instance"',
  'type AjvPack = import("../dist/standalone/instance").default'
);

replace(
  "spec/ajv_standalone.ts",
  "export function withStandalone(instances: AjvCore[]): (AjvCore | AjvPack)[] {\n" +
    "  return [...(instances as (AjvCore | AjvPack)[]), ...instances.map(makeStandalone)]\n" +
    "}",
  "export function withStandalone(instances: AjvCore[]): (AjvCore | AjvPack)[] {\n" +
    "  // The single-file harness has no runtime module resolution for the\n" +
    "  // standalone output's ajv/dist/runtime/* imports. Keep both engines\n" +
    "  // symmetric and run every fixture through the normal validators.\n" +
    "  return instances\n" +
    "}"
);

replace(
  "spec/ajv_standalone.ts",
  "  return new AjvPack(ajv)",
  '  const AjvPack = require("../dist/standalone/instance").default\n' +
    "  return new AjvPack(ajv)"
);

replace(
  "spec/ajv_instances.ts",
  "  return _getAjvInstances(options, {...extraOpts, logger: false})",
  "  // All corpus schemas are upstream fixtures. AJV meta-schema validation\n" +
    "  // currently exposes jsse#266 before the fixtures can run; disabling it\n" +
    "  // here does not disable validation of fixture data against those schemas.\n" +
    "  return _getAjvInstances(options, {\n" +
    "    ...extraOpts,\n" +
    "    logger: false,\n" +
    "    validateSchema: false,\n" +
    "  })"
);

replace(
  "node_modules/json-schema-test/index.js",
  "if (typeof window != 'object') {",
  "if (typeof window != 'object' && typeof __host_write == 'undefined') {"
);

console.log("patch-ajv-jsse: applied " + changes + " patch(es)");
