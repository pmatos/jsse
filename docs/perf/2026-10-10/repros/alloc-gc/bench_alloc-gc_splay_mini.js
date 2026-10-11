// Theme: alloc-gc. Scaled-down verbatim structure of JetStream Octane/splay.js: a splay tree of
// keyed nodes each carrying a depth-5 payload tree (63 objects + 32 arrays + 32 strings per node),
// followed by insert+remove churn that makes old payloads garbage.
// Keys come from a seeded LCG (JetStream uses a deterministic Math.random; here independent of it).
// Measured (buildbox3, median of 5, load 4.7, jsse HEAD 2b621c5 vs boa 0.22.0, 2026-10-10): jsse 2.63 s / boa 0.67 s = 3.93x; jsse --no-bytecode 2.88 s.
var kSplayTreeSize = 2000;
var kSplayTreeModifications = 80;
var kSplayTreePayloadDepth = 5;
var seed = 49734321;
function GenerateKey() {
  seed = (Math.imul(seed, 1103515245) + 12345) & 0x7fffffff;
  return seed / 0x80000000;
}
function GeneratePayloadTree(depth, tag) {
  if (depth == 0) {
    return {
      array: [0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
      string: 'String for key ' + tag + ' in leaf node'
    };
  } else {
    return {
      left: GeneratePayloadTree(depth - 1, tag),
      right: GeneratePayloadTree(depth - 1, tag)
    };
  }
}
function SplayTree() {}
SplayTree.prototype.root_ = null;
SplayTree.prototype.isEmpty = function () { return !this.root_; };
SplayTree.prototype.insert = function (key, value) {
  if (this.isEmpty()) { this.root_ = new SplayTree.Node(key, value); return; }
  this.splay_(key);
  if (this.root_.key == key) return;
  var node = new SplayTree.Node(key, value);
  if (key > this.root_.key) {
    node.left = this.root_;
    node.right = this.root_.right;
    this.root_.right = null;
  } else {
    node.right = this.root_;
    node.left = this.root_.left;
    this.root_.left = null;
  }
  this.root_ = node;
};
SplayTree.prototype.remove = function (key) {
  if (this.isEmpty()) throw Error('Key not found: ' + key);
  this.splay_(key);
  if (this.root_.key != key) throw Error('Key not found: ' + key);
  var removed = this.root_;
  if (!this.root_.left) {
    this.root_ = this.root_.right;
  } else {
    var right = this.root_.right;
    this.root_ = this.root_.left;
    this.splay_(key);
    this.root_.right = right;
  }
  return removed;
};
SplayTree.prototype.find = function (key) {
  if (this.isEmpty()) return null;
  this.splay_(key);
  return this.root_.key == key ? this.root_ : null;
};
SplayTree.prototype.findMax = function (opt_startNode) {
  if (this.isEmpty()) return null;
  var current = opt_startNode || this.root_;
  while (current.right) current = current.right;
  return current;
};
SplayTree.prototype.findGreatestLessThan = function (key) {
  if (this.isEmpty()) return null;
  this.splay_(key);
  if (this.root_.key < key) return this.root_;
  else if (this.root_.left) return this.findMax(this.root_.left);
  else return null;
};
SplayTree.prototype.exportKeys = function () {
  var result = [];
  if (!this.isEmpty()) this.root_.traverse_(function (node) { result.push(node.key); });
  return result;
};
SplayTree.prototype.splay_ = function (key) {
  if (this.isEmpty()) return;
  var dummy, left, right;
  dummy = left = right = new SplayTree.Node(null, null);
  var current = this.root_;
  while (true) {
    if (key < current.key) {
      if (!current.left) break;
      if (key < current.left.key) {
        var tmp = current.left;
        current.left = tmp.right;
        tmp.right = current;
        current = tmp;
        if (!current.left) break;
      }
      right.left = current;
      right = current;
      current = current.left;
    } else if (key > current.key) {
      if (!current.right) break;
      if (key > current.right.key) {
        var tmp = current.right;
        current.right = tmp.left;
        tmp.left = current;
        current = tmp;
        if (!current.right) break;
      }
      left.right = current;
      left = current;
      current = current.right;
    } else {
      break;
    }
  }
  left.right = current.left;
  right.left = current.right;
  current.left = dummy.right;
  current.right = dummy.left;
  this.root_ = current;
};
SplayTree.Node = function (key, value) {
  this.key = key;
  this.value = value;
};
SplayTree.Node.prototype.left = null;
SplayTree.Node.prototype.right = null;
SplayTree.Node.prototype.traverse_ = function (f) {
  var current = this;
  while (current) {
    var left = current.left;
    if (left) left.traverse_(f);
    f(current);
    current = current.right;
  }
};
var splayTree = new SplayTree();
function InsertNewNode() {
  var key;
  do { key = GenerateKey(); } while (splayTree.find(key) != null);
  var payload = GeneratePayloadTree(kSplayTreePayloadDepth, String(key));
  splayTree.insert(key, payload);
  return key;
}
for (var i = 0; i < kSplayTreeSize; i++) InsertNewNode();
for (var rep = 0; rep < 10; rep++) {
  for (var i = 0; i < kSplayTreeModifications; i++) {
    var key = InsertNewNode();
    var greatest = splayTree.findGreatestLessThan(key);
    if (greatest == null) splayTree.remove(key);
    else splayTree.remove(greatest.key);
  }
}
var keys = splayTree.exportKeys();
var ok = keys.length === kSplayTreeSize;
for (var i = 0; i < keys.length - 1; i++) if (keys[i] >= keys[i + 1]) ok = false;
console.log("splay_mini", keys.length, ok, keys[0].toFixed(6));
