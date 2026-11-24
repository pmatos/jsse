const log = [];
const target = { a: 1 };
const handler = {};
for (const trap of ["get", "set", "has", "deleteProperty", "ownKeys", "getOwnPropertyDescriptor", "defineProperty"]) {
  handler[trap] = (...args) => { log.push(trap); return Reflect[trap](...args); };
}
const p = new Proxy(target, handler);
p.a; p.b = 2; "a" in p; delete p.a; Object.keys(p); ({ ...p });
show(log.join(","));
show(Reflect.ownKeys(target), Reflect.has(target, "b"), Reflect.getPrototypeOf(p) === Object.prototype);
const revocable = Proxy.revocable({}, {});
revocable.revoke();
try { revocable.proxy.x; } catch (e) { show(e.name); }
show(Reflect.apply(Math.max, null, [1, 3, 2]), Reflect.construct(Date, [0]).getTime());
const fnProxy = new Proxy(function () { return "called"; }, { apply: (t, th, args) => "trapped " + args.length });
show(fnProxy(1, 2), typeof fnProxy);
