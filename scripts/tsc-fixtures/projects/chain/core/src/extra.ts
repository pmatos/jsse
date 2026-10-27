export class Counter {
    private n = 0;
    next(): number {
        return ++this.n;
    }
}
