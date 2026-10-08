// Which op mix does the bytecode VM actually make cheaper? (issue #526)
// Four loops of equal iteration count, differing only in the kind of work per
// iteration: pure register arithmetic, typed-array element traffic, calls to a
// tiny leaf, and mandreel's own mix of all three.
var heap32 = new Int32Array(1 << 16);
var N = 1000000;

function arith(n) {
  var r0 = 0, r1 = 1, r2 = 2;
  for (var i = 0; i < n; i++) {
    r0 = (r0 + 4) | 0;
    r1 = (r0 + r1) | 0;
    r1 = r1 & 65535;
    r2 = r1 >> 2;
    r0 = (r2 + r0) | 0;
    r0 = r0 & 65535;
  }
  return r0 + r1 + r2;
}

function elem(n) {
  var fp = 16;
  for (var i = 0; i < n; i++) {
    var r0 = heap32[fp];
    r0 = (r0 + 4) | 0;
    heap32[fp] = r0;
    var r1 = heap32[(fp + 1)];
    r1 = (r0 + r1) | 0;
    r1 = r1 & 65535;
    heap32[(fp + 1)] = r1;
  }
  return heap32[fp];
}

function leaf(sp) {
  var r0 = sp >> 2;
  r0 = (r0 + 4) | 0;
  return r0 & 65535;
}

function called(n) {
  var s = 0;
  for (var i = 0; i < n; i++) {
    s = leaf(s);
    s = leaf(s);
  }
  return s;
}

function mixed(n) {
  var fp = 16;
  for (var i = 0; i < n; i++) {
    var r0 = heap32[fp];
    r0 = leaf(r0);
    heap32[fp] = r0;
    var r1 = heap32[(fp + 1)];
    r1 = (r0 + r1) | 0;
    r1 = r1 & 65535;
    heap32[(fp + 1)] = r1;
  }
  return heap32[fp];
}

// Mandreel's own loop shape (issue #873): a labeled while(true) with a
// labeled break terminating the loop and a labeled continue skipping part
// of the body, carrying elem's register traffic. Runs exactly n iterations
// (the increment and break check happen before the continue), so timing is
// comparable to the other op-mix variants above.
function labeledLoop(n) {
  var fp = 16;
  var i = 0;
  loop: while (true) {
    if (i >= n) break loop;
    var r0 = heap32[fp];
    r0 = (r0 + 4) | 0;
    heap32[fp] = r0;
    i++;
    if ((i & 1) === 0) continue loop;
    var r1 = heap32[(fp + 1)];
    r1 = (r0 + r1) | 0;
    r1 = r1 & 65535;
    heap32[(fp + 1)] = r1;
  }
  return heap32[fp];
}

// Isolates the test and branch dispatch in Mandreel's labeled while(true)
// shape. Fifty million iterations keep this otherwise tiny body long enough
// for Date.now() to resolve a small per-iteration change under host load.
function labeledControl(n) {
  var i = 0;
  loop: while (true) {
    if (i >= n) break loop;
    i++;
    continue loop;
  }
  return i;
}

function bench(label, fn, n) {
  fn(1000);
  var t = Date.now();
  var v = fn(n);
  console.log("BENCH\t" + label + "\t" + (Date.now() - t) + "\t" + v);
}

bench("arith", arith, N);
bench("elem", elem, N);
bench("called", called, N);
bench("mixed", mixed, N);
bench("labeledLoop", labeledLoop, N);
bench("labeledControl", labeledControl, N * 50);
