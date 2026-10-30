// Control for scripts/run-ts-lint-oracle.py (#1295): copied to src/compiler/lintControl.ts of two TypeScript
// checkouts. Every statement below breaks one rule of the checkout's eslint.config.mjs, so the diagnostics (and
// the fixes of --fix) that Node and jsse produce can be diffed. Rules that need type information
// (argument-trivia, no-unnecessary-type-assertion) make eslint build a program over src/ on both engines.
import * as ts from "./_namespaces/ts.js";
import { Debug } from "./_namespaces/ts.js";

var oldStyle = 1;
let neverReassigned = 2;

function takesFlag(flag: boolean, count: number, label: string) {
    return flag ? count : label.length;
}

export function lintControl(x: unknown, y: string | undefined, record: object) {
    if (x == null) {
        return takesFlag(true, 0, "a");
    }
    if ("key" in record) {
        Debug.assert(!!y);
    }
    const asserted = (y as string | undefined);
    const nothing = null;
    const unusedWarning = oldStyle + neverReassigned;
    const re = /(a+)+b/;
    const sorted = [3, 1, 2].sort();
    setTimeout(() => {}, 0);
    const fn = function () {
        return re.test(String(asserted)) && nothing === undefined;
    };
    // eslint-disable-next-line no-eval
    return fn() ? sorted : ts.emptyArray;
}

/**
 *No space after the asterisk, and trailing whitespace:
 */
export const badJsdoc = 1;

export interface IBadName {
    _a: string;
}

class NamedBadly {
    constructor() {
        return;
    }
}
export { NamedBadly as badly_named };
