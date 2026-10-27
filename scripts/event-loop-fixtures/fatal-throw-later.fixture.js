// RUN: 1
// The error is built on one line and thrown on another: the header is the throw site.
const err = new TypeError("built early");
function later() {
  throw err;
}
later();
