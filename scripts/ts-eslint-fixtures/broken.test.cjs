// Rule tests that fail in the ways RuleTester reports: a wrong message id, a wrong error count, a wrong
// location, a wrong fix output, a valid case that reports, an invalid case that does not, a rule that throws,
// a schema violation and a parse error. ESLINT_FIXED=1 makes every one of them pass.
const { RuleTester } = require("./support/RuleTester.cjs");
const inOperator = require("../rules/no-in-operator.cjs");
const jsdoc = require("../rules/jsdoc-format.cjs");
const keywords = require("../rules/no-keywords.cjs");

const fixed = !!process.env.ESLINT_FIXED;
const ruleTester = new RuleTester({
    languageOptions: { parserOptions: { warnOnUnsupportedTypeScriptVersion: false } },
});

ruleTester.run("no-in-operator", inOperator, {
    valid: [
        { code: fixed ? "a.b(c);" : "a in b;" },
    ],
    invalid: [
        { code: "a in b;", errors: [{ messageId: fixed ? "noInOperatorError" : "wrongId" }] },
        { code: "a in b; c in d;", errors: fixed ? [{ messageId: "noInOperatorError" }, { messageId: "noInOperatorError" }] : [{ messageId: "noInOperatorError" }] },
        { code: "\n  a in b;", errors: [{ messageId: "noInOperatorError", line: 2, column: fixed ? 3 : 1 }] },
        { code: fixed ? "x in y;" : "a.b(c);", errors: [{ messageId: "noInOperatorError" }] },
    ],
});

ruleTester.run("jsdoc-format", jsdoc, {
    valid: [],
    invalid: [
        {
            code: "/* @internal */\nexport function f() {}",
            output: fixed ? null : "/** @internal */\nexport function f() {}",
            errors: [{ messageId: "internalCommentInNonJSDocError" }],
        },
    ],
});

const thrower = {
    meta: { type: "problem", schema: [], messages: { m: "m" } },
    create: () => {
        if (!fixed) throw new Error("the rule blew up in create");
        return {};
    },
};
ruleTester.run("thrower", thrower, { valid: [{ code: "1;" }], invalid: [] });

ruleTester.run("no-keywords-schema", keywords, {
    valid: [{ code: "1;", options: fixed ? [{ keywords: true }] : [{ keywords: "yes" }] }],
    invalid: [],
});

ruleTester.run("parse-error", inOperator, {
    valid: [{ code: fixed ? "a;" : "a +;" }],
    invalid: [],
});
