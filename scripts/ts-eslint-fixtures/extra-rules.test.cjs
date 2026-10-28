// Rules and RuleTester features the TypeScript suite's own tests leave out: a rule with a fixer and an
// `output`, a rule that reads `context.filename`, rule options, the TypeScript parser on TS-only syntax.
const { RuleTester } = require("./support/RuleTester.cjs");
const jsdoc = require("../rules/jsdoc-format.cjs");
const direct = require("../rules/no-direct-import.cjs");
const keywords = require("../rules/no-keywords.cjs");
const inOperator = require("../rules/no-in-operator.cjs");

const ruleTester = new RuleTester({
    languageOptions: { parserOptions: { warnOnUnsupportedTypeScriptVersion: false } },
});

ruleTester.run("jsdoc-format", jsdoc, {
    valid: [
        { code: "/** @internal */\nexport function f() {}" },
        { code: "/** docs */\nexport const a = 1;" },
        { code: "// plain\nfunction g() {}" },
    ],
    invalid: [
        {
            code: "/* @internal */\nexport function f() {}",
            errors: [{ messageId: "internalCommentInNonJSDocError" }],
        },
        {
            code: "/** @internal */\n/** more */\nexport function f() {}",
            errors: [{ messageId: "internalCommentNotLastError" }, { messageId: "multipleJSDocError" }],
        },
        {
            code: "/** one */\n/** two */\nexport const a = 1;",
            errors: [{ messageId: "multipleJSDocError" }],
        },
        {
            code: "/** @internal */\nfunction hidden() {}",
            errors: [{ messageId: "internalCommentOnUnexported" }],
        },
    ],
});

ruleTester.run("no-direct-import", direct, {
    valid: [
        { code: 'import "./sibling";', filename: "/proj/src/a/b.ts" },
        { code: 'import x = require("lib");', filename: "/proj/src/a/b.ts" },
        { code: 'import "../other/thing";', filename: "/proj/lib/a/b.ts" },
        { code: 'import "../../typescript/typescript.js";', filename: "/proj/src/a/b.ts" },
    ],
    invalid: [
        {
            code: 'import "../other/thing";',
            filename: "/proj/src/a/b.ts",
            errors: [{ messageId: "noDirectImport" }],
        },
        {
            code: 'import x = require("../other/thing");',
            filename: "/proj/src/a/b.ts",
            errors: [{ messageId: "noDirectImport" }],
        },
    ],
});

ruleTester.run("no-keywords-options", keywords, {
    valid: [
        { code: "const { a, b } = c;", options: [{ keywords: true }] },
        { code: "type T = { x: number }; let n: T;" },
    ],
    invalid: [
        {
            code: "const { string, a: number } = c;",
            errors: [{ messageId: "noKeywordsError", data: { name: "string" } }],
        },
        {
            code: "function f(any: any, boolean: boolean) {}",
            errors: [
                { messageId: "noKeywordsError", data: { name: "any" }, line: 1, column: 12 },
                { messageId: "noKeywordsError", data: { name: "boolean" }, line: 1, column: 22 },
            ],
        },
        {
            code: "const [undefined, x] = y;",
            errors: [{ messageId: "noKeywordsError", data: { name: "undefined" }, endColumn: 17 }],
        },
    ],
});

ruleTester.run("no-in-operator-ts", inOperator, {
    valid: [
        { code: "enum E { A } declare module 'm' { const v: E; } for (const k in {}) {}" },
        { code: "abstract class C<T extends object = {}> { abstract m(this: C<T>): void; }" },
    ],
    invalid: [
        {
            code: "type K = keyof { a: 1 }; const r = <const>'a' in { a: 1 };",
            errors: [{ messageId: "noInOperatorError", line: 1, column: 36 }],
        },
        {
            code: "if ('a' in o) {} while ('b' in o) {}",
            errors: [{ messageId: "noInOperatorError" }, { messageId: "noInOperatorError" }],
        },
    ],
});
