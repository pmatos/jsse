// Mini "Vue reactive()" reduction: Proxy get/set traps with WeakMap<target, Map<key, Set<effect>>>
// dependency tracking, an active-effect stack, and effects that re-run on write.
// Models JetStream proxy-vue (Vue 3 reactivity: track/trigger/Reflect.get/Reflect.set/has).
// Measured 2026-10-10, jsse HEAD 2b621c5 vs boa 0.22.0, buildbox3 5-run median: jsse 5.09 s / boa 1.52 s = 3.34x (jsse --no-bytecode 5.14 s).
"use strict";
const targetMap = new WeakMap();
let activeEffect = null;
function track(target, key) {
    if (!activeEffect) return;
    let depsMap = targetMap.get(target);
    if (!depsMap) targetMap.set(target, depsMap = new Map());
    let dep = depsMap.get(key);
    if (!dep) depsMap.set(key, dep = new Set());
    dep.add(activeEffect);
}
function trigger(target, key) {
    const depsMap = targetMap.get(target);
    if (!depsMap) return;
    const dep = depsMap.get(key);
    if (!dep) return;
    for (const e of [...dep]) e();
}
function reactive(obj) {
    return new Proxy(obj, {
        get(t, key, receiver) {
            const r = Reflect.get(t, key, receiver);
            track(t, key);
            return (typeof r === "object" && r !== null) ? reactive(r) : r;
        },
        set(t, key, value, receiver) {
            const old = t[key];
            const ok = Reflect.set(t, key, value, receiver);
            if (old !== value) trigger(t, key);
            return ok;
        },
        has(t, key) { track(t, key); return Reflect.has(t, key); },
    });
}
function effect(fn) { const run = () => { const prev = activeEffect; activeEffect = run; try { fn(); } finally { activeEffect = prev; } }; run(); return run; }

const state = reactive({ count: 0, inner: { x: 1, y: 2 }, list: [1, 2, 3, 4, 5] });
let total = 0;
effect(() => { total += state.count + state.inner.x + state.inner.y; });
effect(() => { let s = 0; for (let i = 0; i < state.list.length; i++) s += state.list[i]; total += s; });
const N = 6000;
for (let i = 1; i <= N; i++) {
    state.count = i;
    state.inner.x = i & 15;
    state.list[i % 5] = i & 31;
}
console.log("proxy_vue_reactive", total);
