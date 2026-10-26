'use strict';

// Calls process.emitWarning when the prelude provides it (#1229). Until then
// this is the same contract for an Error: the 'warning' event on the next tick,
// preceded by the default stderr line.

let traceHintShown = false;

function emitWarning(warning) {
  if (typeof process.emitWarning === 'function') {
    process.emitWarning(warning);
    return;
  }
  process.nextTick(() => {
    let text = `(node:${process.pid}) ${warning.name}: ${warning.message}\n`;
    if (!traceHintShown) {
      traceHintShown = true;
      text += '(Use `node --trace-warnings ...` to show where the warning was created)\n';
    }
    process.stderr.write(text);
    process.emit('warning', warning);
  });
}

module.exports = { emitWarning };
