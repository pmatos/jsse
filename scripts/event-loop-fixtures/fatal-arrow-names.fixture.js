// RUN: 1
// An arrow's frame is named after the receiver it was called with, as V8
// does, not after its lexical this.
const holder = {
  method: () => new Error('m'),
};
const plain = () => new Error('p');
const top = (label) => (label === 'x' ? undefined : new Error(label));
const rows = [
  holder.method(),
  plain(),
  top.call({ a: 1 }, 'c'),
  top.call(holder, 'h'),
  top.call(function Named() {}, 'f'),
];
for (const error of rows) {
  console.log(error.stack.split('\n')[1].trim());
}
process.on('exit', () => {
  throw new Error('thrown by an exit handler');
});
