export class Stack<T> {
    private data: T[] = [];
    push(v: T): void {
        this.data.push(v);
    }
    pop(): T | undefined {
        return this.data.pop();
    }
    peek(): T | undefined {
        return this.data[this.data.length - 1];
    }
    get size(): number {
        return this.data.length;
    }
}
