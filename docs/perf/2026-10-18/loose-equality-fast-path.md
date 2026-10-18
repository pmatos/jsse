# Loose equality with null/undefined and same-type operands in the VM

After #1123/#1124 the largest non-property cost on richards was `x == null` / `x != null`: 1.66M evaluations went
`binary_slow` (root, call, unroot) -> `eval_binary` -> `eval_binary_slow` -> `abstract_equality`, about 450M Ir (5% of the run).
Every one was Object/Null or Null/Null, either as a plain `Eq`/`NotEq` or as the fused `CmpJumpIfFalse` of an `if`/loop condition.

## Change

`loose_equal_no_user_code` decides `l == r` when no user code can run: operands of one type (the strict comparison) and `null`/
`undefined` against anything (both nullish: true; one nullish: only an [[IsHTMLDDA]] object equals it, B.3.6.2). Mixed types that need
ToPrimitive/ToNumber return `None` and take the generic path unchanged. `Eq`/`NotEq` (through `loose_equality_op`, out of line to keep the
dispatch loop's code generation unchanged) and `CmpJumpIfFalse` use it before falling back to `binary_slow`. No `unsafe`.

## Measured (buildbox3, shipped profile, cachegrind whole-process Ir; base = #1124 `80b77029`)

| benchmark | base Ir | new Ir | delta |
|---|---:|---:|---:|
| richards | 9,063,343,332 | 8,705,409,293 | -3.95% |
| navier-stokes | 3,912,607,208 | 3,856,532,478 | -1.43% |
| hash-map | 14,009,173,891 | 13,943,863,371 | -0.47% |
| lazy-collections | 4,415,281,073 | 4,395,493,619 | -0.45% |
| Box2D | 6,022,008,820 | 6,005,492,574 | -0.27% |
| Air | 1,898,676,749 | 1,893,662,862 | -0.26% |
| crypto | 4,923,448,484 | 4,913,348,271 | -0.21% |
| micro:promise_new | 5,862,747,546 | 5,852,677,925 | -0.17% |
| delta-blue | 6,433,425,132 | 6,423,059,606 | -0.16% |
| earley-boyer | 5,182,554,653 | 5,178,673,702 | -0.07% |
| micro:regex | 117,538,054 | 117,460,971 | -0.07% |
| splay | 8,134,922,055 | 8,131,101,888 | -0.05% |
| micro:loop | 3,193,542,779 | 3,192,539,471 | -0.03% |
| micro:string | 342,979,838 | 342,878,175 | -0.03% |
| micro:closures | 5,853,881,290 | 5,852,255,062 | -0.03% |
| doxbee-async | 2,369,501,262 | 2,369,203,051 | -0.01% |
| gaussian-blur | 27,194,476,818 | 27,193,388,707 | -0.00% |
| sync-fs | 6,747,276,657 | 6,747,239,080 | -0.00% |
| micro:fib | 5,654,515,334 | 5,654,512,332 | -0.00% |
| ML | 55,098,756,194 | 55,100,077,462 | +0.00% |
| json-parse-inspector | 9,168,018,510 | 9,168,433,897 | +0.00% |
| doxbee-promise | 3,988,356,622 | 3,988,676,247 | +0.01% |
| stanford-crypto-sha256 | 4,523,570,902 | 4,525,208,466 | +0.04% |
| bigint-bigdenary | 4,505,562,415 | 4,507,220,691 | +0.04% |
| micro:closure_create | 2,627,774,662 | 2,628,814,342 | +0.04% |
| Basic | 3,553,885,540 | 3,555,318,058 | +0.04% |
| async-fs | 7,602,136,300 | 7,605,690,412 | +0.05% |
| raytrace | 12,286,374,527 | 12,293,633,002 | +0.06% |
| bigint-noble-secp256k1 | 6,286,475,374 | 6,290,199,620 | +0.06% |
| Babylon | 2,476,743,700 | 2,478,333,130 | +0.06% |
| micro:opmix | 53,087,267,907 | 53,124,350,942 | +0.07% |
| bigint-noble-bls12-381 | 60,083,478,440 | 60,134,257,894 | +0.08% |
| micro:object | 1,477,136,843 | 1,479,019,987 | +0.13% |
| micro:json | 313,967,045 | 314,411,607 | +0.14% |
| bigint-noble-ed25519 | 7,152,171,350 | 7,165,485,164 | +0.19% |
| ai-astar | 10,672,330,182 | 10,701,539,130 | +0.27% |
| micro:array | 757,314,694 | 761,187,606 | +0.51% |
| js-tokens | 4,799,599,827 | 4,877,021,479 | +1.61% |
| proxy-mobx | 576,800,773 | 596,483,409 | +3.41% |
| proxy-vue | 1,169,878,627 | 1,212,217,309 | +3.62% |

Geometric mean over 40 workloads: +0.06%: the rest is layout noise (proxy-mobx/proxy-vue vary by +-5% between runs of one binary
because their Ir depends on randomised hashing). richards is the workload whose hot loop is `!= null` tests.
