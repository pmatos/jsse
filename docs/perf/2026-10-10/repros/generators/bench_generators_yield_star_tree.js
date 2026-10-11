// Measured 2026-10-10, jsse HEAD 2b621c5 vs boa 0.22.0, buildbox3, median of 5 (loadavg 10-12): jsse 4.63 s, boa 1.77 s, ratio 2.61x, jsse --no-bytecode (median of 3) 4.68 s.
// Exercises: recursive `yield*` delegation through a deep tree, the shape of
// sync-fs forEachFileRecursively() / BinaryTree[Symbol.iterator]. Every leaf
// value is forwarded through `depth` nested delegating generators, and the
// generator itself yields an object literal {name, entry, isDirectory}.
// Models: sync-fs, lazy-collections (BinaryTree).
class Dir {
  constructor() { this.entries = new Map(); }
  *ls() {
    for (const [name, entry] of this.entries)
      yield { name, entry, isDirectory: entry instanceof Dir };
  }
  *files() {
    for (const item of this.ls()) {
      if (item.isDirectory) yield* item.entry.files();
      else yield item;
    }
  }
}
class FileObj { constructor(n) { this.size = n; } }
// 6 levels, fan-out 4, 3 files per leaf-ish dir
function build(depth) {
  const d = new Dir();
  for (let i = 0; i < 3; i++) d.entries.set("f" + i, new FileObj(i + depth));
  if (depth > 0) for (let i = 0; i < 4; i++) d.entries.set("d" + i, build(depth - 1));
  return d;
}
const root = build(6);
let count = 0, bytes = 0;
for (let r = 0; r < 4; r++) {
  for (const { entry } of root.files()) { count++; bytes = (bytes + entry.size) | 0; }
}
console.log("yield_star_tree", count, bytes);
