// Array.prototype.join (§23.1.3.18) must not recurse until the engine's
// call-depth guard fires when element stringification reaches the same active
// join receiver.

function sameValue(actual, expected, label) {
  if (actual !== expected) {
    throw new Error(label + ": expected " + JSON.stringify(expected) +
      ", got " + JSON.stringify(actual));
  }
}

var selfCycle = [];
selfCycle.push(1, selfCycle, 2);
sameValue(selfCycle.join("-"), "1--2", "direct cycle");

var left = [];
var right = [left];
left.push(right);
sameValue(left.join(), "", "mutual cycle");

var generic = { length: 1 };
generic[0] = generic;
generic.join = Array.prototype.join;
generic.toString = Array.prototype.toString;
sameValue(String(generic), "", "generic cycle");

// Separator coercion precedes element conversion, so a reentrant join from the
// separator is not yet cyclic and must still inspect the receiver.
var separatorLog = [];
var separatorArray = [1];
var separator = {
  toString: function () {
    separatorLog.push(separatorArray.join("-"));
    return ",";
  },
};
sameValue(separatorArray.join(separator), "1", "separator result");
sameValue(separatorLog.join(), "1", "separator reentrant join");

// An abrupt element conversion must remove the receiver from the active stack.
var afterThrow = [{
  toString: function () {
    throw new Error("sentinel");
  },
}];
try {
  afterThrow.join();
} catch (error) {
  sameValue(error.message, "sentinel", "element error");
}
afterThrow[0] = "recovered";
sameValue(afterThrow.join(), "recovered", "active join state unwinds");
