# #931 frame-free `get x() { return this.#x; }`

After the id-based private names (see `2026-10-13/931-private-names.md`) the
remaining gap between raytrace-private-class-fields and raytrace-public-class-fields
was the 20 `get x() { return this.#x; }` accessors the private variant reads its
fields through: 1.28M extra compiled calls (3.3M vs 2.0M), each building an
environment, binding `this`, pushing a call frame and running a one-op chunk.

The bytecode compiler now recognises a function body that is exactly
`return this.#name;` (`Chunk::trivial_private_getter`). `Interpreter::call_getter`
(used by the ordinary [[Get]] paths in `property.rs`) serves such a getter by
resolving the Private Name from the closure's environment and calling
`private_get_resolved` on the receiver, in the getter's realm. The getter runs
no user code of its own, so no frame is observable; a private *accessor* the
getter reads still goes through `call_function`, which keeps the call-depth
guard on mutually recursive private accessors. Arrow, generator, async, sloppy,
proxy and wrapped functions, and getters not yet compiled, take the normal path.

Counters (`--features perf-counters`, rt_private): `body_dispatch_compiled`
2,031,133 (was 3.3M), `trivial_getter_hits` 1,281,430; rt_public 2,031,113.

Measured on this host, interleaved, median of 5, CPU seconds, before/after/boa 0.22
(raytrace files are one `runIteration`):

| repro | before | after | boa | after/boa |
|---|---|---|---|---|
| raytrace-private-class-fields | 2.096 | 1.911 | 3.345 | 0.57x |
| raytrace-public-class-fields | 1.920 | 1.918 | 2.799 | 0.69x |
| private_access (issue repro) | 0.149 | 0.150 | 0.271 | 0.55x |
| 2M x (2 getters + 1 setter) micro | 2.055 | 1.462 | 1.814 | 0.81x |

raytrace-private / raytrace-public: 1.09 before (1.20 in the #994 run), 1.00
after. Setters are not inlined; the micro's remaining cost is its setter.
