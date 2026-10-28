export const n: number = "text";
export function f(p: { a: number }): string {
    return p.a;
}
const missing = unknownName + 1;
export class C {
    private secret = 1;
    method(): void {}
}
new C().secret;
new C().nope();
export function g(x) {
    return x.y;
}
let unused: number;
