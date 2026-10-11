// bench_bigint_bigdenary_numeric_control.js
// CONTROL (BigInt-free): same class/constructor/object-literal/getter structure and the same
//   call pattern as bench_bigint_bigdenary_real.js, but the `base` field is a Number.
//   If jsse/boa here is ~ the ratio of the real BigDenary repro, BigInt arithmetic is NOT the
//   cause of the bigint-bigdenary gap; it is generic call/construct/property overhead.
// Models: bigint-bigdenary (shape only).
// Measured 2026-10-10, jsse 2b621c5 (v0.11.0) vs boa 0.22.0 on buildbox3 (EPYC 7501), median of 5:
//   jsse 3.79 s / boa 1.34 s = 2.82x slower; jsse --no-bytecode 3.89 s; loadavg1 <= 9.8.
// Role: SHAPE CONTROL (BigInt-free; same ratio as the BigInt twin = BigInt is not the cause).
function countTrailingZeros(n, upTo) {
  if (n === 0) return 0;
  let count = 0;
  let c = n < 0 ? n * -1 : n;
  while (c % 10 === 0 && count < upTo) {
    count += 1;
    c = c / 10;
  }
  return count;
}
class Dec {
  constructor(n) {
    if (n instanceof Dec) {
      this.base = n.base;
      this._decimals = n.decimals;
    } else if (typeof n === "object" && n !== null) {
      if (n.decimals < 0) throw new Error("bad");
      this.base = n.base;
      this._decimals = n.decimals;
    } else {
      this.base = n;
      this._decimals = 0;
    }
    this.trimTrailingZeros();
  }
  get decimals() { return this._decimals; }
  get decimalMultiplier() { return Dec.getDecimalMultiplier(this._decimals); }
  static getDecimalMultiplier(d) { return Math.pow(10, d); }
  scaleDecimalsTo(d) {
    if (d > this._decimals) {
      this.base = this.base * Dec.getDecimalMultiplier(d - this._decimals);
    } else if (d < this._decimals) {
      const m = Dec.getDecimalMultiplier(this._decimals - d);
      const rem = this.base % m;
      this.base = (this.base - rem) / m;
      if (Math.abs(rem * 2) >= m) this.base += this.base >= 0 ? 1 : -1;
    }
    this._decimals = d;
  }
  trimTrailingZeros() {
    const z = countTrailingZeros(this.base, this.decimals);
    if (z > 0) this.scaleDecimalsTo(this.decimals - z);
  }
  plus(o) {
    const curr = new Dec(this);
    const oper = new Dec(o);
    const t = Math.max(curr.decimals, oper.decimals);
    curr.scaleDecimalsTo(t);
    oper.scaleDecimalsTo(t);
    return new Dec({ base: curr.base + oper.base, decimals: t });
  }
  negated() { return new Dec({ base: this.base * -1, decimals: this.decimals }); }
  minus(o) { return this.plus(new Dec(o).negated()); }
  multipliedBy(o) {
    const curr = new Dec(this);
    const oper = new Dec(o);
    return new Dec({ base: curr.base * oper.base, decimals: curr.decimals + oper.decimals });
  }
  dividedBy(o) {
    const curr = new Dec(this);
    const oper = new Dec(o);
    const t = Math.max(curr.decimals * 2, oper.decimals * 2, 20);
    curr.scaleDecimalsTo(t);
    return new Dec({ base: Math.floor(curr.base / oper.base), decimals: curr.decimals - oper.decimals });
  }
  comparedTo(o) {
    const curr = new Dec(this);
    const comp = new Dec(o);
    const t = Math.max(curr.decimals, comp.decimals);
    curr.scaleDecimalsTo(t);
    comp.scaleDecimalsTo(t);
    return curr.base > comp.base ? 1 : curr.base < comp.base ? -1 : 0;
  }
  equals(o) { return this.comparedTo(o) === 0; }
}
const d1 = new Dec({ base: 89651684856225, decimals: 7 });
const d2 = new Dec({ base: 24809862135494, decimals: 7 });
const eq = (a, b) => a.equals(b);
function firstAndLastEqual(thunk, areEqual = eq) {
  const first = thunk();
  var last;
  for (var i = 0; i < 1e4; i++) last = thunk();
  return areEqual(first, last);
}
let ok = true;
ok = ok && firstAndLastEqual(() => d1.plus(d2));
ok = ok && firstAndLastEqual(() => d1.minus(d2));
ok = ok && firstAndLastEqual(() => d1.negated());
ok = ok && firstAndLastEqual(() => d1.comparedTo(d2), Object.is);
ok = ok && firstAndLastEqual(() => d1.multipliedBy(d2));
ok = ok && firstAndLastEqual(() => d1.dividedBy(d2));
console.log("bigdenary_numeric", ok, d1.plus(d2).base, d1.dividedBy(d2).base);
