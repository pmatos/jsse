const thrower = require("./thrower");
module.exports = { run };
function run() {
  return thrower.inner();
}
