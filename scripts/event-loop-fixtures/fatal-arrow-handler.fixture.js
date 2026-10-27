// RUN: 7
// An arrow handler of a process event is called with the process as receiver.
process.on('uncaughtException', () => {
  throw new Error('thrown by a handler');
});
throw new Error('first');
