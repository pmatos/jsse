/*---
description: >
  A break or continue that leaves a try statement containing a yield or await
  in an async generator runs the try's finally block before control reaches the
  loop or labelled statement.
esid: sec-try-statement-runtime-semantics-evaluation
info: |
  Evaluation of a try-finally statement evaluates the Finally clause for
  every completion of its try Block, including break and continue completions.
  If the Finally clause completes normally, the original completion is
  restored and propagates to the enclosing loop or labelled statement.
flags: [async]
includes: [compareArray.js, asyncHelpers.js]
features: [async-iteration]
---*/

async function drain(iterator) {
  var results = [];
  for (var step = await iterator.next(); !step.done; step = await iterator.next()) {
    results.push(step.value);
  }
  return results;
}

async function* breakOutOfLoop() {
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

async function* awaitInTryThenBreak() {
  var log = [];
  for (var i = 0; i < 4; i++) {
    try {
      await null;
      if (i == 1) break;
    } finally {
      log.push(i);
    }
  }
  yield log.join();
}

async function* continueLoop() {
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

async function* labeledContinue() {
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

async function* labeledBreak() {
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

async function* breakOutOfWhile() {
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

async function* breakOutOfDoWhile() {
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

async function* breakOutOfSwitch() {
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

async function* breakOutOfLabeledBlock() {
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

async function* nestedFinalizers() {
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

async function* breakFromCatch() {
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

async function main() {
  assert.compareArray(await drain(breakOutOfLoop()), [0, 1, '0,1'], 'break out of for');
  assert.compareArray(await drain(awaitInTryThenBreak()), ['0,1'], 'await in try, then break');
  assert.compareArray(
    await drain(continueLoop()),
    [0, 1, 2, 'body0,finally0,finally1,body2,finally2'],
    'continue in for'
  );
  assert.compareArray(
    await drain(labeledContinue()),
    ['0:0', '1:0', '2:0', 'f00,f10,f20'],
    'continue to an outer label'
  );
  assert.compareArray(await drain(labeledBreak()), ['0:0', 'f00'], 'break to an outer label');
  assert.compareArray(await drain(breakOutOfWhile()), [0, 1, 'f1,f2'], 'break out of while');
  assert.compareArray(await drain(breakOutOfDoWhile()), [0, 1, 'f1,f2'], 'break out of do-while');
  assert.compareArray(await drain(breakOutOfSwitch()), ['in', 'finally'], 'break out of switch');
  assert.compareArray(
    await drain(breakOutOfLabeledBlock()),
    ['in', 'finally'],
    'break out of labelled block'
  );
  assert.compareArray(
    await drain(nestedFinalizers()),
    ['y0', 'y1', 'inner0,outer0,inner1,outer1'],
    'nested finalizers run innermost first and the loop still terminates'
  );
  assert.compareArray(
    await drain(breakFromCatch()),
    [0, 'catch0,finally0'],
    'break from a catch clause runs the attached finally'
  );
}

asyncTest(main);
