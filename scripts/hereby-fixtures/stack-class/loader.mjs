export class Loader {
  static create() {
    return new Loader();
  }
  constructor() {
    this.items = [1, 2];
  }
  load() {
    let last;
    for (const n of this.items) last = this.check(n);
    return last;
  }
  check(n) {
    if (n === 2) {
      throw new Error("bad item " + n);
    }
    return n;
  }
}
