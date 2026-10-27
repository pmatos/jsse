// RUN: 1
// An anonymous function held by a prototype property is named after it.
const proto = {
  run: function () {
    throw new Error("from run");
  },
};
const instance = Object.create(proto);
instance.run();
