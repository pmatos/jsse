/*---
description: >
  Values reachable only through a block-scoped binding of a compiled function
  stay alive across garbage collections: the VM roots the block environments
  it opens, including the per-iteration copies of a `for (let ...)` head and a
  `switch` body's environment.
info: |
  jsse issue #958: block environments live in the VM's own scope stack, so the
  collector sees them only through the interpreter's environment roots. A
  missing root frees an object whose arena id is then recycled, which shows up
  as a wrong-typed value after later allocations.
esid: sec-blockdeclarationinstantiation
flags: [noStrict]
---*/

function churn() {
  var junk = [];
  for (var n = 0; n < 200; n++) {
    junk.push({ n: n, pad: [n, n + 1, n + 2] });
  }
  return junk.length;
}

function collect() {
  $262.gc();
  churn();
  $262.gc();
}

function blockConstSurvives() {
  {
    const held = { tag: "held", items: [1, 2, 3], nested: { deep: "ok" } };
    collect();
    churn();
    assert.sameValue(held.tag, "held");
    assert.sameValue(held.items.join(), "1,2,3");
    assert.sameValue(held.nested.deep, "ok");
  }
}
blockConstSurvives();

function loopCarriedBlockLexicalsSurvive() {
  var sum = 0;
  for (let i = 0; i < 4; i++) {
    const row = { index: i, cells: [i, i * 2] };
    collect();
    sum += row.index + row.cells[1];
    assert.sameValue(row.cells.length, 2);
  }
  return sum;
}
assert.sameValue(loopCarriedBlockLexicalsSurvive(), 18);

function capturedPerIterationBindingsSurvive() {
  var readers = [];
  for (let i = 0; i < 3; i++) {
    let box = { value: i * 100, list: [i] };
    readers.push(function () { return box.value + box.list[0] + i; });
    collect();
  }
  collect();
  return readers.map(function (read) { return read(); });
}
var captured = capturedPerIterationBindingsSurvive();
assert.sameValue(captured.join(), "0,102,204");

function forLetHeadBindingSurvivesItsBody() {
  var total = 0;
  for (let o = { n: 1, more: [2] }, steps = 0; steps < 3; steps++) {
    collect();
    total += o.n + o.more[0];
  }
  return total;
}
assert.sameValue(forLetHeadBindingSurvivesItsBody(), 9);

function switchBlockBindingsSurvive(which) {
  switch (which) {
    case 1:
      let kept = { label: "kept", data: [4, 5, 6] };
      collect();
      return kept.label + kept.data.join("");
    default: {
      const other = { label: "other" };
      collect();
      return other.label;
    }
  }
}
assert.sameValue(switchBlockBindingsSurvive(1), "kept456");
assert.sameValue(switchBlockBindingsSurvive(2), "other");

function nestedBlocksAcrossCallsSurvive() {
  var results = [];
  for (let a = 0; a < 2; a++) {
    let outer = { a: a, list: [a, a] };
    {
      let inner = { sum: outer.a + 1 };
      collect();
      {
        let innermost = [outer.list.length, inner.sum];
        collect();
        results.push(innermost.join());
      }
    }
  }
  return results.join(";");
}
assert.sameValue(nestedBlocksAcrossCallsSurvive(), "2,1;2,2");

function returnFromBlockWithLiveObject() {
  for (let i = 0; i < 3; i++) {
    const result = { found: i };
    collect();
    if (i === 1) return result;
  }
  return null;
}
assert.sameValue(returnFromBlockWithLiveObject().found, 1);
