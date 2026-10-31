// Control for scripts/run-ts-lint-oracle.py (#1295): copied to scripts/lintControl.mjs of two TypeScript
// checkouts; the scripts project (scripts/tsconfig.json, checkJs) is linted with its own type information.
var legacy = 1;
let fixable = legacy;
const wrapper = new String("x");

export function control(input) {
    if (input == null) {
        return __dirname;
    }
    if (eval("1")) {
        throw "literal";
    }
    const obj = { fixable: fixable, wrapper: wrapper };
    return obj.fixable + require("fs").sep;
}
