// RUN: 0
// MODE: eval
// Node prefixes the stack of what the -e program throws with the excerpt, so
// a handler of uncaughtException sees it there.
process.on('uncaughtException', (e) => {
  console.log(e.stack.split('\n').filter((l) => !l.includes('node:')).join('\n'));
});
throw new Error('handled in eval');
