var a = 1, c = 2, e = 3, g = 4;
module.exports = {
  a,
  b: c,
  'd': e,
  ...require('./dot.cjs'),
  f: function () {},
  g,
};
