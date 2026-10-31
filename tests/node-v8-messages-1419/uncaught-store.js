// A store through undefined: V8 points at the `=`.
const state = {};
function save() {
  state.user.name = "x";
}
save();
