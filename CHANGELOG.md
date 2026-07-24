## [0.4.8](https://github.com/pmatos/jsse/compare/v0.4.7...v0.4.8) (2026-07-24)


### Bug Fixes

* **interpreter:** deepen the String-exotic index predicate into string_exotic_index ([#383](https://github.com/pmatos/jsse/issues/383)) ([e334585](https://github.com/pmatos/jsse/commit/e334585e42812c0d919ed578b78b541ed0a6a8b4))

## [0.4.7](https://github.com/pmatos/jsse/compare/v0.4.6...v0.4.7) (2026-07-24)


### Bug Fixes

* **date:** honor TZ as the system time zone ([#379](https://github.com/pmatos/jsse/issues/379)) ([2eb7009](https://github.com/pmatos/jsse/commit/2eb7009b98a3c54c181c495760164d3cede490b5))
* **json:** include token context in parse errors ([#382](https://github.com/pmatos/jsse/issues/382)) ([3b6a03a](https://github.com/pmatos/jsse/commit/3b6a03a35ceed5bdbe9aa924ace6f83d18adf778))
* **node-shim:** match Node %s object dispatch ([#380](https://github.com/pmatos/jsse/issues/380)) ([01537c0](https://github.com/pmatos/jsse/commit/01537c00cdae09f288ce1405af9cfd34dbc7717f))
* **regexp:** preserve exact-zero capture slots ([#381](https://github.com/pmatos/jsse/issues/381)) ([c957b1c](https://github.com/pmatos/jsse/commit/c957b1cb42fc95c096b90494906878ae8d60ebe0))

## [0.4.6](https://github.com/pmatos/jsse/compare/v0.4.5...v0.4.6) (2026-07-23)


### Bug Fixes

* **regexp:** close residual nullable-alternation gaps ([#376](https://github.com/pmatos/jsse/issues/376)) ([1d578bc](https://github.com/pmatos/jsse/commit/1d578bc4a2a3bd0676294ec663c0e629b67ddac8))

## [0.4.5](https://github.com/pmatos/jsse/compare/v0.4.4...v0.4.5) (2026-07-23)


### Bug Fixes

* **interpreter:** clear tail-call eligibility by default in eval_expr ([#372](https://github.com/pmatos/jsse/issues/372)) ([716a5d0](https://github.com/pmatos/jsse/commit/716a5d06a08a7fa457a6fc06701291381cee9647))
* **regexp:** scope nullable-quantifier rewrite to nullable alternation branches ([#374](https://github.com/pmatos/jsse/issues/374)) ([8aa5729](https://github.com/pmatos/jsse/commit/8aa5729604699b815d7c518dde00d54482ab9a5c))

## [0.4.4](https://github.com/pmatos/jsse/compare/v0.4.3...v0.4.4) (2026-07-23)


### Bug Fixes

* **intl:** strip redundant script subtags in locale resolution ([#367](https://github.com/pmatos/jsse/issues/367)) ([687dbc1](https://github.com/pmatos/jsse/commit/687dbc1661943ddbca119abae98ede485b5c3e80))
* **regexp:** treat non-quantifier { as Annex B literal ([#368](https://github.com/pmatos/jsse/issues/368)) ([a9b7f60](https://github.com/pmatos/jsse/commit/a9b7f607a5444ba4999dcb18e6c630b3ebf35956))

## [0.4.3](https://github.com/pmatos/jsse/compare/v0.4.2...v0.4.3) (2026-07-23)


### Bug Fixes

* **interpreter:** guard boxing/error constructors against [[Call]] this-mutation ([#369](https://github.com/pmatos/jsse/issues/369)) ([a1f9351](https://github.com/pmatos/jsse/commit/a1f93516e93a47dbbb2b1df036ec451c286b147c))

## [0.4.2](https://github.com/pmatos/jsse/compare/v0.4.1...v0.4.2) (2026-07-23)


### Bug Fixes

* **interpreter:** deepen StringToNumber; concentrate the WhiteSpace predicate ([96ac1b0](https://github.com/pmatos/jsse/commit/96ac1b08890d20039d7c10de321fa4990cd86388))
* **interpreter:** round non-decimal strings exactly ([7f2a516](https://github.com/pmatos/jsse/commit/7f2a51683fe8cff27e0c05cef2582dfd8cc78514))

## [0.4.1](https://github.com/pmatos/jsse/compare/v0.4.0...v0.4.1) (2026-07-22)


### Bug Fixes

* **regexp:** respect unicode mode for property escapes ([b19c344](https://github.com/pmatos/jsse/commit/b19c3443aea0ebb7537f6cc570fb5f8e3c0a997f))

# [0.4.0](https://github.com/pmatos/jsse/compare/v0.3.0...v0.4.0) (2026-07-21)


### Bug Fixes

* **scripts:** compare both key sets in non-strict assert.deepEqual ([1d9175c](https://github.com/pmatos/jsse/commit/1d9175cf0ed865d28a9b13ea6752c4ce7a2ab469))


### Features

* **scripts:** add esprima Node-compat library harness ([#295](https://github.com/pmatos/jsse/issues/295)) ([f08883c](https://github.com/pmatos/jsse/commit/f08883c78d5594a8c2c605161bdf343e87ece886)), closes [#357](https://github.com/pmatos/jsse/issues/357) [#358](https://github.com/pmatos/jsse/issues/358) [#359](https://github.com/pmatos/jsse/issues/359)

# [0.3.0](https://github.com/pmatos/jsse/compare/v0.2.1...v0.3.0) (2026-07-21)


### Bug Fixes

* **bytecode:** use HTMLDDA-aware truthiness in VM jump opcodes ([2618220](https://github.com/pmatos/jsse/commit/2618220391d666fec31d9803d304361aa40c65c0))


### Features

* **bytecode:** compile numeric loops ([b25a727](https://github.com/pmatos/jsse/commit/b25a727e39fa79e9543436ee2e38c0361a22cf80))

## [0.2.1](https://github.com/pmatos/jsse/compare/v0.2.0...v0.2.1) (2026-07-21)


### Performance Improvements

* **gc:** arena-allocate JavaScript objects ([395799a](https://github.com/pmatos/jsse/commit/395799a7331ee794dd91df5e420a9df0187e6929))
* **interpreter:** pool function call environments ([#73](https://github.com/pmatos/jsse/issues/73)) ([b7196d1](https://github.com/pmatos/jsse/commit/b7196d16e1313ddb64e1760690088180b7eda2af))
* **runtime:** bound property-key intern cache ([#164](https://github.com/pmatos/jsse/issues/164)) ([87598cb](https://github.com/pmatos/jsse/commit/87598cba04ba73d77984da4bb6980d8517bce8ce))

# [0.2.0](https://github.com/pmatos/jsse/compare/v0.1.1...v0.2.0) (2026-07-20)


### Bug Fixes

* catch late repeated done callbacks ([e21c22a](https://github.com/pmatos/jsse/commit/e21c22ab09733db689a30a5081d4d6f055b5691c))
* cover remaining late done callback cases ([4c67385](https://github.com/pmatos/jsse/commit/4c673857a0bee8b4a7804367252ba48dd3835d33))
* drain late callback timers before TAP output ([f2555e4](https://github.com/pmatos/jsse/commit/f2555e42e188d8e6f3cac29b2889e8a6b77cf3d8))
* drain promise-scheduled callback timers ([aaa5a9d](https://github.com/pmatos/jsse/commit/aaa5a9dceb6a12dbe03007beb074738a7ffd5f40))
* exclude unary calls from tail position ([4d94044](https://github.com/pmatos/jsse/commit/4d94044ca1b31d4b0751520dd48b765d8f8d62eb))
* **gc:** deduplicate mark graph traversal ([2cae47b](https://github.com/pmatos/jsse/commit/2cae47bfea231ccdf2cac5aad7ab11cdd3c6cd81))
* **harness:** distinguish sparse holes in tape deepEqual ([8de47b0](https://github.com/pmatos/jsse/commit/8de47b0dbfc56b28da18efac12f8ef858531a566))
* **harness:** use enumerable-key check for tape array-index comparison ([dd9ea28](https://github.com/pmatos/jsse/commit/dd9ea28104f1501366d63226d956b051c349abb7))
* honor exotic Set in prototype-ignoring setters ([9e874ee](https://github.com/pmatos/jsse/commit/9e874ee9d596185adaa16b70da021c36def52f46))
* honor Mocha only exclusivity in test harness ([ab6f521](https://github.com/pmatos/jsse/commit/ab6f52191f46dab17776bd966c5994dacd614538))
* include receiver in readonly assignment error ([3055920](https://github.com/pmatos/jsse/commit/30559204d0c8ee4e8a22bf0cd88704b1f39d8554))
* **intl:** extend unpadded numeric hour to language-only es ([811be61](https://github.com/pmatos/jsse/commit/811be61ecee3319054826c09a22e54a756a04139))
* **intl:** handle locale decimal separators for fractionalSecond ([9899389](https://github.com/pmatos/jsse/commit/989938985628f70be5a72dbe1d40031109ae438a))
* **intl:** localize DateTimeFormat output ([9f482b5](https://github.com/pmatos/jsse/commit/9f482b5e287ea8f18d2b961440f45cd161565186))
* **intl:** preserve es-ES numeric hour width ([bb4a2f8](https://github.com/pmatos/jsse/commit/bb4a2f86a4aecff96a2ea550a43671fa80d71491))
* **intl:** preserve locale year width for dateStyle:short ([d0d3f59](https://github.com/pmatos/jsse/commit/d0d3f59cce84051a86c271c29964ac09121ba533))
* **intl:** preserve mixed DateTimeFormat field widths ([8eeb62a](https://github.com/pmatos/jsse/commit/8eeb62a5a85514bd8a5df73fb556cfeeee6a7fab))
* **intl:** preserve offset-name width in mixed DateTimeFormat patterns ([4c7f8d7](https://github.com/pmatos/jsse/commit/4c7f8d785a38512766f5b356f369b96aeac9105a))
* **intl:** reject unknown IANA time zones ([018af30](https://github.com/pmatos/jsse/commit/018af303051273bc984124faee28de6a684cc084))
* **intl:** un-pad es/es-ES numeric hour under timeStyle presets ([82142f2](https://github.com/pmatos/jsse/commit/82142f2eb1e89406da42b0b4b82ef21b73b9a713))
* **node-compat:** buffer split StringDecoder input ([f25da2e](https://github.com/pmatos/jsse/commit/f25da2e3aeb7839e08e59ed9f1b1a1739615245d))
* **node-shim:** enforce TextDecoder encoding labels ([5b2aaf9](https://github.com/pmatos/jsse/commit/5b2aaf9f0425b381b47e26802bcd595357fb9ac8))
* **parser:** don't treat optional-chain property names as await identifiers ([3c84349](https://github.com/pmatos/jsse/commit/3c84349d90ec481afd4e3861dd5d65e71ae8ec93))
* preserve binary operands across GC ([#311](https://github.com/pmatos/jsse/issues/311)) ([d1908b0](https://github.com/pmatos/jsse/commit/d1908b001cedc5e8da154497641181ac7efd515a))
* preserve lone surrogates in property keys ([d80b41e](https://github.com/pmatos/jsse/commit/d80b41e95a7e50abb17e01e9098c63177f5244fd))
* **regexp:** empty char class must match empty under zero-count quantifier ([8c109aa](https://github.com/pmatos/jsse/commit/8c109aaeb373b29d0d13cbc52419878233698e01))
* **regexp:** exclude negation marker from surrogate-range expansion ([e51645c](https://github.com/pmatos/jsse/commit/e51645cb1cae4e624f4d0e60268e72b7fad6b707)), closes [#321](https://github.com/pmatos/jsse/issues/321)
* **regexp:** preserve lone surrogates in string $N/$<name> substitution ([a68bab3](https://github.com/pmatos/jsse/commit/a68bab3cbdac59e492a51acf62be95f1ceaecfab)), closes [#321](https://github.com/pmatos/jsse/issues/321)
* **regexp:** preserve quantified empty v sets ([c0c69d6](https://github.com/pmatos/jsse/commit/c0c69d6841247ed01ded6c4a6cc6c9147dddda4d))
* register test.only.each rows as focused tests ([cc2b263](https://github.com/pmatos/jsse/commit/cc2b2639956bfcc3b804e45c6eba4e50d0709b85))
* reject nonzero engine exit in the AJV library verdict ([11a6750](https://github.com/pmatos/jsse/commit/11a675071676f610866f340e3baf15ad8d9db92e))
* reject nonzero engine exits in js-md5 verdict ([ab3c921](https://github.com/pmatos/jsse/commit/ab3c92166d41f0670662d34a14871b791eeef0a0))
* reject nonzero exits in library verdicts ([512f09c](https://github.com/pmatos/jsse/commit/512f09c4a14ea034be318a4c83233f918e55c278))
* reject repeated done callbacks in TAP harness ([bd2c6a0](https://github.com/pmatos/jsse/commit/bd2c6a0879afb94fc833e6b71ef61e0b67be75e3))
* root array literal values across GC ([0c29a84](https://github.com/pmatos/jsse/commit/0c29a843fbd7ddb142ccafd385deb0da0c5b7d66))
* root tagged template substitutions during evaluation ([8dd24b8](https://github.com/pmatos/jsse/commit/8dd24b8fb947c2bfb6fa8587d099b6246e0f5015))
* run xdescribe callback so nested skipped tests register ([ab41344](https://github.com/pmatos/jsse/commit/ab4134417f575efa10a1ab55a9f70f970a413333))
* run xdescribe callback so nested skipped tests register ([a0933eb](https://github.com/pmatos/jsse/commit/a0933eb3acdd6fc1f13253cebcd1f63cf57098d2))
* **runtime:** distinguish symbol property keys ([3cdd45f](https://github.com/pmatos/jsse/commit/3cdd45f6d9c3b182858cfc92d4b2cb5166366fdb))
* stop cyclic array joins exhausting call depth ([c5b73fc](https://github.com/pmatos/jsse/commit/c5b73fcd589d7dd76de9f1c6267c7fb6fd0a6939))
* suppress tail calls for all unary forms ([fed6640](https://github.com/pmatos/jsse/commit/fed664037ad24b57c1776344642052e89ec99057))
* unroot binary operands without dropping persistent GC roots ([03df6fe](https://github.com/pmatos/jsse/commit/03df6fe237af8e2c86a01c0277058fc1ad65e3af))


### Features

* add highlight.js compatibility harness ([f9b1e0f](https://github.com/pmatos/jsse/commit/f9b1e0f0c5bcc8ef9fcd7a6379291c9265db6311))
* add PrismJS compatibility harness ([ddcf201](https://github.com/pmatos/jsse/commit/ddcf2012c8f509ba9cc062f901b9ae36e2df1e94))
* implement per-realm Math.random PRNG ([8423c31](https://github.com/pmatos/jsse/commit/8423c312c8c881553fbd12b71dd84db617504387))
