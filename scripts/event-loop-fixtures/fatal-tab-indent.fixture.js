// RUN: 1
// Tabs in the source line are kept so the caret lines up.
function f() {
	if (true) { throw new Error("tabbed"); }
}
f();
