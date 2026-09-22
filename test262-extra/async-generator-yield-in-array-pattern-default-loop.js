/*---
description: >
  A `yield` inside the default of an array destructuring pattern nested
  inside a `for` loop suspends an async generator per iteration, without the
  loop's own suspension detection collapsing the whole loop into one
  tree-walked statement (which would replay -- and duplicate the side
  effects of -- earlier iterations).
esid: sec-runtime-semantics-iteratorbindinginitialization
info: |
  Nothing in IteratorBindingInitialization restricts a `yield` expression
  from appearing as a binding pattern element's Initializer, so an async
  generator must suspend at it like any other `yield`, once per loop
  iteration, without re-running already-completed iterations.
flags: [async]
features: [async-iteration, destructuring-binding]
---*/

async function run() {
  var log = [];
  async function* g() {
    for (var i = 0; i < 2; i++) {
      var [a = yield i] = [];
      log.push('a=' + a);
    }
  }
  var it = g();
  var results = [];
  results.push(await it.next());
  results.push(await it.next('x0'));
  results.push(await it.next('x1'));
  return { results: results, log: log };
}

run()
  .then(function (r) {
    assert.sameValue(r.results[0].value, 0, 'first loop iteration yields its own iteration value');
    assert.sameValue(r.results[0].done, false, 'first yield has not completed the generator');
    assert.sameValue(r.results[1].value, 1, 'second loop iteration yields its own iteration value');
    assert.sameValue(r.results[1].done, false, 'second yield has not completed the generator');
    assert.sameValue(r.results[2].done, true, 'the loop completes after its second iteration');
    assert.sameValue(
      r.log.length,
      2,
      'each loop iteration runs its body exactly once -- no replay-induced duplicate'
    );
    assert.sameValue(r.log[0], 'a=x0', 'first iteration bound the value sent on resume');
    assert.sameValue(r.log[1], 'a=x1', 'second iteration bound the value sent on resume');
  })
  .then($DONE, $DONE);
