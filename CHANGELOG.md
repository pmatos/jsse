# [0.7.0](https://github.com/pmatos/jsse/compare/v0.6.0...v0.7.0) (2026-09-06)


### Bug Fixes

* **harness:** await callback-style tape completion ([#576](https://github.com/pmatos/jsse/issues/576)) ([bdc46f5](https://github.com/pmatos/jsse/commit/bdc46f5dfcba380bfeb12c5cef4392317007b10e))
* **intl:** add region-sensitive calendar preferences ([#587](https://github.com/pmatos/jsse/issues/587)) ([f10d351](https://github.com/pmatos/jsse/commit/f10d351d1d04e5486e6eed909ff319fce1748be8))
* **intl:** honor Intl.Locale region overrides ([#564](https://github.com/pmatos/jsse/issues/564)) ([d747e3e](https://github.com/pmatos/jsse/commit/d747e3e998aafd36fe2476d2036047521931dd8c))
* **intl:** honor language-specific hour cycle preferences ([#589](https://github.com/pmatos/jsse/issues/589)) ([e03bc43](https://github.com/pmatos/jsse/commit/e03bc434e0645f640280a5acb84c1e30cbe487e6))
* **iterator:** create validation errors before IteratorClose ([#590](https://github.com/pmatos/jsse/issues/590)) ([995c6b3](https://github.com/pmatos/jsse/commit/995c6b3dd752e691ae7dacd1417be43973efba67))
* **iterator:** keep cached next methods alive across helpers ([#591](https://github.com/pmatos/jsse/issues/591)) ([e9437de](https://github.com/pmatos/jsse/commit/e9437de6c45ed40fc3d2c656a4c725df4e4b9292))
* **iterator:** keep flatMap inner iterator GC-rooted ([#585](https://github.com/pmatos/jsse/issues/585)) ([8ffe5a3](https://github.com/pmatos/jsse/commit/8ffe5a3728379c13f9ba9ca7756b7aabbd44ae19))
* **iterator:** reject take/drop limits above 2**53 - 1 ([#545](https://github.com/pmatos/jsse/issues/545)) ([4a49b84](https://github.com/pmatos/jsse/commit/4a49b84bbaf5236c6e9ea67c7a8a4c985c69c02e))
* **module:** dispose top-level using resources ([#583](https://github.com/pmatos/jsse/issues/583)) ([c7bab4a](https://github.com/pmatos/jsse/commit/c7bab4a03b9270836d60bbfb319ccabed003eac2))
* **module:** stop module DFS after a dependency's __host_exit ([#601](https://github.com/pmatos/jsse/issues/601)) ([a0ef691](https://github.com/pmatos/jsse/commit/a0ef691dbfd39359b33e0fb6cfc8936929988613))
* **modules:** wait for deferred async cycle dependencies ([#567](https://github.com/pmatos/jsse/issues/567)) ([bcf04a9](https://github.com/pmatos/jsse/commit/bcf04a9c5cda1e5ee906518daa240afbba21d545))
* **parser:** reject trailing commas after object rest patterns ([#565](https://github.com/pmatos/jsse/issues/565)) ([d7ce4d0](https://github.com/pmatos/jsse/commit/d7ce4d085460d7e89c1c1b7dd79e6340c4d2d4ec))
* **promise:** preserve Promise.try returned promise identity ([#566](https://github.com/pmatos/jsse/issues/566)) ([4b38491](https://github.com/pmatos/jsse/commit/4b3849137d84ab711a019087a5d3967442fe8bf1))
* **promise:** root combinator capabilities during synchronous setup ([#577](https://github.com/pmatos/jsse/issues/577)) ([7f74a4f](https://github.com/pmatos/jsse/commit/7f74a4fdc2988d1aa98722f9261dc472b35d657a))
* **regexp:** preserve genuine supplementary PUA code points ([#579](https://github.com/pmatos/jsse/issues/579)) ([fdc21f8](https://github.com/pmatos/jsse/commit/fdc21f8a2d7afebc42af840b253774d15d9265d0))
* **test262:** fail collection on unreadable directories ([#586](https://github.com/pmatos/jsse/issues/586)) ([c5f44f0](https://github.com/pmatos/jsse/commit/c5f44f0a3b611cc1d8746a45ae323d0fae17cbf6))
* **test262:** stop killed runs leaving scratch files in the submodule ([#546](https://github.com/pmatos/jsse/issues/546)) ([e17a15c](https://github.com/pmatos/jsse/commit/e17a15c67e9cbcf70bdf7627b747de488be7749f)), closes [#559](https://github.com/pmatos/jsse/issues/559)


### Features

* **fuzz:** add cargo-fuzz parser and differential-vs-node targets ([#598](https://github.com/pmatos/jsse/issues/598)) ([d6c4d75](https://github.com/pmatos/jsse/commit/d6c4d759747d8b9aa5f1e611ab826eb163377c1e))
* **iterator:** implement chunks and windows helpers ([#557](https://github.com/pmatos/jsse/issues/557)) ([bf9c191](https://github.com/pmatos/jsse/commit/bf9c1916c2ecc69285c535a53dddb631c5654ec5))
* **iterator:** implement Iterator.prototype.includes ([#558](https://github.com/pmatos/jsse/issues/558)) ([ae39edd](https://github.com/pmatos/jsse/commit/ae39edd3a9b832a1a8214531240c9706dc4d803c)), closes [#569](https://github.com/pmatos/jsse/issues/569) [#571](https://github.com/pmatos/jsse/issues/571) [#549](https://github.com/pmatos/jsse/issues/549)
* **iterator:** implement Iterator.prototype.join ([#561](https://github.com/pmatos/jsse/issues/561)) ([fc6ffd9](https://github.com/pmatos/jsse/commit/fc6ffd9122c01c0ad1f2558da622b09da8cb0a91))
* **release:** reconcile orphaned release tags after publish failures ([#600](https://github.com/pmatos/jsse/issues/600)) ([642f9db](https://github.com/pmatos/jsse/commit/642f9dbc82c8b499d20552985a93c9e5af34d1a4))


### Performance Improvements

* **bytecode:** compile this expressions ([#578](https://github.com/pmatos/jsse/issues/578)) ([92669f3](https://github.com/pmatos/jsse/commit/92669f31328cb6153cca418d1a853bd4a9888380))
* **bytecode:** fast-path numeric element reads ([#580](https://github.com/pmatos/jsse/issues/580)) ([cea2e1c](https://github.com/pmatos/jsse/commit/cea2e1c4d156cd25a34324dfdb14d034ad194faa))
* **counters:** resolve generator/async BODY rows to function names ([#581](https://github.com/pmatos/jsse/issues/581)) ([f93e7a3](https://github.com/pmatos/jsse/commit/f93e7a3687fc842ad5f5953e4345fa57115e23e6))

# [0.6.0](https://github.com/pmatos/jsse/compare/v0.5.0...v0.6.0) (2026-08-30)


### Bug Fixes

* **array:** enumerate dynamically added elements ([#520](https://github.com/pmatos/jsse/issues/520)) ([908365a](https://github.com/pmatos/jsse/commit/908365a7ca0edd2708c4874837781c30dbb8527c))
* **gc:** keep pending promise resolvers reachable ([#499](https://github.com/pmatos/jsse/issues/499)) ([fad328f](https://github.com/pmatos/jsse/commit/fad328ff90dc7de2095b7f4d1adaeef82de3046d))
* **interpreter:** bound per-body inline-cache storage ([#503](https://github.com/pmatos/jsse/issues/503)) ([f5de46e](https://github.com/pmatos/jsse/commit/f5de46ee712920635cace3754ccdfe7e3d45740a))
* **interpreter:** bound proxy prototype cycles ([#519](https://github.com/pmatos/jsse/issues/519)) ([ec908a8](https://github.com/pmatos/jsse/commit/ec908a8fff01fff57a6e122428bc43ba481646e9))
* **interpreter:** discard exited async for-of bindings ([#504](https://github.com/pmatos/jsse/issues/504)) ([15e55df](https://github.com/pmatos/jsse/commit/15e55dfa007626141e50c58b6983a7a5c8682eb5))
* **interpreter:** honor proxy descriptors in enumerable own keys ([#490](https://github.com/pmatos/jsse/issues/490)) ([fbf02c8](https://github.com/pmatos/jsse/commit/fbf02c88d2ba12a46931484fd57dcfe8cd888a20))
* **interpreter:** preserve generator for-of lexical bindings ([#508](https://github.com/pmatos/jsse/issues/508)) ([d329a4b](https://github.com/pmatos/jsse/commit/d329a4b3ac4a5f38df0f903c33e33c8711cfe16a))
* **interpreter:** preserve nested async for-of environments ([#486](https://github.com/pmatos/jsse/issues/486)) ([3590776](https://github.com/pmatos/jsse/commit/3590776cefe9e0b7d42497e843c35b98faaee48b))
* **interpreter:** preserve try clause continuation states ([#507](https://github.com/pmatos/jsse/issues/507)) ([8adc42f](https://github.com/pmatos/jsse/commit/8adc42f0fca03e66197fbae017a05cf3fc6e55f8))
* **interpreter:** resolve inline labelled continue targets ([#510](https://github.com/pmatos/jsse/issues/510)) ([07c2600](https://github.com/pmatos/jsse/commit/07c2600ca82bec33ebe5a630e0fc203ed8db8379))
* **interpreter:** root computed member target base ([#514](https://github.com/pmatos/jsse/issues/514)) ([534d914](https://github.com/pmatos/jsse/commit/534d91455a0a3828a0d096cff67c0ae737faf198))
* **interpreter:** root destructuring targets, keys, and rest values ([#498](https://github.com/pmatos/jsse/issues/498)) ([c2efa6f](https://github.com/pmatos/jsse/commit/c2efa6f14299f6586104a9ccdf72fde4754cc29c))
* **interpreter:** root object destructuring sources ([#513](https://github.com/pmatos/jsse/issues/513)) ([de9eaef](https://github.com/pmatos/jsse/commit/de9eaefe4cccb57821dedb4a261e70eeec7d24f7))
* **interpreter:** route async continue through finally ([#509](https://github.com/pmatos/jsse/issues/509)) ([0d05a31](https://github.com/pmatos/jsse/commit/0d05a31026f4a235300d3180b772680255c09f17))
* **interpreter:** route typed-array element writes through the canonical ToNumber ([#481](https://github.com/pmatos/jsse/issues/481)) ([432b0f0](https://github.com/pmatos/jsse/commit/432b0f076d1379a1e4ab83e628bca2b712f30345))
* **modules:** honor JSON import attributes ([#487](https://github.com/pmatos/jsse/issues/487)) ([cddeaf8](https://github.com/pmatos/jsse/commit/cddeaf85d05cffc212d23397047de21bcc015010))
* **node-shim:** avoid invoking user code during inspect ([#497](https://github.com/pmatos/jsse/issues/497)) ([0e48d5c](https://github.com/pmatos/jsse/commit/0e48d5c0a6fcca65fe950aae0cb48a33409fdd61))
* **node-shim:** cap inspected array entries ([#518](https://github.com/pmatos/jsse/issues/518)) ([e73e427](https://github.com/pmatos/jsse/commit/e73e42716fc5f782d7869b985e636b262240ccad))
* **node-shim:** render array extra properties ([#521](https://github.com/pmatos/jsse/issues/521)) ([c08ed03](https://github.com/pmatos/jsse/commit/c08ed03a9f92b7585427d18836757aa70706ce20))
* **test262:** collect extra module tests ([#485](https://github.com/pmatos/jsse/issues/485)) ([618b76a](https://github.com/pmatos/jsse/commit/618b76a2203733dc42d9a65a4eafa2191911b062))


### Features

* **perf:** measure the bytecode VM's work share on mandreel ([#537](https://github.com/pmatos/jsse/issues/537)) ([285b131](https://github.com/pmatos/jsse/commit/285b131cfa47935a8fd443a1d9d49ae0ec6140d3))


### Performance Improvements

* **bytecode:** compile top-level script bodies ([#531](https://github.com/pmatos/jsse/issues/531)) ([1483b43](https://github.com/pmatos/jsse/commit/1483b43a92a484414bfedcad4688397ebe7d19b7))
* **regexp:** cache converted exec subjects ([#532](https://github.com/pmatos/jsse/issues/532)) ([24dbeda](https://github.com/pmatos/jsse/commit/24dbeda6a753fc08a16a8f8760d2df70686d9f53))
* **string:** slice substring directly in UTF-16 ([#511](https://github.com/pmatos/jsse/issues/511)) ([0170f6f](https://github.com/pmatos/jsse/commit/0170f6f0c4c6b3a2eaeedf71d6087cfdb8571d6b))

# [0.5.0](https://github.com/pmatos/jsse/compare/v0.4.18...v0.5.0) (2026-08-23)


### Bug Fixes

* **interpreter:** bound the per-body hoist-analysis cache ([#472](https://github.com/pmatos/jsse/issues/472)) ([97b35b1](https://github.com/pmatos/jsse/commit/97b35b128a55b391fdda733bb2540bcac14fd76a))
* **interpreter:** pin promise combinator captures against major GC ([#473](https://github.com/pmatos/jsse/issues/473)) ([01fd282](https://github.com/pmatos/jsse/commit/01fd282c1d9f11441150731e28a3cb69c78ff717))
* **modules:** resolve the <module source> host specifier in every import phase ([#470](https://github.com/pmatos/jsse/issues/470)) ([68c3502](https://github.com/pmatos/jsse/commit/68c35029308d28a25b9efa92d7c2e2555cc292dc)), closes [#471](https://github.com/pmatos/jsse/issues/471) [#475](https://github.com/pmatos/jsse/issues/475) [#476](https://github.com/pmatos/jsse/issues/476) [#479](https://github.com/pmatos/jsse/issues/479) [#480](https://github.com/pmatos/jsse/issues/480) [#222](https://github.com/pmatos/jsse/issues/222)


### Features

* **interpreter:** serve setTimeout/setInterval from an event-loop timer queue ([#474](https://github.com/pmatos/jsse/issues/474)) ([483a940](https://github.com/pmatos/jsse/commit/483a940c5a276301392a925ba1a48b0fb69f763b))

## [0.4.18](https://github.com/pmatos/jsse/compare/v0.4.17...v0.4.18) (2026-08-19)


### Bug Fixes

* **arraybuffer:** resolve slice bounds via shared relative-index helpers ([#453](https://github.com/pmatos/jsse/issues/453)) ([2364170](https://github.com/pmatos/jsse/commit/23641706d52283523b4a6e4ef9f4e621127024b5))
* **bigint:** unify StringToBigInt into one spec-correct operation ([#455](https://github.com/pmatos/jsse/issues/455)) ([020aa9d](https://github.com/pmatos/jsse/commit/020aa9d850441897b3a645664f27424be4309ab1))
* **interpreter:** route private accessor get/set through one PrivateGet/PrivateSet MOP ([#463](https://github.com/pmatos/jsse/issues/463)) ([5c306b9](https://github.com/pmatos/jsse/commit/5c306b9a12862f7bed788bcff72099f9f939c813))
* **interpreter:** throw on non-iterable spread in private-method calls ([#454](https://github.com/pmatos/jsse/issues/454)) ([b425f5c](https://github.com/pmatos/jsse/commit/b425f5c829093f03ded671f7c0d52c5e8ec507bd))

## [0.4.17](https://github.com/pmatos/jsse/compare/v0.4.16...v0.4.17) (2026-07-31)


### Bug Fixes

* **ast:** reject arguments/super() hidden in eval'd class field initializers ([#444](https://github.com/pmatos/jsse/issues/444)) ([5869825](https://github.com/pmatos/jsse/commit/5869825832c7814f032bd2e7eb88b37fb6d7ddf6))

## [0.4.16](https://github.com/pmatos/jsse/compare/v0.4.15...v0.4.16) (2026-07-30)


### Bug Fixes

* **interpreter:** route [[Set]] on Array length and primitive receivers through spec MOP ([#442](https://github.com/pmatos/jsse/issues/442)) ([b2a2d7c](https://github.com/pmatos/jsse/commit/b2a2d7c7b63514c578e1ca5eeba230eb4dbe8de6))

## [0.4.15](https://github.com/pmatos/jsse/compare/v0.4.14...v0.4.15) (2026-07-30)


### Performance Improvements

* **nan-box:** implement one-word JsValue representation ([#439](https://github.com/pmatos/jsse/issues/439)) ([abd3b0f](https://github.com/pmatos/jsse/commit/abd3b0ff12d8d41aef2b3333982eeab522db1db6))

## [0.4.14](https://github.com/pmatos/jsse/compare/v0.4.13...v0.4.14) (2026-07-29)


### Performance Improvements

* **bytecode:** extend call-site IC to the bytecode Call/ReturnCall opcodes ([#434](https://github.com/pmatos/jsse/issues/434)) ([7d16398](https://github.com/pmatos/jsse/commit/7d1639840e893ea8fdb78a80576834842d6c20a2))

## [0.4.13](https://github.com/pmatos/jsse/compare/v0.4.12...v0.4.13) (2026-07-26)


### Performance Improvements

* **bytecode:** compile direct identifier calls ([#399](https://github.com/pmatos/jsse/issues/399)) ([2da6b6d](https://github.com/pmatos/jsse/commit/2da6b6d02ab58ff56bb89e16122eec357df92510))

## [0.4.12](https://github.com/pmatos/jsse/compare/v0.4.11...v0.4.12) (2026-07-26)


### Performance Improvements

* **gc:** add generational nursery ([#400](https://github.com/pmatos/jsse/issues/400)) ([47c1754](https://github.com/pmatos/jsse/commit/47c175420a5b62b5dcf21a905c5d9f04f671ba85))

## [0.4.11](https://github.com/pmatos/jsse/compare/v0.4.10...v0.4.11) (2026-07-26)


### Performance Improvements

* **bytecode:** compile member/array-element access ([#397](https://github.com/pmatos/jsse/issues/397)) ([2691b54](https://github.com/pmatos/jsse/commit/2691b54e7c3912e598147ebd4eea2d645326b2b4))

## [0.4.10](https://github.com/pmatos/jsse/compare/v0.4.9...v0.4.10) (2026-07-25)


### Bug Fixes

* **interpreter:** invoke primitive optional-chain getters ([#394](https://github.com/pmatos/jsse/issues/394)) ([c29d70c](https://github.com/pmatos/jsse/commit/c29d70c269e7c0758722f197a78d1e921f6b0a57))
* **interpreter:** randomize environment binding hashing ([#393](https://github.com/pmatos/jsse/issues/393)) ([1945998](https://github.com/pmatos/jsse/commit/1945998a95de45882282fb602e00e90e90fa2757))
* **regexp:** honor nullable priority under positive quantifiers ([#395](https://github.com/pmatos/jsse/issues/395)) ([a903e16](https://github.com/pmatos/jsse/commit/a903e16e7a9aa69e009a2bece35896f6a9f4bbeb))


### Performance Improvements

* **interpreter:** collapse local binding writes to one lookup ([#396](https://github.com/pmatos/jsse/issues/396)) ([d34ae92](https://github.com/pmatos/jsse/commit/d34ae921db4203476747caf2f3b1d0729e62776f))

## [0.4.9](https://github.com/pmatos/jsse/compare/v0.4.8...v0.4.9) (2026-07-25)


### Performance Improvements

* **interpreter:** use FxHashMap for environment bindings ([#389](https://github.com/pmatos/jsse/issues/389)) ([05cddd3](https://github.com/pmatos/jsse/commit/05cddd375643d9c49fe7ebba0f4d1358b55eefb0))

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
