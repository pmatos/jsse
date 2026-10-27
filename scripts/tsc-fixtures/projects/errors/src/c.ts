type Rec = { next: Rec } & string;
let s: string = 5 as string;
const v = <T,>(x: T): T => x;
v<number, string>(1);
declare const k: keyof { a: 1 };
k = "b";
async function af(): Promise<number> { return "no"; }
for (const q of 5) {}
class D extends Object { constructor() { this.x = 1; super(); } }
