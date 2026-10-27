export interface Point { x: number; y: number }
export enum Color { Red, Green }
export class Shape<T extends Point> {
    constructor(private readonly origin: T, public name: string = "shape") {}
    describe(): string { return this.name + ":" + this.origin.x; }
}
export const make = (x: number, y: number): Point => ({ x, y });
export const shape = new Shape(make(1, 2));
