using first = {
  [Symbol.dispose]() {
    globalThis.moduleUsingDisposalLog.push('first');
  }
};

using second = {
  [Symbol.dispose]() {
    globalThis.moduleUsingDisposalLog.push('second');
    throw globalThis.moduleUsingDisposeError;
  }
};

throw globalThis.moduleUsingBodyError;
