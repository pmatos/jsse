// Dependency for SetterThatIgnoresPrototypeProperties-module-namespace-set.mjs.
// The exported names ensure the namespace reports writable own data
// descriptors even though its [[Set]] internal method always returns false.

export let stack = "original stack";
export let constructor = "original constructor";
