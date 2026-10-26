'use strict';

// Node's fixed_queue is a segmented ring buffer; this keeps its push, shift and
// isEmpty contract with an array and a head index.
class FixedQueue {
  constructor() {
    this.items = [];
    this.head = 0;
  }

  isEmpty() {
    return this.head === this.items.length;
  }

  push(item) {
    this.items.push(item);
  }

  shift() {
    if (this.head === this.items.length) return null;
    const item = this.items[this.head];
    this.items[this.head++] = undefined;
    if (this.head === this.items.length) {
      this.items = [];
      this.head = 0;
    }
    return item;
  }
}

module.exports = FixedQueue;
