function* gen() {
  const x = yield 1;
  show("received", x);
  try { yield 2; } finally { show("cleanup"); }
  yield 3;
}
const it = gen();
show(it.next(), it.next("hello"), it.return(42), it.next());
function* inner() { yield "a"; return "inner-done"; }
function* outer() { const r = yield* inner(); yield r; }
show([...outer()]);
const iterable = { *[Symbol.iterator]() { yield* [1, 2, 3]; } };
const [first, ...rest] = iterable;
show(first, rest, Array.from(iterable, (x) => x * 10));
const closing = {
  [Symbol.iterator]() {
    let i = 0;
    return { next: () => ({ value: i++, done: false }), return: () => { show("return called"); return {}; } };
  },
};
for (const v of closing) { if (v > 1) break; }
const [p, q] = closing;
show(p, q);
function* infinite() { let i = 0; while (true) yield i++; }
show(infinite().take(3).toArray(), infinite().filter((x) => x % 2).map((x) => x * x).take(3).toArray());
