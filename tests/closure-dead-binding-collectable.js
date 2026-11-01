// #1420: a closure keeps the bindings it names, not every local of the call
// that made it. ECMAScript lets an implementation retain more (so this is an
// engine test, not a test262 one).
'use strict';

function check(condition, message) {
  if (!condition) throw new Error(message);
}

const dead = [];
const kept = [];
const evalKept = [];

function makeCallback(tag) {
  const deadObject = { tag, payload: new Array(8) };
  dead.push(new WeakRef(deadObject));
  const named = { tag };
  kept.push(new WeakRef(named));
  return () => named.tag;
}

function makeEvalCallback() {
  const reachableByEval = { tag: 'eval' };
  evalKept.push(new WeakRef(reachableByEval));
  return (source) => eval(source);
}

function makeNestedCallback(tag) {
  const deadObject = { tag };
  dead.push(new WeakRef(deadObject));
  const named = { tag };
  kept.push(new WeakRef(named));
  return () => () => named.tag;
}

const callbacks = [makeCallback('a'), makeCallback('b'), makeNestedCallback('c')()];
const evalCallback = makeEvalCallback();

$262.clearKeptObjects();
$262.gc();

const deadLive = dead.filter((ref) => ref.deref() !== undefined).length;
const keptLive = kept.filter((ref) => ref.deref() !== undefined).length;
check(deadLive === 0, `${deadLive} unnamed locals are still reachable from closures`);
check(keptLive === 3, `a captured binding was released (${keptLive} of 3 live)`);
check(callbacks.map((f) => f()).join() === 'a,b,c', 'captured bindings read back');
check(evalCallback('reachableByEval.tag') === 'eval', 'direct eval keeps every binding');
check(evalKept[0].deref() !== undefined, 'a binding reachable by eval was released');
