// A CommonJS script.
/** @typedef {{ id: number, tag?: string }} Rec */

/**
 * @param {Rec} r
 * @returns {number}
 */
function id(r) {
    return r.id;
}

class Box {
    constructor() {
        /** @type {number[]} */
        this.items = [];
    }
    /** @param {number} n */
    add(n) {
        this.items.push(n);
        return this;
    }
}

module.exports = { id, Box };
