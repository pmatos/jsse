abstract class A { abstract m(): void; private p = 1; }
class B extends A {}
new A();
const b = new B();
b.p;
interface I { a: number }
class C implements I {}
