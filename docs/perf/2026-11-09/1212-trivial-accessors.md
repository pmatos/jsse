# #1212: class getter/setter calls

Base `b2d977b0`, patched = this commit, buildbox3 (load about 2), all engines pinned to one core, 9 interleaved
runs, fat-LTO release builds (nightly), Boa HEAD `64c209f1`. Wall median in ms
(min-max).

| workload | base | patched | Boa HEAD | base / Boa | patched / Boa |
|---|---:|---:|---:|---:|---:|
| `proxy-classes/getter_setter` (the issue fixture) | 880 (877-888) | 388 (383-389) | 716 (708-761) | 1.23 | 0.54 |
| same loop inside a function | 788 | 304 | 696 | 1.13 | 0.44 |
| accessor bodies that do more than `this.x` (`this._x + 0`) | 817 | 721 | 750 | 1.09 | 0.96 |
| `proxy-classes/proxy_ownkeys_has` | 1261 | 1262 | 1107 | 1.14 | 1.14 |
| control: `proxy-classes/public_access` (no accessors) | 342 | 346 | 493 | 0.69 | 0.70 |
| control: `bench_object.js` (no accessors) | 345 | 342 | 477 | 0.72 | 0.72 |

The Proxy `ownKeys`/`has` row is a different path (trap dispatch) and is untouched.

Callgrind Ir on the repro inside a function (counters build, `release-nolto`): 4.00 G
before, 1.80 G after. User-function calls: 1,500,002 before, 6 after.

## What changed

- `SetProp` finds an accessor's setter with `named_setter_fast` (an ordinary-object chain walk
  that reads only) and calls it with the original receiver. Before, it ran the whole generic
  [[Set]] with a descriptor copy. Alone this is 7% of Ir on the repro and 4% on non-trivial bodies.
- A getter whose body is exactly `return this.name;` and a setter whose body is exactly
  `this.name = param;` skip the call frame when the receiver is an object and
  `named_get_fast`/`named_store_fast` complete in place. They run no user code, so the frame is
  unobservable. Any other case (inherited accessor, Proxy, primitive receiver, read-only or
  non-extensible target, parameter default) takes the normal call.
- `trivial_accessor_hits` counts frame-less accessor calls (1,499,996 of the repro's 1.5 M).

Behaviour is pinned by `test262-extra/trivial-accessor-body-fallbacks.js`, which passes on
both the base and patched builds.
