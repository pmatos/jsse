export class Loader {
  static create() {
    return new Loader();
  }
  constructor() {
    this.items = [1, 2];
  }
  load() {
    return this.items.map((n) => this.check(n));
  }
  check(n) {
    if (n === 2) {
      throw new Error("bad item " + n);
    }
    return n;
  }
}
