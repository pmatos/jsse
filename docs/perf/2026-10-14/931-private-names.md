# #931 id-based private names and bytecode `#x` ops

Private names were branded `String`s (`"x#<class id>"`) resolved by walking the
environment chain and cloned on every access, stored in a hashed
`String`-keyed map per object, and `#x` forms stayed on the tree-walker (44 of
63 compile bails in raytrace-private-class-fields).

Now each class evaluation creates a `PrivateId` (u64) per source name
(`PrivateNameScope` on the class environment); objects hold their private
elements in a linear `PrivateFields` vector keyed by id. The bytecode compiler
lowers `o.#x`, `o.#x = v`, `o.#x op= v`, `++o.#x`, `o.#m(...)` and `#x in o`
to `GetPrivate`/`SetPrivate`/`UpdatePrivate`/`LoadMethodPrivate`/`PrivateIn`;
the VM resolves each chunk private-name index to its id once per frame.

Compound assignment on a private reference now reads the element before it
evaluates the right-hand side (13.15.2), in both engines.

Measured on this host, interleaved, median of 5, CPU seconds, before/after/boa 0.22:

| repro | before | after | boa | after/boa |
|---|---|---|---|---|
| private_access (issue repro) | 0.378 | 0.152 | 0.281 | 0.54x |
| new_class_private_fields | 1.022 | 0.801 | 1.108 | 0.72x |
| raytrace-private-class-fields (1 iteration) | 3.075 | 2.393 | 3.106 | 0.77x |
| raytrace-public-class-fields (1 iteration) | 2.086 | 1.986 | 2.928 | 0.68x |

raytrace-private / raytrace-public: 1.47x before, 1.20x after. The remaining
gap is the 20 `get`/`set` accessor pairs the private variant routes field reads
through (3.3M vs 2.0M compiled calls); `FALLBACK`/`BAIL` rows for private
accesses are gone.
