// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-asyncgenerator-prototype-next
description: >
  Requests made to an async generator whose body is a bytecode chunk are
  queued in order: back-to-back next(), return() and throw() calls, a request
  made from inside the running body, requests on a generator that has not
  started, and requests held behind awaits in the body. The interleaving with
  other jobs pins the tick count of every settlement.
info: |
  AsyncGenerator.prototype.next ( value )

  AsyncGeneratorEnqueue appends the request; AsyncGeneratorCompleteStep removes
  the first request and settles its capability; AsyncGeneratorDrainQueue
  continues with the next one.
flags: [async]
includes: [compareArray.js]
features: [async-iteration]
---*/

var log = [];
var EXPECTED = [
  "body-start",
  "before-yield-0",
  "sync-after-enqueue",
  "body-start",
  "before-yield-0",
  "after-yield-0:b",
  "before-yield-1",
  "resumed:inner",
  "g1!before-start",
  "g2={\"done\":true}",
  "T:tick1",
  "q1={\"value\":0,\"done\":false}",
  "after-yield-1:c",
  "body-end",
  "outer1={\"value\":1,\"done\":false}",
  "m1={\"value\":0,\"done\":false}",
  "T:tick2",
  "q2={\"value\":1,\"done\":false}",
  "inner={\"value\":2,\"done\":false}",
  "outer2={\"done\":true}",
  "m2={\"value\":\"early\",\"done\":true}",
  "m3={\"done\":true}",
  "m4!late",
  "f1={\"value\":\"unwrapped\",\"done\":true}",
  "f2={\"done\":true}",
  "T:tick3",
  "q3={\"value\":\"ret\",\"done\":true}",
  "q4={\"done\":true}",
  "s1={\"value\":\"one\",\"done\":false}",
  "T:tick4",
  "T:tick5",
  "T:tick6",
  "T:tick7",
  "T:tick8",
  "s2={\"value\":\"two\",\"done\":false}",
  "s3={\"done\":true}",
  "T:tick9",
  "T:tick10",
  "T:tick11",
  "T:tick12",
  "T:tick13",
  "T:tick14",
  "T:tick15",
  "T:tick16",
  "T:tick17",
  "T:tick18",
  "T:tick19",
  "T:tick20",
  "T:tick21",
  "T:tick22",
  "T:tick23",
  "T:tick24",
  "T:tick25",
  "T:tick26",
  "T:tick27",
  "T:tick28",
  "T:tick29",
  "T:tick30",
  "T:tick31",
  "T:tick32",
  "T:tick33",
  "T:tick34",
  "T:tick35",
  "T:tick36",
  "T:tick37",
  "T:tick38",
  "T:tick39",
  "T:tick40"
];
function later(value) { return new Promise(function (resolve) { Promise.resolve().then(function () { resolve(value); }); }); }
function tick(tag, n) {
  var p = Promise.resolve();
  for (var i = 1; i <= n; i++) {
    (function (i) { p = p.then(function () { log.push(tag + ":tick" + i); }); })(i);
  }
  return p;
}
function show(tag) {
  return function (r) { log.push(tag + "=" + JSON.stringify(r)); };
}
function fail(tag) {
  return function (e) { log.push(tag + "!" + (e && e.message !== undefined ? e.message : String(e))); };
}

async function* counter(n) {
  log.push("body-start");
  for (var i = 0; i < n; i++) {
    log.push("before-yield-" + i);
    var sent = yield i;
    log.push("after-yield-" + i + ":" + sent);
  }
  log.push("body-end");
  return "ret";
}

// Requests made back to back are queued and answered in order.
var q = counter(2);
q.next("a").then(show("q1"), fail("q1"));
q.next("b").then(show("q2"), fail("q2"));
q.next("c").then(show("q3"), fail("q3"));
q.next("d").then(show("q4"), fail("q4"));
log.push("sync-after-enqueue");

// A request made from inside the body is queued behind the running one.
var self;
async function* reentrant() {
  self.next("inner").then(show("inner"), fail("inner"));
  var v = yield 1;
  log.push("resumed:" + v);
  yield 2;
}
self = reentrant();
self.next("first").then(show("outer1"), fail("outer1"));
self.next("second").then(show("outer2"), fail("outer2"));

// return() and throw() queued behind pending requests.
var mixed = counter(5);
mixed.next().then(show("m1"), fail("m1"));
mixed.return("early").then(show("m2"), fail("m2"));
mixed.next().then(show("m3"), fail("m3"));
mixed.throw(new Error("late")).then(show("m4"), fail("m4"));

// A generator never started and finished by return()/throw().
var fresh = counter(3);
fresh.return(later("unwrapped")).then(show("f1"), fail("f1"));
fresh.next().then(show("f2"), fail("f2"));
var fresh2 = counter(3);
fresh2.throw(new Error("before-start")).then(show("g1"), fail("g1"));
fresh2.next().then(show("g2"), fail("g2"));

// Awaits inside the body hold the queue.
async function* slow() {
  yield await later("one");
  await later(0);
  yield await later("two");
}
var s = slow();
s.next().then(show("s1"), fail("s1"));
s.next().then(show("s2"), fail("s2"));
s.next().then(show("s3"), fail("s3"));

tick("T", 40).then(function () {
  assert.compareArray(log, EXPECTED);
}).then($DONE, $DONE);
