// RUN: 1
// MODE: eval
const make = () => new RangeError('made');
Promise.resolve().then(() => {
  throw make();
});
