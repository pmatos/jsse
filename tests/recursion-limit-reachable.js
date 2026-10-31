// JS call recursion that Node completes must complete here too (jsse#1271). TypeScript's checker reaches a
// flow-analysis depth of 2000 nested getTypeAtFlowNode calls (largeControlFlowGraph.ts), more than 4,000 JS calls in
// all, before it reports TS2563 instead of overflowing; a call-depth limit of 4,000 turned that into a
// RangeError. The depth limit is implementation-defined (ECMA-262 does not bound recursion), so this is a floor
// on the engine's capacity, not a semantics change: the limit-hit behaviour stays in
// recursion-limit-interpreter.js.

function plain(n) {
  return n <= 0 ? 0 : 1 + plain(n - 1);
}
if (plain(7000) !== 7000) {
  throw new Error("plain recursion to depth 7000 did not complete");
}

function viaMap(n) {
  return n <= 0 ? 0 : [n].map(function (x) { return 1 + viaMap(x - 1); })[0];
}
if (viaMap(2000) !== 2000) {
  throw new Error("recursion through Array.prototype.map to depth 2000 did not complete");
}

class Chain {
  constructor(depth) {
    this.child = depth > 0 ? new Chain(depth - 1) : null;
  }
  get size() {
    return 1 + (this.child ? this.child.size : 0);
  }
}
if (new Chain(2500).size !== 2501) {
  throw new Error("nested constructor/getter recursion to depth 2500 did not complete");
}
