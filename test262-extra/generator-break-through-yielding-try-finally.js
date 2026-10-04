/*---
description: >
  A break or continue that leaves a try statement containing a yield runs the
  try's finally block before control reaches the loop or labelled statement.
esid: sec-try-statement-runtime-semantics-evaluation
info: |
  Evaluation of a try-finally statement evaluates the Finally clause for
  every completion of its try Block, including break and continue completions.
  If the Finally clause completes normally, the original completion is
  restored and propagates to the enclosing loop or labelled statement.
includes: [compareArray.js]
features: [generators]
---*/

function drain(iterator) {
  var results = [];
  for (var step = iterator.next(); !step.done; step = iterator.next()) {
    results.push(step.value);
  }
  return results;
}

function* breakOutOfLoop() {
  var log = [];
  for (var i = 0; i < 4; i++) {
    try {
      yield i;
      if (i == 1) break;
    } finally {
      log.push(i);
    }
  }
  yield log.join();
}
assert.compareArray(drain(breakOutOfLoop()), [0, 1, '0,1'], 'break out of for');

function* continueLoop() {
  var log = [];
  for (var i = 0; i < 3; i++) {
    try {
      yield i;
      if (i == 1) continue;
      log.push('body' + i);
    } finally {
      log.push('finally' + i);
    }
  }
  yield log.join();
}
assert.compareArray(
  drain(continueLoop()),
  [0, 1, 2, 'body0,finally0,finally1,body2,finally2'],
  'continue in for'
);

function* labeledContinue() {
  var log = [];
  outer: for (var i = 0; i < 3; i++) {
    for (var j = 0; j < 2; j++) {
      try {
        yield i + ':' + j;
        continue outer;
      } finally {
        log.push('f' + i + j);
      }
    }
  }
  yield log.join();
}
assert.compareArray(
  drain(labeledContinue()),
  ['0:0', '1:0', '2:0', 'f00,f10,f20'],
  'continue to an outer label'
);

function* labeledBreak() {
  var log = [];
  outer: for (var i = 0; i < 3; i++) {
    for (var j = 0; j < 2; j++) {
      try {
        yield i + ':' + j;
        break outer;
      } finally {
        log.push('f' + i + j);
      }
    }
  }
  yield log.join();
}
assert.compareArray(drain(labeledBreak()), ['0:0', 'f00'], 'break to an outer label');

function* breakOutOfWhile() {
  var log = [];
  var i = 0;
  while (true) {
    try {
      yield i++;
      if (i == 2) break;
    } finally {
      log.push('f' + i);
    }
  }
  yield log.join();
}
assert.compareArray(drain(breakOutOfWhile()), [0, 1, 'f1,f2'], 'break out of while');

function* breakOutOfDoWhile() {
  var log = [];
  var i = 0;
  do {
    try {
      yield i++;
      if (i == 2) break;
    } finally {
      log.push('f' + i);
    }
  } while (true);
  yield log.join();
}
assert.compareArray(drain(breakOutOfDoWhile()), [0, 1, 'f1,f2'], 'break out of do-while');

function* continueDoWhile() {
  var log = [];
  var i = 0;
  do {
    try {
      yield i;
      continue;
    } finally {
      log.push('f' + i);
    }
  } while (++i < 2);
  yield log.join();
}
assert.compareArray(drain(continueDoWhile()), [0, 1, 'f0,f1'], 'continue in do-while');

function* breakOutOfSwitch() {
  var log = [];
  switch (1) {
    case 1:
      try {
        yield 'in';
        break;
      } finally {
        log.push('finally');
      }
    case 2:
      log.push('WRONG fallthrough');
  }
  yield log.join();
}
assert.compareArray(drain(breakOutOfSwitch()), ['in', 'finally'], 'break out of switch');

function* breakOutOfLabeledBlock() {
  var log = [];
  block: {
    try {
      yield 'in';
      break block;
    } finally {
      log.push('finally');
    }
    log.push('WRONG after break');
  }
  yield log.join();
}
assert.compareArray(drain(breakOutOfLabeledBlock()), ['in', 'finally'], 'break out of labelled block');

function* nestedFinalizers() {
  var log = [];
  for (var i = 0; i < 3; i++) {
    try {
      try {
        yield 'y' + i;
        if (i == 1) break;
      } finally {
        log.push('inner' + i);
      }
    } finally {
      log.push('outer' + i);
    }
  }
  yield log.join();
}
assert.compareArray(
  drain(nestedFinalizers()),
  ['y0', 'y1', 'inner0,outer0,inner1,outer1'],
  'nested finalizers run innermost first and the loop still terminates'
);

function* breakFromCatch() {
  var log = [];
  for (var i = 0; i < 3; i++) {
    try {
      yield i;
      throw 'boom';
    } catch (e) {
      log.push('catch' + i);
      break;
    } finally {
      log.push('finally' + i);
    }
  }
  yield log.join();
}
assert.compareArray(
  drain(breakFromCatch()),
  [0, 'catch0,finally0'],
  'break from a catch clause runs the attached finally'
);
