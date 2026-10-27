export class Secret {
    #v = 1;
    static #made = 0;
    #peek(): number {
        return this.#v;
    }
    static has(o: object): boolean {
        return #v in o;
    }
    get v(): number {
        Secret.#made++;
        return this.#peek();
    }
}
