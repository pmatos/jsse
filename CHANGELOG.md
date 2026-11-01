# [0.13.0](https://github.com/pmatos/jsse/compare/v0.12.0...v0.13.0) (2026-11-01)


### Bug Fixes

* **ci:** make the node-host stream, events and tty tests independent of build speed ([#1517](https://github.com/pmatos/jsse/issues/1517)) ([3d0104e](https://github.com/pmatos/jsse/commit/3d0104e77c7b50ed4a41ffc65557e393ef6f64ed))
* **function:** name the class in the call-without-new TypeError ([#1538](https://github.com/pmatos/jsse/issues/1538)) ([5f7e3e9](https://github.com/pmatos/jsse/commit/5f7e3e9010599ef86c8f3d835d5ffcfd8a613db5))
* **function:** parse dynamic function parameters and body separately ([#1457](https://github.com/pmatos/jsse/issues/1457)) ([b45a30e](https://github.com/pmatos/jsse/commit/b45a30e2d1f8cc51d7f13e0b9ef8b44a1d67348a))
* **gc:** root values across user code found by the nightly GC stress sample ([#1526](https://github.com/pmatos/jsse/issues/1526)) ([333c631](https://github.com/pmatos/jsse/commit/333c631bbbfb962b9ba5358ec0d36b5ada7d56f0))
* **global:** route var, function and implicit-global names through the global object ([#1540](https://github.com/pmatos/jsse/issues/1540)) ([ece4161](https://github.com/pmatos/jsse/commit/ece41617c07f3ed3f6920fc48e93a39693a6d5f7))
* **interpreter:** a parameter default naming a later parameter hits its TDZ ([#1535](https://github.com/pmatos/jsse/issues/1535)) ([85f08da](https://github.com/pmatos/jsse/commit/85f08dab90c7c11dba058ed9675202702d26a504))
* **interpreter:** throw a ReferenceError on TDZ assignment, more V8 wording under --node ([#1549](https://github.com/pmatos/jsse/issues/1549)) ([d1943e1](https://github.com/pmatos/jsse/commit/d1943e1f903d42374fca38071d9aa69f854f2b4e))
* **node-host:** async frames in the tree-walker and after captureStackTrace ([#1509](https://github.com/pmatos/jsse/issues/1509)) ([ed375b6](https://github.com/pmatos/jsse/commit/ed375b60aee78601c82e6c16eececc2ace64c016))
* **node-host:** decode base64 strings in Buffer the way Node does ([#1499](https://github.com/pmatos/jsse/issues/1499)) ([6594613](https://github.com/pmatos/jsse/commit/6594613c16094f36d6de5b4d74d27956db2a0c4e))
* **node-host:** deliver a child's IPC disconnect before its exit ([#1437](https://github.com/pmatos/jsse/issues/1437)) ([29e4f52](https://github.com/pmatos/jsse/commit/29e4f52a43b2f12bdb11033a1ee1ec0af775e62a))
* **node-host:** make the async fs uncaught-exception test order independent ([#1356](https://github.com/pmatos/jsse/issues/1356)) ([09bf4f5](https://github.com/pmatos/jsse/commit/09bf4f564fe348b6117469f2e983edd8a73a7032))
* **node-host:** make the tty keypress and child_process EPIPE tests deterministic ([#1352](https://github.com/pmatos/jsse/issues/1352)) ([52350d3](https://github.com/pmatos/jsse/commit/52350d3b28e5eae8c4def2838a300006d15ad313))
* **node-host:** match Node's TextDecoder error codes, validation and fatal flush ([#1536](https://github.com/pmatos/jsse/issues/1536)) ([e1eabc7](https://github.com/pmatos/jsse/commit/e1eabc78c54ac9c90d4ffd2906d01e45df8058c0))
* **node-host:** rewatch a recursive-watch directory replaced with a reused inode ([#1397](https://github.com/pmatos/jsse/issues/1397)) ([7c3797d](https://github.com/pmatos/jsse/commit/7c3797d9ad692f92c8d69e577317ad5c69886f59))
* **node-host:** run Proxy ownKeys traps in deep-equality key collection ([#1498](https://github.com/pmatos/jsse/issues/1498)) ([421173e](https://github.com/pmatos/jsse/commit/421173ef5293587d25358ea36f31df8d39842d2e))
* **node-host:** scope call-position and name-inference hooks to the interpreter's thread ([#1495](https://github.com/pmatos/jsse/issues/1495)) ([e9c39e0](https://github.com/pmatos/jsse/commit/e9c39e03fbc0194599673d922862ccbbfca9461c))
* **object:** report [object Error] for AggregateError and SuppressedError ([#1463](https://github.com/pmatos/jsse/issues/1463)) ([5a47989](https://github.com/pmatos/jsse/commit/5a47989cd3c51cdcdc17811e5287e4d6b9048811))
* **object:** route spread and rest of a Proxy through [[GetOwnProperty]] ([#1421](https://github.com/pmatos/jsse/issues/1421)) ([2c2c7b1](https://github.com/pmatos/jsse/commit/2c2c7b157e3356e266541aca2de21726043bc29c))
* **parser:** accept null, true and false as module export and import names ([#1422](https://github.com/pmatos/jsse/issues/1422)) ([57c331f](https://github.com/pmatos/jsse/commit/57c331f0d337f9d1e5ec3a5fd51ed668b58d4fba))
* **parser:** allow await in async arrow body within parameter defaults ([#1489](https://github.com/pmatos/jsse/issues/1489)) ([b37dac8](https://github.com/pmatos/jsse/commit/b37dac8fd90cb2834f711a3204968ec4e76deed2))
* **parser:** block-bodied arrow followed by newline and regexp literal ([#1439](https://github.com/pmatos/jsse/issues/1439)) ([bf7a258](https://github.com/pmatos/jsse/commit/bf7a2582be7fb54e2c6182fd7f9feee07390e347))
* **parser:** reject cover-grammar, catch-param, rest-target and module var clashes ([#1552](https://github.com/pmatos/jsse/issues/1552)) ([7093f1f](https://github.com/pmatos/jsse/commit/7093f1ff8fb29c56774ef4565a214b4d2fe1c597))
* **parser:** reject function-body and try-block lexical/var redeclarations ([#1464](https://github.com/pmatos/jsse/issues/1464)) ([5b39966](https://github.com/pmatos/jsse/commit/5b399669f479843b87aa0a2197a2a6d37d311477))
* **parser:** reject missing commas in call arguments, parameters and patterns ([#1371](https://github.com/pmatos/jsse/issues/1371)) ([4b76517](https://github.com/pmatos/jsse/commit/4b765178e7150f6637df99bd8473d4318600a6ef))
* **process-fixtures:** resolve SonarCloud reliability warnings ([#1309](https://github.com/pmatos/jsse/issues/1309)) ([fa47382](https://github.com/pmatos/jsse/commit/fa473828d55e4dbd6896a18056c57227854ad83e))
* **regexp:** finish RegExp String Iterator after an abrupt next() ([#1494](https://github.com/pmatos/jsse/issues/1494)) ([d85a745](https://github.com/pmatos/jsse/commit/d85a745983a5ca25d90a5ea8a96202c9676d5057))
* **regexp:** keep RegExp String Iterator unicode flag in an internal slot ([#1456](https://github.com/pmatos/jsse/issues/1456)) ([c2a6846](https://github.com/pmatos/jsse/commit/c2a68469f449d821450a16b8a18beab8615e6229))
* **regexp:** route variable-width lookbehind captures to the right-to-left matcher ([#1441](https://github.com/pmatos/jsse/issues/1441)) ([fd5452a](https://github.com/pmatos/jsse/commit/fd5452ade72c4728d359ca4ff2ba2d03eded3d5b))
* **regexp:** throw TypeError on re-entrant RegExp String Iterator next ([#1519](https://github.com/pmatos/jsse/issues/1519)) ([6e2a042](https://github.com/pmatos/jsse/commit/6e2a04292c513035fd81345beba96857ec5d450c))


### Features

* **gc:** collect WeakRef targets and run FinalizationRegistry cleanup ([#1404](https://github.com/pmatos/jsse/issues/1404)) ([fd522ce](https://github.com/pmatos/jsse/commit/fd522cef24dc1bee6ec35faf42fc5c636cd7bff6))
* **node-host:** add --expose-internals and vendor six more events tests ([#1398](https://github.com/pmatos/jsse/issues/1398)) ([9726b12](https://github.com/pmatos/jsse/commit/9726b1236f67ad96827e739bb18f0b33585e2ad3))
* **node-host:** add async fs with callbacks, fs/promises and file streams ([#1342](https://github.com/pmatos/jsse/issues/1342)) ([4423807](https://github.com/pmatos/jsse/commit/4423807df75dfc6b77c3a2c4a694c8945908b5d0))
* **node-host:** add brotli to the zlib module ([#1416](https://github.com/pmatos/jsse/issues/1416)) ([8dd4a8f](https://github.com/pmatos/jsse/commit/8dd4a8fe454b5355386e7e0cc4296117ff52a8c2))
* **node-host:** add child_process spawn/exec family and process signals ([#1343](https://github.com/pmatos/jsse/issues/1343)) ([2c91a32](https://github.com/pmatos/jsse/commit/2c91a325e7eb49b7768f4d4866de46fb4f207094))
* **node-host:** add child_process.fork and the IPC channel ([#1363](https://github.com/pmatos/jsse/issues/1363)) ([241c660](https://github.com/pmatos/jsse/commit/241c6609862ae6d95049266e5e1af2db581df098))
* **node-host:** add combinator async frames, [as method] suffixes and timer frame names ([#1530](https://github.com/pmatos/jsse/issues/1530)) ([0837d3a](https://github.com/pmatos/jsse/commit/0837d3af7412c54dd505ab604073201ba217ccea))
* **node-host:** add fs.watch, fs.watchFile and fs.promises.watch ([#1354](https://github.com/pmatos/jsse/issues/1354)) ([ea79d88](https://github.com/pmatos/jsse/commit/ea79d88de72137310860ab329f77e3192df9bddc))
* **node-host:** add import.meta, esm/cjs interop and require(esm) ([#1350](https://github.com/pmatos/jsse/issues/1350)) ([c09f283](https://github.com/pmatos/jsse/commit/c09f2839741ecd6356ed6b12406f893408d19641))
* **node-host:** add native synchronous fs primitives and fs basics ([#1310](https://github.com/pmatos/jsse/issues/1310)) ([b974c76](https://github.com/pmatos/jsse/commit/b974c7627012594ebe8f7faf6fe18d7a43dfd955))
* **node-host:** add node event-loop semantics ([#1312](https://github.com/pmatos/jsse/issues/1312)) ([2efb3c4](https://github.com/pmatos/jsse/commit/2efb3c4bedc55c54ec8225298eafb9b8312666be))
* **node-host:** add process metrics and the perf_hooks module ([#1332](https://github.com/pmatos/jsse/issues/1332)) ([6e57e50](https://github.com/pmatos/jsse/commit/6e57e5008032117d44cc965188f31817555f0d48))
* **node-host:** add stdio streams, tty and readline ([#1340](https://github.com/pmatos/jsse/issues/1340)) ([65940fc](https://github.com/pmatos/jsse/commit/65940fc82e4b7c746a6665d32a494dbee2218450))
* **node-host:** add the assert module ([#1335](https://github.com/pmatos/jsse/issues/1335)) ([c346b13](https://github.com/pmatos/jsse/commit/c346b13ab61b28161e725da1949dfb55849c7c61))
* **node-host:** add the buffer and string_decoder modules ([#1337](https://github.com/pmatos/jsse/issues/1337)) ([ecce2ff](https://github.com/pmatos/jsse/commit/ecce2ff3a7270ffa70060f376b026bce128655ba))
* **node-host:** add the commonjs loader and unify builtins behind require ([#1319](https://github.com/pmatos/jsse/issues/1319)) ([8947fe7](https://github.com/pmatos/jsse/commit/8947fe71253b8507e69792a134ce9a2940c40490))
* **node-host:** add the crypto module with hashing, hmac and random ([#1329](https://github.com/pmatos/jsse/issues/1329)) ([0b025ab](https://github.com/pmatos/jsse/commit/0b025ab9f4096c7f8b8e884827c39b5272387e72))
* **node-host:** add the es module loader ([#1330](https://github.com/pmatos/jsse/issues/1330)) ([63c0383](https://github.com/pmatos/jsse/commit/63c038327866787fec26aee5059bfcd7c80d511e))
* **node-host:** add the events module, EventTarget and AbortSignal ([#1316](https://github.com/pmatos/jsse/issues/1316)) ([613a745](https://github.com/pmatos/jsse/commit/613a7453668b8a0e7ffc4e12d5a6bea54e9185ec))
* **node-host:** add the full util module and Node's util.inspect ([#1325](https://github.com/pmatos/jsse/issues/1325)) ([683ad39](https://github.com/pmatos/jsse/commit/683ad398ac98763712682f1f0de78ce84ef686d0))
* **node-host:** add the module builtin, createRequire and source map flags ([#1338](https://github.com/pmatos/jsse/issues/1338)) ([9453427](https://github.com/pmatos/jsse/commit/94534275839c14ce93c448ed1b0b59b6617370fc))
* **node-host:** add the os module ([#1315](https://github.com/pmatos/jsse/issues/1315)) ([139bba8](https://github.com/pmatos/jsse/commit/139bba8df9168f357c41f2b3968d8a3a08181c27))
* **node-host:** add the path module and an embedded builtin-module table ([#1311](https://github.com/pmatos/jsse/issues/1311)) ([3ea4a7c](https://github.com/pmatos/jsse/commit/3ea4a7ca41ff7565b243c059c87bf02e5e14516a))
* **node-host:** add the stream module ([#1331](https://github.com/pmatos/jsse/issues/1331)) ([17a422f](https://github.com/pmatos/jsse/commit/17a422fd12b4c70840128765cb73ec5f80015a80))
* **node-host:** add the V8-compatible Error API ([#1324](https://github.com/pmatos/jsse/issues/1324)) ([322a05b](https://github.com/pmatos/jsse/commit/322a05b0833ef3f57053d099e5dce1833ba875c5))
* **node-host:** add the vm module ([#1351](https://github.com/pmatos/jsse/issues/1351)) ([5937e00](https://github.com/pmatos/jsse/commit/5937e0024be4043cd70ddbd9d8f94a8fc07eb650))
* **node-host:** add the WebAssembly global over wasmi ([#1414](https://github.com/pmatos/jsse/issues/1414)) ([74c41a4](https://github.com/pmatos/jsse/commit/74c41a477da8b4958f9cdb8349e1296e2482bcde))
* **node-host:** add the zlib module ([#1359](https://github.com/pmatos/jsse/issues/1359)) ([88eaef1](https://github.com/pmatos/jsse/commit/88eaef1d99092db9a246a6b5091bd404d63d7fb2))
* **node-host:** add url, querystring, punycode and the WHATWG URL globals ([#1318](https://github.com/pmatos/jsse/issues/1318)) ([8ebd719](https://github.com/pmatos/jsse/commit/8ebd7190c5fbf089d17952310c4661353ff7363b))
* **node-host:** ADR and --node script args, argv, env, cwd, exitCode ([#1303](https://github.com/pmatos/jsse/issues/1303)) ([ab5fdf7](https://github.com/pmatos/jsse/commit/ab5fdf75dfa87782c19ef49242679a8cc63d1f81))
* **node-host:** builtin frames, import() call sites and V8 module error text in stacks ([#1525](https://github.com/pmatos/jsse/issues/1525)) ([c645eba](https://github.com/pmatos/jsse/commit/c645eba604c61352bd8d4f99b1d09236638e989c))
* **node-host:** complete the synchronous fs surface ([#1322](https://github.com/pmatos/jsse/issues/1322)) ([ba13567](https://github.com/pmatos/jsse/commit/ba13567decb2afc17336124595dee878098ad59c))
* **node-host:** embed the node prelude into --node ([#1307](https://github.com/pmatos/jsse/issues/1307)) ([fb9f7c3](https://github.com/pmatos/jsse/commit/fb9f7c310b03ca859ccc8b74c5eca0262ce6c9f1))
* **node-host:** frame -e programs, colour the fatal report and name arrow frames like node ([#1400](https://github.com/pmatos/jsse/issues/1400)) ([39078ba](https://github.com/pmatos/jsse/commit/39078ba6b34e6d4bee5af2fe604e92b946c421d1))
* **node-host:** fs signal support for the FileHandle forms and mid-write aborts ([#1537](https://github.com/pmatos/jsse/issues/1537)) ([5ea24d7](https://github.com/pmatos/jsse/commit/5ea24d7892cb2adc6b8eda77aca8969a81bd2be6))
* **node-host:** give generator bodies call frames and for-of heads their position ([#1551](https://github.com/pmatos/jsse/issues/1551)) ([29698ff](https://github.com/pmatos/jsse/commit/29698ff08c6322d533b92c4b9535f513310bb96b))
* **node-host:** infer member names, keep async arrow receivers and order error keys like v8 ([#1405](https://github.com/pmatos/jsse/issues/1405)) ([a979b9c](https://github.com/pmatos/jsse/commit/a979b9cac2c29a24ce8aa3dc193cc4e7a05d51ab))
* **node-host:** match V8 builtin, destructuring and iteration error messages under --node ([#1534](https://github.com/pmatos/jsse/issues/1534)) ([d6755b3](https://github.com/pmatos/jsse/commit/d6755b318fa8dbb851905a60daaa2427279d9c4e))
* **node-host:** match V8 call and member error messages and positions under --node ([#1513](https://github.com/pmatos/jsse/issues/1513)) ([f445e02](https://github.com/pmatos/jsse/commit/f445e02696f351dd164882bedc44fd958bfee401))
* **node-host:** match V8 receiver, spread, proxy and tagged template errors under --node ([#1546](https://github.com/pmatos/jsse/issues/1546)) ([6930012](https://github.com/pmatos/jsse/commit/6930012b6ed19fd3e532a2f23dfd883d648d4418))
* **node-host:** non-blocking async fs rm/readdir/realpath, pool metadata, readv/writev ([#1506](https://github.com/pmatos/jsse/issues/1506)) ([b974300](https://github.com/pmatos/jsse/commit/b9743006b7e309d0b788cea38e53c941b65a73d8))
* **node-host:** non-blocking fs.cp with an async filter ([#1520](https://github.com/pmatos/jsse/issues/1520)) ([a66638c](https://github.com/pmatos/jsse/commit/a66638c1630dd87fc742a1e87008b509d75ce79b))
* **node-host:** print at-async frames in stacks captured inside reaction jobs ([#1423](https://github.com/pmatos/jsse/issues/1423)) ([d4505c1](https://github.com/pmatos/jsse/commit/d4505c1c8150e6d9d20d5c8dd06a45d66ed02ab7))
* **node-host:** print long zone names in Date toString and join date and zone ([#1539](https://github.com/pmatos/jsse/issues/1539)) ([121e2bf](https://github.com/pmatos/jsse/commit/121e2bf6cd7a4ac55f05976e5f33ecf59227721e))
* **node-host:** print node's fatal-error report for uncaught errors ([#1357](https://github.com/pmatos/jsse/issues/1357)) ([16c8f9d](https://github.com/pmatos/jsse/commit/16c8f9dbd6161155e7c3086e006c4f1b80244c4c))
* **node-host:** report ES module entry-point failures like node ([#1399](https://github.com/pmatos/jsse/issues/1399)) ([8f5f63e](https://github.com/pmatos/jsse/commit/8f5f63ec884a5c4f7e67b16c764bb8395429f92b))
* **node-host:** report V8 positions of operator and class heritage errors under --node ([#1555](https://github.com/pmatos/jsse/issues/1555)) ([2c630d7](https://github.com/pmatos/jsse/commit/2c630d71154128b42b4cc9b521f5522712cc279d))
* **node-host:** run hereby and its dependencies on jsse ([#1370](https://github.com/pmatos/jsse/issues/1370)) ([37d631c](https://github.com/pmatos/jsse/commit/37d631c0083c531830be5f4f42732f30e052bfbb))
* **node-host:** run hereby run-eslint-rules-tests on jsse and add worker_threads ([#1417](https://github.com/pmatos/jsse/issues/1417)) ([ff7c011](https://github.com/pmatos/jsse/commit/ff7c011a16e3f53c75a7e3ffe7cb2a882b9f5fd0))
* **node-host:** transfer web streams over an in-process message channel ([#1554](https://github.com/pmatos/jsse/issues/1554)) ([14698d3](https://github.com/pmatos/jsse/commit/14698d370fd3906be2d8fc5c78278e3527d44796))
* **node-host:** web streams, stream/web, toWeb/fromWeb adapters and lazy openAsBlob ([#1531](https://github.com/pmatos/jsse/issues/1531)) ([ff52579](https://github.com/pmatos/jsse/commit/ff5257938eb3c686e38e065b5ca7e42dd02e8678))
* **node-host:** word syntax errors like v8, export-from link errors, retried loads ([#1550](https://github.com/pmatos/jsse/issues/1550)) ([f60cd74](https://github.com/pmatos/jsse/commit/f60cd74d932c8d84c6dcdda74544a788b1ccfbca))
* **ts-suite:** block the nightly on regressions and let a scripted pass-list bucket grow ([#1541](https://github.com/pmatos/jsse/issues/1541)) ([77f4802](https://github.com/pmatos/jsse/commit/77f4802ce70fc9b9685c0296ddbd39400ca9d365))


### Performance Improvements

* **bytecode:** switch dispatch table for literal cases, call positions folded under --node ([#1543](https://github.com/pmatos/jsse/issues/1543)) ([ca996aa](https://github.com/pmatos/jsse/commit/ca996aae9dabb6664e2e8e7bddf6658a9442398a))
* **gc:** grow the nursery with the environment registry and probe it read-only ([#1490](https://github.com/pmatos/jsse/issues/1490)) ([#1553](https://github.com/pmatos/jsse/issues/1553)) ([b8ee330](https://github.com/pmatos/jsse/commit/b8ee330d731a928f26a519c3b2d84f10a6c72077))
* **global:** resolve script block-scope name sites from the frame env ([#1302](https://github.com/pmatos/jsse/issues/1302)) ([3eaac4a](https://github.com/pmatos/jsse/commit/3eaac4a2e062712f941cbf36eda855fb4c78695f))
* **memory:** index spilled property maps with tag bytes instead of a hash table ([#1407](https://github.com/pmatos/jsse/issues/1407)) ([5bf9fc3](https://github.com/pmatos/jsse/commit/5bf9fc381ef007a9f7081e776a5a1cb2f85f7621))
* **memory:** shrink and share property storage, tsc self-compile peak RSS ([#1556](https://github.com/pmatos/jsse/issues/1556)) ([63a7430](https://github.com/pmatos/jsse/commit/63a7430019c1bdf540566f2b2616c68fa741cd35))
* **modules:** make module namespace export lookup O(1) ([#1500](https://github.com/pmatos/jsse/issues/1500)) ([3f9a46c](https://github.com/pmatos/jsse/commit/3f9a46cd56e4f4fe2887fc14b39fedc1b16af12c))

# [0.12.0](https://github.com/pmatos/jsse/compare/v0.11.0...v0.12.0) (2026-10-26)


### Bug Fixes

* **array:** sort/toSorted default order compares UTF-16 code units ([#938](https://github.com/pmatos/jsse/issues/938)) ([a227b65](https://github.com/pmatos/jsse/commit/a227b659a1a73ecf49cbe5179c801f467ba93af3))
* **async-gen:** coerce sloppy-mode this to the global object or wrapper ([#1114](https://github.com/pmatos/jsse/issues/1114)) ([969e784](https://github.com/pmatos/jsse/commit/969e784ddc02b0c408c733e00a55ec3472ec5c30))
* **async-gen:** make a rejected yield operand catchable in the generator ([#1113](https://github.com/pmatos/jsse/issues/1113)) ([667302e](https://github.com/pmatos/jsse/commit/667302ec9e50daf4846624945764cd920d3bbea8))
* **async:** keep function-level lexicals uninitialised on the state machine ([#1154](https://github.com/pmatos/jsse/issues/1154)) ([517f617](https://github.com/pmatos/jsse/commit/517f617388957d8d97e321eb71f084a4d060e4ae))
* **class:** initialize each class's own fields when constructing a subclass ([#949](https://github.com/pmatos/jsse/issues/949)) ([d79ca38](https://github.com/pmatos/jsse/commit/d79ca3835acc4a839295d24e1a9237b9540eaf96))
* **class:** new.target is undefined in field initializers and static blocks ([#952](https://github.com/pmatos/jsse/issues/952)) ([e582acf](https://github.com/pmatos/jsse/commit/e582acfd43316bd22ee5211498114ba6ffcc8d02))
* **class:** run every class's field initializers through one path ([#951](https://github.com/pmatos/jsse/issues/951)) ([f2f97f9](https://github.com/pmatos/jsse/commit/f2f97f9d57ea02a66797b2e548b2942dc55bb8ae))
* **collections:** accept primitive iterables in WeakMap and WeakSet constructors ([#1072](https://github.com/pmatos/jsse/issues/1072)) ([660c5a1](https://github.com/pmatos/jsse/commit/660c5a174b86d6b282cf937620a31388cc2ad4f8))
* **collections:** iterators and forEach see entries added after clear() ([#946](https://github.com/pmatos/jsse/issues/946)) ([c5c9fbd](https://github.com/pmatos/jsse/commit/c5c9fbd300ad2b7cb524b0863bda3e6937581c1f))
* **destructuring:** give accessors the primitive as receiver in patterns and GetIterator ([#1068](https://github.com/pmatos/jsse/issues/1068)) ([6df3879](https://github.com/pmatos/jsse/commit/6df387952fcc53ea98d9f6f8037cc9b4fbca5e22))
* **eval:** direct eval var conflicts with function-top-level let/const/class ([#1071](https://github.com/pmatos/jsse/issues/1071)) ([496acd9](https://github.com/pmatos/jsse/commit/496acd9f5cd5652d7c1a4bf0599265de305d0c17))
* **eval:** resolve outer class private names in direct eval in nested class ([#1205](https://github.com/pmatos/jsse/issues/1205)) ([63de220](https://github.com/pmatos/jsse/commit/63de2208c2ffee65ee53f049606a5712b6c77d19))
* **gc:** root holder and value across JSON.parse reviver calls ([#1202](https://github.com/pmatos/jsse/issues/1202)) ([ccb0fe1](https://github.com/pmatos/jsse/commit/ccb0fe1cc1cb84c3b39b91e583251f2ffafc297e)), closes [#1198](https://github.com/pmatos/jsse/issues/1198)
* **gc:** root holder and value across JSON.stringify callbacks ([#1206](https://github.com/pmatos/jsse/issues/1206)) ([828ac43](https://github.com/pmatos/jsse/commit/828ac4325aff665504262268d0d6cab22e264e4d))
* **gc:** root the for-loop head environment while its initializers run ([#1118](https://github.com/pmatos/jsse/issues/1118)) ([252429a](https://github.com/pmatos/jsse/commit/252429a7b9b02610c612d0826047d3488e690f80))
* **generator:** evaluate operands before a yield/await first in state machines ([#1106](https://github.com/pmatos/jsse/issues/1106)) ([840767b](https://github.com/pmatos/jsse/commit/840767bb87056754c4a8e9d82fa41737c1c200ab))
* **generator:** evaluation order of callees, spread and Annex B in state machines ([#1109](https://github.com/pmatos/jsse/issues/1109)) ([c2e370b](https://github.com/pmatos/jsse/commit/c2e370bbaf42b46d165d462cfaa86dccbdb0d613))
* **generator:** order class heritage and computed keys around a suspension ([#1111](https://github.com/pmatos/jsse/issues/1111)) ([b408a2e](https://github.com/pmatos/jsse/commit/b408a2edb99dda1640ff0b5167585b2bcbf92d88))
* **generator:** run class heritage checks before computed keys in state machines ([#1117](https://github.com/pmatos/jsse/issues/1117)) ([cddcc24](https://github.com/pmatos/jsse/commit/cddcc24207c7b377332f2d49ea0018daa284f627))
* **generators:** instantiate function declarations at state-machine entry ([#1044](https://github.com/pmatos/jsse/issues/1044)) ([74295ab](https://github.com/pmatos/jsse/commit/74295abb3ac463e5f0c7e20ad97b8266af094b57))
* **host:** expose console as a global object property ([#1036](https://github.com/pmatos/jsse/issues/1036)) ([1761a98](https://github.com/pmatos/jsse/commit/1761a984fd49b853de3f1b25063b433914af96b4))
* **interpreter:** close for-of iterator on strict return of a call ([#1115](https://github.com/pmatos/jsse/issues/1115)) ([41f538c](https://github.com/pmatos/jsse/commit/41f538cf501cc3e75f51cc37916f441bce34786d))
* **interpreter:** dispose using resources in catch and finally blocks ([#1156](https://github.com/pmatos/jsse/issues/1156)) ([3c4d507](https://github.com/pmatos/jsse/commit/3c4d50763e624068e740663d469553c03ecbb998))
* **interpreter:** for-in identifier target resolves via PutValue in the tree-walker ([#1160](https://github.com/pmatos/jsse/issues/1160)) ([630a7c8](https://github.com/pmatos/jsse/commit/630a7c8f419c1d0fb0d4cb477a6e383960115d81))
* **interpreter:** root computed member base across key coercion in the tree-walker ([#1087](https://github.com/pmatos/jsse/issues/1087)) ([a327c2f](https://github.com/pmatos/jsse/commit/a327c2f2ab1b3824e4424d46b9d901b804c3acab))
* **interpreter:** run a returned call before disposing using resources ([#1153](https://github.com/pmatos/jsse/issues/1153)) ([8c7f7e4](https://github.com/pmatos/jsse/commit/8c7f7e414a552f77fdb5cef0200e0cabc061cb73))
* **lexer:** separate genuine U+F0000-F0800 from lone surrogates in eval source ([#1004](https://github.com/pmatos/jsse/issues/1004)) ([#1006](https://github.com/pmatos/jsse/issues/1006)) ([7edce8c](https://github.com/pmatos/jsse/commit/7edce8c2b595df89c06a340d5165842a473ef830))
* **parser:** give the outer new its Arguments in new new F()() ([#1146](https://github.com/pmatos/jsse/issues/1146)) ([e75a926](https://github.com/pmatos/jsse/commit/e75a9262c3a9353eecd728f95716b81df3a7500e))
* **parser:** reject destructuring declarations without an initializer ([#1147](https://github.com/pmatos/jsse/issues/1147)) ([8fffeb0](https://github.com/pmatos/jsse/commit/8fffeb0a6e26d2b73a8be09f68cea46d6172514e))
* **parser:** resolve nested class private names against enclosing class ([#1201](https://github.com/pmatos/jsse/issues/1201)) ([299c48a](https://github.com/pmatos/jsse/commit/299c48aea892adfcd27cb9e611c16555442f156a))
* **proxy:** complete getOwnPropertyDescriptor trap result ([#1003](https://github.com/pmatos/jsse/issues/1003)) ([65977f2](https://github.com/pmatos/jsse/commit/65977f2cedcda8b931e2783e46b2f98fe52d71d7))
* **regexp:** keep genuine U+F0000-U+10FFFF apart from lone surrogates ([#1007](https://github.com/pmatos/jsse/issues/1007)) ([#1008](https://github.com/pmatos/jsse/issues/1008)) ([0f9e70a](https://github.com/pmatos/jsse/commit/0f9e70a21d278728a078a51a0948ae584133b3c0))
* **regexp:** reject invalid escapes and ranges in v-flag classes ([#1010](https://github.com/pmatos/jsse/issues/1010)) ([#1012](https://github.com/pmatos/jsse/issues/1012)) ([2ea6417](https://github.com/pmatos/jsse/commit/2ea6417535037ca58d78a433a72a2a0e2dfb4d77))
* **regexp:** reject unknown property names and Script values in u/v mode ([#1070](https://github.com/pmatos/jsse/issues/1070)) ([759b0f5](https://github.com/pmatos/jsse/commit/759b0f5f6b6b80377a86c1526fbe032b9de10340))
* **regexp:** treat \u{ in a non-Unicode class as an identity escape ([#1009](https://github.com/pmatos/jsse/issues/1009)) ([#1011](https://github.com/pmatos/jsse/issues/1011)) ([d9ecea1](https://github.com/pmatos/jsse/commit/d9ecea1f44087f1a2dfb25aecd120d18115e7e44))
* **set-methods:** root values across user code; fix GetSetRecord and IteratorClose ([#950](https://github.com/pmatos/jsse/issues/950)) ([2616069](https://github.com/pmatos/jsse/commit/261606973bd6ed98cb31b8ca3ea03c1ecb1cfea3))
* **string:** compare raw UTF-16 code units in String.prototype search methods ([#937](https://github.com/pmatos/jsse/issues/937)) ([6655c76](https://github.com/pmatos/jsse/commit/6655c7680d2f493e3f1a06d7e9a4ebee7126e888))
* **string:** keep lone surrogates in the remaining String builtins ([#939](https://github.com/pmatos/jsse/issues/939)) ([531e48f](https://github.com/pmatos/jsse/commit/531e48f6770cc04ae40c2c9d53f363ca482ea369))
* **strings:** keep lone surrogates through JSON, Function and match ([#1005](https://github.com/pmatos/jsse/issues/1005)) ([932c0f2](https://github.com/pmatos/jsse/commit/932c0f2cf6a01792c4f4fd2568ea7a9d112cabcf))
* **string:** throw RangeError past an engine-wide maximum string length ([#967](https://github.com/pmatos/jsse/issues/967)) ([abd46c8](https://github.com/pmatos/jsse/commit/abd46c8fc29c5dac607123676b8635a89c7e5720))
* **symbol:** keep description across property-key round trip ([#1067](https://github.com/pmatos/jsse/issues/1067)) ([b247dbd](https://github.com/pmatos/jsse/commit/b247dbd9610f4ce5a961d3b9de9551e491dd1e3f))
* **symbol:** mark well-known symbols by construction, not by description ([#1066](https://github.com/pmatos/jsse/issues/1066)) ([34c13ae](https://github.com/pmatos/jsse/commit/34c13ae53e4b2676708a89c8d1e0963068382464))
* **typedarray:** sort/toSorted no longer panic on an inconsistent comparefn ([#942](https://github.com/pmatos/jsse/issues/942)) ([6897bd9](https://github.com/pmatos/jsse/commit/6897bd9dff41e727a095efa09714552826273118))


### Features

* **fuzz:** add gc_stress fuzz target ([#1200](https://github.com/pmatos/jsse/issues/1200)) ([004a914](https://github.com/pmatos/jsse/commit/004a9148bff150f0032232e515b740bcd93e88fd))
* **fuzz:** add proxy and generator re-entrancy target ([#1196](https://github.com/pmatos/jsse/issues/1196)) ([df265c7](https://github.com/pmatos/jsse/commit/df265c7cc2e128defa13322737e236a578239d66))
* **fuzz:** arbitrary-driven AST generator target (execute_ast) ([#1197](https://github.com/pmatos/jsse/issues/1197)) ([bfcc529](https://github.com/pmatos/jsse/commit/bfcc529fd19522de64b3dd0021d76f84af3c9780))
* **fuzz:** in-process execute target with bytecode vs tree-walker comparison ([#1183](https://github.com/pmatos/jsse/issues/1183)) ([33855c4](https://github.com/pmatos/jsse/commit/33855c4b580ceb87ebe353050ef6aa9c91ec3fdc))
* **fuzz:** typed array, ArrayBuffer and Atomics target ([#1194](https://github.com/pmatos/jsse/issues/1194)) ([e5307d5](https://github.com/pmatos/jsse/commit/e5307d502a277375d398ad3d59bd89080516810d))
* **host:** add structuredClone, btoa and atob globals ([#1207](https://github.com/pmatos/jsse/issues/1207)) ([58606ad](https://github.com/pmatos/jsse/commit/58606ad14640042275c24b01897f27a1f0871338))
* **interpreter:** opt-in deterministic step budget ([#1182](https://github.com/pmatos/jsse/issues/1182)) ([5fdf291](https://github.com/pmatos/jsse/commit/5fdf2919dce2a0e8bb24270b91ee12bbbf216c1b))


### Performance Improvements

* **arguments:** cheaper arguments objects and compiled calls (-26..-44% Ir) ([#1134](https://github.com/pmatos/jsse/issues/1134)) ([5272a4b](https://github.com/pmatos/jsse/commit/5272a4b297ffc3b9238a9341382c4f0b75a7eb9d)), closes [#1025](https://github.com/pmatos/jsse/issues/1025)
* **array:** amortised O(1) Array.prototype.shift on dense arrays ([#1195](https://github.com/pmatos/jsse/issues/1195)) ([b4e7031](https://github.com/pmatos/jsse/commit/b4e7031fff7d6d7af2b27c86add39deeceff60f4)), closes [#1184](https://github.com/pmatos/jsse/issues/1184)
* **array:** build Array.of, new Array(n) and holey literals without per-index bookkeeping ([#985](https://github.com/pmatos/jsse/issues/985)) ([b252382](https://github.com/pmatos/jsse/commit/b252382f68ef3d4ed71353ed74df5ca57b9f5b2a))
* **array:** dense-array fast paths for Array.prototype builtins ([#975](https://github.com/pmatos/jsse/issues/975)) ([f4d83fe](https://github.com/pmatos/jsse/commit/f4d83fec233e164c76d515f5c7da5219275280f7))
* **async-gen:** drop per-request and per-await function objects (async-fs -10.6% Ir) ([#1083](https://github.com/pmatos/jsse/issues/1083)) ([00bb965](https://github.com/pmatos/jsse/commit/00bb965d79b6cd6ce52f2573d2661fb7b623e534))
* **async:** run async function bodies as suspending bytecode chunks (-30% doxbee-async) ([#1148](https://github.com/pmatos/jsse/issues/1148)) ([1a9f320](https://github.com/pmatos/jsse/commit/1a9f320e8dc3344eacd707ab2847a06fe7ae497c))
* **async:** run async generator bodies as suspending bytecode chunks (-30% async-fs Ir) ([#1149](https://github.com/pmatos/jsse/issues/1149)) ([687f325](https://github.com/pmatos/jsse/commit/687f325011f18657d683c2f591619e6b623f8f50))
* **async:** typed microtask jobs and await resumption without per-await function objects ([#971](https://github.com/pmatos/jsse/issues/971)) ([a477382](https://github.com/pmatos/jsse/commit/a4773823d1473769414d82e5a10d3a4a33bdff7e))
* **bigint:** add a fast path for BigInt operands in binary and relational operators ([#990](https://github.com/pmatos/jsse/issues/990)) ([5080641](https://github.com/pmatos/jsse/commit/50806410a0e7e650fa702b9b966bf00aa6653bc2))
* **bigint:** allocation-free BigInt.prototype.toString and handle-based thisBigIntValue ([#992](https://github.com/pmatos/jsse/issues/992)) ([80da759](https://github.com/pmatos/jsse/commit/80da759c2ca4074f99470e724a50d037405ba699))
* **bigint:** parse BigInt literals once in the parser ([#991](https://github.com/pmatos/jsse/issues/991)) ([e2d046b](https://github.com/pmatos/jsse/commit/e2d046bcf3efb495d76dff94626d9ddbaa33bb04))
* **build:** defer transparent huge pages until the heap has grown ([#1075](https://github.com/pmatos/jsse/issues/1075)) ([#1102](https://github.com/pmatos/jsse/issues/1102)) ([3d126b8](https://github.com/pmatos/jsse/commit/3d126b879bd9a48e3e955adf9f05a696cd598a37))
* **build:** use jemalloc as global allocator (six heavy rows -6.3% wall, splay -8.4%) ([#1074](https://github.com/pmatos/jsse/issues/1074)) ([2a86956](https://github.com/pmatos/jsse/commit/2a86956ed1f90b0ddfa3eba81fba506e43e28848))
* **bytecode:** block environments for block-scoped let/const and for(let) heads ([#980](https://github.com/pmatos/jsse/issues/980)) ([a1d98fc](https://github.com/pmatos/jsse/commit/a1d98fc77c49a2a46180429250818b7703779465))
* **bytecode:** compile plain functions containing await in nested async arrows ([#984](https://github.com/pmatos/jsse/issues/984)) ([c1acef7](https://github.com/pmatos/jsse/commit/c1acef750978f453423cc4ff9db967f57bcc7a15))
* **bytecode:** fuse effect-context assignment/update ops and inline number ops ([#963](https://github.com/pmatos/jsse/issues/963)) ([8fade38](https://github.com/pmatos/jsse/commit/8fade382669fbbec7cda755d47980c4a246e3339))
* **bytecode:** lower nested tail-call returns natively (vue -6.7%, mobx -3.6% Ir) ([#1085](https://github.com/pmatos/jsse/issues/1085)) ([ede164c](https://github.com/pmatos/jsse/commit/ede164c0455673fa0142c624e41dbdbb7e4bdacf))
* **bytecode:** native array/object/regexp literals and closure creation ([#981](https://github.com/pmatos/jsse/issues/981)) ([2cddf07](https://github.com/pmatos/jsse/commit/2cddf07d7bcdb83c56bbb3868ec1b2f3b3691fa6))
* **bytecode:** native try/catch/finally, for-in and for-of ([#982](https://github.com/pmatos/jsse/issues/982)) ([bdbf4f1](https://github.com/pmatos/jsse/commit/bdbf4f151db0d0571b689de308356194bb9ef9a4))
* **bytecode:** per-node tree-walker fallback and native switch/throw/typeof/update/let-const ([#962](https://github.com/pmatos/jsse/issues/962)) ([34201ab](https://github.com/pmatos/jsse/commit/34201ab55fd260817492349e02bbe2ae89a0f489))
* **bytecode:** per-node tree-walker fallback and valued completions in script goal ([#983](https://github.com/pmatos/jsse/issues/983)) ([ffd59cf](https://github.com/pmatos/jsse/commit/ffd59cf19ccfa326cbc57c671765083dc88ad582))
* **bytecode:** share one environment across a loop's body-block lexicals ([#1132](https://github.com/pmatos/jsse/issues/1132)) ([#1159](https://github.com/pmatos/jsse/issues/1159)) ([afe7919](https://github.com/pmatos/jsse/commit/afe7919eb2e9282129c26237787b2fd7f540db83))
* **bytecode:** share per-literal function metadata across closures ([#956](https://github.com/pmatos/jsse/issues/956)) ([4fa14fa](https://github.com/pmatos/jsse/commit/4fa14fac88fd747a13d25f88614a964d14dc192e))
* **bytecode:** slot-address function lexicals, hint free-name scopes (-30% let loop) ([#1126](https://github.com/pmatos/jsse/issues/1126)) ([ead00de](https://github.com/pmatos/jsse/commit/ead00de749a0cca9998f26b7157c745a7e17cc01))
* **call:** bind default and rest parameters via atoms, share the parameter env (-44% Ir) ([#1127](https://github.com/pmatos/jsse/issues/1127)) ([c133514](https://github.com/pmatos/jsse/commit/c1335149ff22e63905e09de3ea2c1b7a2e8a8677))
* **call:** borrow sloppy arguments record (fib -5.4%, delta-blue -3.3%, richards -1.9% Ir) ([#1090](https://github.com/pmatos/jsse/issues/1090)) ([814eba0](https://github.com/pmatos/jsse/commit/814eba00d09e53126e55ebe203eeba8f0db5f963))
* **call:** fewer layers and one-pass parameter binding (-4.5% fib Ir) ([#1133](https://github.com/pmatos/jsse/issues/1133)) ([f52b921](https://github.com/pmatos/jsse/commit/f52b9219354514587deb7a2cded73581a454523a))
* **call:** keep a function's call-site caches on its literal frame (-4.7% fib Ir) ([#1121](https://github.com/pmatos/jsse/issues/1121)) ([2098468](https://github.com/pmatos/jsse/commit/2098468296063e5baa42601a677ba71606b0b428))
* **call:** per-literal param atoms and IC handle (fib -5.1%, delta-blue -3.1% Ir) ([#1088](https://github.com/pmatos/jsse/issues/1088)) ([9ae23c6](https://github.com/pmatos/jsse/commit/9ae23c6ce32f11c920f27f95f195e36abad81ec1))
* **call:** serve ordinary compiled calls from the literal frame (-15% fib Ir) ([#1120](https://github.com/pmatos/jsse/issues/1120)) ([67dce67](https://github.com/pmatos/jsse/commit/67dce67a7c4b5267acbe937ca9a92576b42e2aea))
* **classes:** id-based private names and bytecode ops for #x accesses ([#994](https://github.com/pmatos/jsse/issues/994)) ([787d642](https://github.com/pmatos/jsse/commit/787d642d65027deac8694b9d91b0c78810a14e8e)), closes [#x](https://github.com/pmatos/jsse/issues/x)
* **classes:** serve return-this.#x getters without a call frame ([#999](https://github.com/pmatos/jsse/issues/999)) ([d7cc2e2](https://github.com/pmatos/jsse/commit/d7cc2e2fa6773631a65bd9a1985355f605723fc1)), closes [return-this.#x](https://github.com/return-this./issues/x)
* **classes:** share instance field definitions and private-name maps, cheaper private fields ([#993](https://github.com/pmatos/jsse/issues/993)) ([7b63d5f](https://github.com/pmatos/jsse/commit/7b63d5f2a86e3dde8c7493b695cb9f33e31cf093))
* **class:** share one Body per synthesized default constructor ([#947](https://github.com/pmatos/jsse/issues/947)) ([273301c](https://github.com/pmatos/jsse/commit/273301c827a5aad128e3b366562081e88b0cc55b))
* **closure:** hash-free own-name bindings, exact property storage (-17% named closure Ir) ([#1138](https://github.com/pmatos/jsse/issues/1138)) ([c07afbc](https://github.com/pmatos/jsse/commit/c07afbca99de923fdbdce02b8660e3ddeecfd5f7)), closes [#1028](https://github.com/pmatos/jsse/issues/1028)
* **collections:** hash-index Map/Set/WeakMap/WeakSet entries ([#969](https://github.com/pmatos/jsse/issues/969)) ([8c3fc53](https://github.com/pmatos/jsse/commit/8c3fc53dde05b13444c91b2c582d7fa48d6b2d60))
* **gc:** per-card dirty tracking for large old Arrays ([#989](https://github.com/pmatos/jsse/issues/989)) ([d27735e](https://github.com/pmatos/jsse/commit/d27735ece21f144ef8bb5751e58dded11c498da4))
* **gc:** per-card dirty tracking for Map/Set entry lists ([#998](https://github.com/pmatos/jsse/issues/998)) ([7a6f334](https://github.com/pmatos/jsse/commit/7a6f33477b7a568e4796021313b674505216f1e5))
* **gc:** re-scan only written environments in minor collections (-24% Three.js) ([#1137](https://github.com/pmatos/jsse/issues/1137)) ([a217156](https://github.com/pmatos/jsse/commit/a217156dd03fb75e7517d4823ee328b4241276d7))
* **gc:** stamp visited environments with a collection epoch (-42% deep-queue Ir) ([#1136](https://github.com/pmatos/jsse/issues/1136)) ([ced9e3f](https://github.com/pmatos/jsse/commit/ced9e3fe808ebd842170e1d08a6ada1104663bb9))
* **generator:** elide scope for declaration-free blocks (lazy-collections -14.9% Ir) ([#1076](https://github.com/pmatos/jsse/issues/1076)) ([243d142](https://github.com/pmatos/jsse/commit/243d142ba98d5b7927c2685cc6f4d1a396d341b2))
* **generator:** run sync generator bodies as suspending bytecode chunks ([#1103](https://github.com/pmatos/jsse/issues/1103)) ([9dc51be](https://github.com/pmatos/jsse/commit/9dc51bec0607c16fd28cddded515ceac3e588278))
* **generators:** borrow state terminators and trim per-step driver work ([#968](https://github.com/pmatos/jsse/issues/968)) ([b28c11b](https://github.com/pmatos/jsse/commit/b28c11b18b646097f31ece5412dcd24fb424ccfb))
* **interpreter:** cheap function-object creation from pre-interned keys ([#957](https://github.com/pmatos/jsse/issues/957)) ([ea5c615](https://github.com/pmatos/jsse/commit/ea5c61530d586b4d6c3351b7573039c840b2bb15))
* **interpreter:** declare function-top-level lexicals from atoms (bigdenary -8.0% Ir) ([#1077](https://github.com/pmatos/jsse/issues/1077)) ([dfb74b0](https://github.com/pmatos/jsse/commit/dfb74b0af163bf8d07afc9a63c1f2daace9516b1))
* **interpreter:** fast path for global-object data property reads and writes ([#964](https://github.com/pmatos/jsse/issues/964)) ([4f8144a](https://github.com/pmatos/jsse/commit/4f8144a8174b7991eb6c0eaf46e752e5797e17ed))
* **interpreter:** remove fixed per-call work and make arguments objects cheap ([#953](https://github.com/pmatos/jsse/issues/953)) ([b7fd447](https://github.com/pmatos/jsse/commit/b7fd447016cf621de495c9c9b7362e6cd3c5b830))
* **interpreter:** resolve function-local names via atoms and verified slots ([#954](https://github.com/pmatos/jsse/issues/954)) ([9703e35](https://github.com/pmatos/jsse/commit/9703e359a45e723434565a6d549bcb6e2c903ace))
* **interpreter:** skip key stringification for numeric element stores ([#1093](https://github.com/pmatos/jsse/issues/1093)) ([5784ea9](https://github.com/pmatos/jsse/commit/5784ea97037275699887bfe48363c9e58848b3ee))
* **interpreter:** skip the per-iteration env copy for non-capturing loops ([#979](https://github.com/pmatos/jsse/issues/979)) ([45a0ec8](https://github.com/pmatos/jsse/commit/45a0ec8a385ca7073a6dd4319c3b367430725e2e))
* **interpreter:** take the compiled-call path on a closure's first call (-11%) ([#1161](https://github.com/pmatos/jsse/issues/1161)) ([ce8cd33](https://github.com/pmatos/jsse/commit/ce8cd3307be5378155c7ce0ef720d3948369959b))
* **intl:** cache ICU DecimalFormatter and DateTimeFormatter across format calls ([#1191](https://github.com/pmatos/jsse/issues/1191)) ([bbe295c](https://github.com/pmatos/jsse/commit/bbe295caa13f4a69d47bee80a878c3bec1b9f89a))
* **intl:** cache ICU plural rules on the PluralRules instance ([#1140](https://github.com/pmatos/jsse/issues/1140)) ([f35eda7](https://github.com/pmatos/jsse/commit/f35eda7d3b3080a4c70091a2d829411e19ac0157))
* **iterators:** allocation-free iterator steps for intrinsic iterators ([#978](https://github.com/pmatos/jsse/issues/978)) ([bbdecb0](https://github.com/pmatos/jsse/commit/bbdecb0b544ae22efe46b791c2c17b8581340d0c))
* **object:** ordinary-object fast paths for keys/values/entries/assign, spread and for-in ([#974](https://github.com/pmatos/jsse/issues/974)) ([956db7d](https://github.com/pmatos/jsse/commit/956db7d27c45bc49bbbf0199fcf23ed4849d2c2b))
* **object:** property order in PropertyMap, keyed fast hasher, interned member keys ([#986](https://github.com/pmatos/jsse/issues/986)) ([83924f9](https://github.com/pmatos/jsse/commit/83924f99e7c364a51c71cebb8195b8fe8ddc9f92))
* **object:** shrink JsObjectData to 312 B and fold the major collection passes ([#987](https://github.com/pmatos/jsse/issues/987)) ([0d41f4f](https://github.com/pmatos/jsse/commit/0d41f4f01cce9264ed9879e1ce1a91aa07286148))
* **promise:** intrinsic-promise reactions without per-then resolve/reject function objects ([#970](https://github.com/pmatos/jsse/issues/970)) ([c0391bd](https://github.com/pmatos/jsse/commit/c0391bd8c65ce0abd22fc6e461b28d03ae9acfa6))
* **property:** dense Array element get/set fast paths ([#977](https://github.com/pmatos/jsse/issues/977)) ([de01a66](https://github.com/pmatos/jsse/commit/de01a66a4e1f80191b39d4bd17be3b55769797be))
* **property:** dense hole reads and single-scan own-property misses ([#1096](https://github.com/pmatos/jsse/issues/1096)) ([9506005](https://github.com/pmatos/jsse/commit/95060058a65ae8709d625bea116c7b326246a26d))
* **property:** in-place named get, store, method load and update (-17% richards Ir) ([#1123](https://github.com/pmatos/jsse/issues/1123)) ([73db045](https://github.com/pmatos/jsse/commit/73db045b618f5f6fc736125479677672a63542f7))
* **property:** per-site slot hints for named property access (-4% Box2D Ir) ([#1124](https://github.com/pmatos/jsse/issues/1124)) ([80b7702](https://github.com/pmatos/jsse/commit/80b770295e8a12932ebf510345e9ff33b588f76a))
* **proxy:** native descriptor path for [[Set]] with a Proxy receiver ([#996](https://github.com/pmatos/jsse/issues/996)) ([f9504bb](https://github.com/pmatos/jsse/commit/f9504bbef8413a36aa52ff5f7b07efeaf0113759))
* **proxy:** reuse property-key strings across Proxy traps and Reflect ([#1097](https://github.com/pmatos/jsse/issues/1097)) ([801f585](https://github.com/pmatos/jsse/commit/801f5854bdb23e4180e65d82bafdcaca1f42e779))
* quick wins from the boa 0.22 comparison ([#913](https://github.com/pmatos/jsse/issues/913) [#904](https://github.com/pmatos/jsse/issues/904) [#923](https://github.com/pmatos/jsse/issues/923) [#920](https://github.com/pmatos/jsse/issues/920) [#907](https://github.com/pmatos/jsse/issues/907) [#900](https://github.com/pmatos/jsse/issues/900) [#914](https://github.com/pmatos/jsse/issues/914) [#908](https://github.com/pmatos/jsse/issues/908)) ([#943](https://github.com/pmatos/jsse/issues/943)) ([f1b4d32](https://github.com/pmatos/jsse/commit/f1b4d32a2fc3f5e6f3ed569ac53ababa5c34b6bf))
* **regexp:** anchored sticky exec, slot-cached matcher and multi-entry subject cache ([#972](https://github.com/pmatos/jsse/issues/972)) ([7dc4e5e](https://github.com/pmatos/jsse/commit/7dc4e5e5b02e31d168439a779451af49629c7ea1))
* **runtime:** inline short strings, name-pool index, dense spread, exit teardown ([#1099](https://github.com/pmatos/jsse/issues/1099)) ([e22d6a9](https://github.com/pmatos/jsse/commit/e22d6a91ebfc4490fdcfae8b24ea303a5287c1f3))
* **runtime:** return cached typeof result strings instead of allocating ([#1095](https://github.com/pmatos/jsse/issues/1095)) ([02d9293](https://github.com/pmatos/jsse/commit/02d92938fcee77cd9f3dfc313434c76748bfcb19))
* **sort:** detect natural runs in comparefn merge sort ([#1193](https://github.com/pmatos/jsse/issues/1193)) ([baac12c](https://github.com/pmatos/jsse/commit/baac12c365245bfcf0519121db9152e23f4ea8ba))
* **strings:** amortised O(1) string concatenation via extensible buffers ([#945](https://github.com/pmatos/jsse/issues/945)) ([80aa515](https://github.com/pmatos/jsse/commit/80aa515e184bdb8f37cb255e42dfa072c83bb9a7))
* **strings:** fast paths for Array.prototype.join and first-unit string search ([#973](https://github.com/pmatos/jsse/issues/973)) ([1cc3e97](https://github.com/pmatos/jsse/commit/1cc3e979b51f5d700dbe03879f1265aacafc3702))
* **symbol:** flag Symbol.for symbols instead of scanning the registry in can_be_held_weakly ([#1141](https://github.com/pmatos/jsse/issues/1141)) ([7461a9a](https://github.com/pmatos/jsse/commit/7461a9a6f76862de19c398cecab06b093de54c0a))
* **symbol:** memoize symbol property keys, recover the original symbol ([#1091](https://github.com/pmatos/jsse/issues/1091)) ([1abeab0](https://github.com/pmatos/jsse/commit/1abeab0751e767cd2ebf1f137d4df27b5128e503))
* **types:** one-compare heap-ownership test in JsValue clone/drop (-6.9% Ir) ([#1119](https://github.com/pmatos/jsse/issues/1119)) ([01ace55](https://github.com/pmatos/jsse/commit/01ace55ba9ec1aa856dd7377f5723dada10f51c6))
* **vm:** answer BigInt-pair operators without the generic dispatch layers (-8% ops loop, -2.5% noble Ir) ([#1128](https://github.com/pmatos/jsse/issues/1128)) ([fe56817](https://github.com/pmatos/jsse/commit/fe568175a60aa727e2aada21e9f1aaf512842f37)), closes [#1026](https://github.com/pmatos/jsse/issues/1026)
* **vm:** answer loose equality with null and same-type operands directly (-4% richards) ([#1125](https://github.com/pmatos/jsse/issues/1125)) ([e825563](https://github.com/pmatos/jsse/commit/e825563369e33a5bdb8952d6814c7f4ba7cccfad))
* **vm:** drop operand re-rooting, inline GC safepoint poll (richards -5.1%, Box2D -6.5% Ir) ([#1080](https://github.com/pmatos/jsse/issues/1080)) ([029b06a](https://github.com/pmatos/jsse/commit/029b06a707c797f131bdd8cb9c515e629b982163))
* **vm:** fuse local-pair binary ops, moves and compare-jumps (-9% opmix Ir) ([#1130](https://github.com/pmatos/jsse/issues/1130)) ([731c374](https://github.com/pmatos/jsse/commit/731c37498a066c094df7570fd43565d0a1be7c4f))
* **vm:** inline non-Number ===/!== and fuse switch case compares ([#1094](https://github.com/pmatos/jsse/issues/1094)) ([0b264d5](https://github.com/pmatos/jsse/commit/0b264d5ab3e9bfd9596ebe8c0d0c3fdc87aeed7a))
* **vm:** native CallSpread/ConstructSpread ops for spread call arguments (-10..-18% Ir) ([#1143](https://github.com/pmatos/jsse/issues/1143)) ([f2f57af](https://github.com/pmatos/jsse/commit/f2f57af08114395c152f564c553de88125395caa))
* **vm:** native plain-name let/const object binding patterns (-27% wall on 2-4 names) ([#1144](https://github.com/pmatos/jsse/issues/1144)) ([57cfcae](https://github.com/pmatos/jsse/commit/57cfcaebcb4a088b06ee4c2df3fde7538d484448))
* **vm:** vet frame once, decode operands in one check, move plain locals (-20% Ir) ([#1129](https://github.com/pmatos/jsse/issues/1129)) ([0474e48](https://github.com/pmatos/jsse/commit/0474e481dbf789494011e8c8338f80ad74df5d88)), closes [#1030](https://github.com/pmatos/jsse/issues/1030)

# [0.11.0](https://github.com/pmatos/jsse/compare/v0.10.0...v0.11.0) (2026-10-09)


### Bug Fixes

* **atomics:** validate index against current length of length-tracking views ([#884](https://github.com/pmatos/jsse/issues/884)) ([7428647](https://github.com/pmatos/jsse/commit/7428647c3f69c82c7fa2d3708b909316c9d84374)), closes [#882](https://github.com/pmatos/jsse/issues/882)
* **disposable:** relax await-using gate for using-headed for-of loops ([#856](https://github.com/pmatos/jsse/issues/856)) ([948dff8](https://github.com/pmatos/jsse/commit/948dff868144c44fcf9cb6601fa9296906622833))
* **disposable:** suspend await-using disposal in a with-wrapped block ([#860](https://github.com/pmatos/jsse/issues/860)) ([8a63354](https://github.com/pmatos/jsse/commit/8a63354dddafb1ccd757a842fd52f61379946247))
* **disposable:** suspend await-using disposal in for(let)/for-in/try bodies ([#846](https://github.com/pmatos/jsse/issues/846)) ([e2d53df](https://github.com/pmatos/jsse/commit/e2d53df252271c48b9c166a1ceb54f083247c354))
* **gc:** root Object.fromEntries's result, iterator, and entry locals ([#851](https://github.com/pmatos/jsse/issues/851)) ([8082010](https://github.com/pmatos/jsse/commit/808201043d63edcb06838787f8d1a05c5bae6789))
* **gc:** root Object.groupBy's iterator and result under GC stress ([#837](https://github.com/pmatos/jsse/issues/837)) ([264fe0f](https://github.com/pmatos/jsse/commit/264fe0f54e734f9869ec332f24364c4c063b2d5f))
* **gc:** root rest-destructuring objects across CopyDataProperties ([#852](https://github.com/pmatos/jsse/issues/852)) ([51c4181](https://github.com/pmatos/jsse/commit/51c4181c8038e58cc339a22900b4b77a26c7c19e))
* **gc:** stop yield* retrying sync GetIterator after async iterator error ([#850](https://github.com/pmatos/jsse/issues/850)) ([8af27c7](https://github.com/pmatos/jsse/commit/8af27c7bd72d2b07e782dbce7922925624fc1af1))
* **generator:** dispose using-headed C-style for at loop exit ([#868](https://github.com/pmatos/jsse/issues/868)) ([a0ceb89](https://github.com/pmatos/jsse/commit/a0ceb893061e1ac2df293530e897bb5deda6991d))
* **generator:** give switch CaseBlock one shared per-entry lexical scope ([#853](https://github.com/pmatos/jsse/issues/853)) ([9ab52df](https://github.com/pmatos/jsse/commit/9ab52df1d9f8a752026e24eb2ac07d76a104c411))
* **generator:** suspend Blocked await-using disposal, not just Isolatable ([#859](https://github.com/pmatos/jsse/issues/859)) ([9347418](https://github.com/pmatos/jsse/commit/9347418accbdee82ee839b1a4ea89ffa2df5e368))
* **generator:** suspend with-wrapped await-using disposal in async generators ([#869](https://github.com/pmatos/jsse/issues/869)) ([931f637](https://github.com/pmatos/jsse/commit/931f6377d10a8f28f733075169d7731446743b93))
* **interpreter:** hoist Annex B functions declared directly in try/catch/finally blocks ([#865](https://github.com/pmatos/jsse/issues/865)) ([51d1950](https://github.com/pmatos/jsse/commit/51d19505effafd7d36f7d7789ab4200e939f03fb))
* **interpreter:** hoist Annex B functions declared in try/catch/finally bodies ([#866](https://github.com/pmatos/jsse/issues/866)) ([19845aa](https://github.com/pmatos/jsse/commit/19845aa50cbdbd52f583669784002ebf8e6fc36c))
* **interpreter:** restore Annex B hoisting in lowered function bodies ([#854](https://github.com/pmatos/jsse/issues/854)) ([2e1b796](https://github.com/pmatos/jsse/commit/2e1b796ad19d2a4505d5475a1ad4ec1d10bf6408))


### Features

* add pm-plan planning stage and claim-based workflow gates ([#879](https://github.com/pmatos/jsse/issues/879)) ([50b6d8d](https://github.com/pmatos/jsse/commit/50b6d8d761731241bd466dd863c6a41f11e90cec))
* **bytecode:** compile labeled while/for loops with break/continue ([#876](https://github.com/pmatos/jsse/issues/876)) ([7f8d4e2](https://github.com/pmatos/jsse/commit/7f8d4e2f7afe54b09b1df62ea07cc3879a646599))
* **bytecode:** compile member-expression call callees ([#864](https://github.com/pmatos/jsse/issues/864)) ([3d77e54](https://github.com/pmatos/jsse/commit/3d77e541a9fd238b3e0a1eebb2844943c5320131))
* **cli:** enable the bytecode VM by default (--no-bytecode opts out) ([#875](https://github.com/pmatos/jsse/issues/875)) ([37339f3](https://github.com/pmatos/jsse/commit/37339f311f0f0b2d15ed03975819b5969bf961f3))


### Performance Improvements

* **bytecode:** bring Mandreel under 120 seconds ([#878](https://github.com/pmatos/jsse/issues/878)) ([6408e77](https://github.com/pmatos/jsse/commit/6408e77e764930fe4a2c396d1ab2fcc28f03f035))

# [0.10.0](https://github.com/pmatos/jsse/compare/v0.9.1...v0.10.0) (2026-10-04)


### Bug Fixes

* **gc:** root @[@split](https://github.com/split)'s species-constructed splitter ([#833](https://github.com/pmatos/jsse/issues/833)) ([18704be](https://github.com/pmatos/jsse/commit/18704bed7fc0a9e85c356e33aa0c57785c752ef1))
* **gc:** root async-dispose's promise capability and intermediates ([#835](https://github.com/pmatos/jsse/issues/835)) ([44b78be](https://github.com/pmatos/jsse/commit/44b78bef903af4308ea68cbf464ff8a448b48927))
* **gc:** root AsyncFromSyncIterator wrapper and dispose-fallback captures ([#812](https://github.com/pmatos/jsse/issues/812)) ([7246d7f](https://github.com/pmatos/jsse/commit/7246d7f66e0daa046ce50d437cb01e57ae4affec))
* **gc:** root for-of running value across same-label continue ([#834](https://github.com/pmatos/jsse/issues/834)) ([a0998d6](https://github.com/pmatos/jsse/commit/a0998d6e434ef6c1e4d04d92514f9f66f42fe15f))
* **gc:** root in-flight Throw/Return completion payloads across finally/IteratorClose ([#815](https://github.com/pmatos/jsse/issues/815)) ([eacee8d](https://github.com/pmatos/jsse/commit/eacee8d62858dfac132c96173cde3b9a8a5f2fa6))
* **gc:** root Iterator.prototype.toArray's thrown value and accumulated values ([#836](https://github.com/pmatos/jsse/issues/836)) ([e937ef0](https://github.com/pmatos/jsse/commit/e937ef0dc851237b327f9a718325cfbbb0888e86))
* **gc:** root Map/Set/WeakMap/WeakSet constructors' under-construction object ([#832](https://github.com/pmatos/jsse/issues/832)) ([35a490e](https://github.com/pmatos/jsse/commit/35a490e33844f851ad51725fc76fcb099b837d81))
* **gc:** root RegExp String Iterator matcher through a typed field ([#817](https://github.com/pmatos/jsse/issues/817)) ([b2140c1](https://github.com/pmatos/jsse/commit/b2140c117bbd00adb5591e01956abc0ccb62c6bc))
* **gc:** root resolve fn and iterator in Promise.all/allSettled/any/race ([#816](https://github.com/pmatos/jsse/issues/816)) ([3c190cb](https://github.com/pmatos/jsse/commit/3c190cbb58adcf48224536763d514b82e0034e2b))
* **gc:** trace DisposableStack resources and dispose-loop state ([#818](https://github.com/pmatos/jsse/issues/818)) ([06bb5c9](https://github.com/pmatos/jsse/commit/06bb5c9f95ddc1eef584e3b25030ef0859c37b59))
* **generators:** deliver yield* abrupt exits into the body, fix return Await order ([#781](https://github.com/pmatos/jsse/issues/781)) ([af8f570](https://github.com/pmatos/jsse/commit/af8f5703ff816fe0a103890513b7f8045ec45413))
* **generators:** give a suspending own-initializer let/const a real block binding ([#790](https://github.com/pmatos/jsse/issues/790)) ([396fd62](https://github.com/pmatos/jsse/commit/396fd6299eb475b7d4d785d96010e759594b018c))
* **generators:** keep block let/const TDZ intact across suspension points ([#777](https://github.com/pmatos/jsse/issues/777)) ([e3cadc9](https://github.com/pmatos/jsse/commit/e3cadc91876646e313897763ac61ad1492fc15f3))
* **generators:** lower await in array-pattern defaults at catch/for-head/for-init sites ([#789](https://github.com/pmatos/jsse/issues/789)) ([c666612](https://github.com/pmatos/jsse/commit/c666612018dd86ea8f7dad77d376bbc87d14b8d3))
* **generators:** lower await in array/for-head assignment patterns ([#801](https://github.com/pmatos/jsse/issues/801)) ([b647da2](https://github.com/pmatos/jsse/commit/b647da2d20e60281ca14f2c9b233fcf9f8d60898))
* **generators:** lower await in catch-param and for-in/of-head destructuring defaults ([#773](https://github.com/pmatos/jsse/issues/773)) ([154db0f](https://github.com/pmatos/jsse/commit/154db0f96a9d28a3d62c28338d9f0a4aed674660))
* **generators:** lower await/yield in array-pattern defaults to states ([#772](https://github.com/pmatos/jsse/issues/772)) ([d1b1213](https://github.com/pmatos/jsse/commit/d1b12137a79cf5714497f654d4328b89da4afc6e))
* **generators:** lower object rest beside a suspending sibling ([#783](https://github.com/pmatos/jsse/issues/783)) ([e19e677](https://github.com/pmatos/jsse/commit/e19e6775a5987fef13188c471c8cb79b4b5508f1))
* **generators:** preserve iteration across head-pattern yield ([#760](https://github.com/pmatos/jsse/issues/760)) ([1227c8c](https://github.com/pmatos/jsse/commit/1227c8c845ad6bdc79e0fc62b73d764f2121326d))
* **generators:** run AsyncIteratorClose before yield* no-throw-method TypeError ([#785](https://github.com/pmatos/jsse/issues/785)) ([7c60330](https://github.com/pmatos/jsse/commit/7c60330b48cce19c3d4de1695faad391daa2e13c))
* **generators:** scope pending throw/return/loop-control to owning try context ([#778](https://github.com/pmatos/jsse/issues/778)) ([af01cdb](https://github.com/pmatos/jsse/commit/af01cdbe7aac9cd32c2351b603df6e2d52ccbd0b))
* **generators:** suspend async functions' for-of unwind disposal at its own Await ([#784](https://github.com/pmatos/jsse/issues/784)) ([a815480](https://github.com/pmatos/jsse/commit/a815480e19d51fff22b06821320eb69783ffa984))
* **generators:** suspend C-style for (await using ...) head disposal in async functions ([#787](https://github.com/pmatos/jsse/issues/787)) ([c4bb651](https://github.com/pmatos/jsse/commit/c4bb65119d6983b6b7b390d0e80e24722a5eed85))
* **generators:** suspend yield in for-await assignment-head patterns ([#775](https://github.com/pmatos/jsse/issues/775)) ([67e6515](https://github.com/pmatos/jsse/commit/67e6515512e3b8da55a3d4cbd0a46754539f1414))


### Features

* **bytecode:** add statement-boundary GC safepoints to the VM ([#830](https://github.com/pmatos/jsse/issues/830)) ([b7e63ac](https://github.com/pmatos/jsse/commit/b7e63ac272a863ba6c5b1dda966c14a79537d681))
* **gc:** add JSSE_GC_STRESS safepoint stress mode ([#799](https://github.com/pmatos/jsse/issues/799)) ([5e01209](https://github.com/pmatos/jsse/commit/5e012099f092de94a55e1a591301a33ca4c99b6c))

## [0.9.1](https://github.com/pmatos/jsse/compare/v0.9.0...v0.9.1) (2026-09-27)


### Bug Fixes

* **ci:** align CodeQL analyze action with init and autobuild ([#759](https://github.com/pmatos/jsse/issues/759)) ([58a25c5](https://github.com/pmatos/jsse/commit/58a25c5aebc772fe60e641543e86aa145a37088b))


### Performance Improvements

* **array:** make result property creation linear ([#758](https://github.com/pmatos/jsse/issues/758)) ([09bc891](https://github.com/pmatos/jsse/commit/09bc891521be78cbbe2f2738e0fdfe67ed794351))

# [0.9.0](https://github.com/pmatos/jsse/compare/v0.8.2...v0.9.0) (2026-09-23)


### Bug Fixes

* **ast:** drop Expression's boxed links iteratively via ExprBox ([#697](https://github.com/pmatos/jsse/issues/697)) ([054860f](https://github.com/pmatos/jsse/commit/054860f03ddecb742f79a2949d0f939716b06a8f))
* **date:** accept bare one/two-digit month strings in Date parsing ([#705](https://github.com/pmatos/jsse/issues/705)) ([3e99c79](https://github.com/pmatos/jsse/commit/3e99c79801d2addc80c4a08d55edabc837fb74ea))
* **date:** accept negative-year tokens in legacy written-month parser ([#718](https://github.com/pmatos/jsse/issues/718)) ([eba6e18](https://github.com/pmatos/jsse/commit/eba6e183c2b0c02d0db9c12fb099fbeef1aabf6f))
* **date:** parse legacy date strings in Date.parse and add console.assert ([#675](https://github.com/pmatos/jsse/issues/675)) ([d3c785f](https://github.com/pmatos/jsse/commit/d3c785f15690d1dc0eb7d05e72cc4bb6370e9d1e))
* **date:** reject out-of-range ISO time components in Date.parse ([#693](https://github.com/pmatos/jsse/issues/693)) ([497b0a0](https://github.com/pmatos/jsse/commit/497b0a0ce1414ceb6ad95c63ff21dc92d3ceb471))
* **delete:** apply exotic [[Delete]] semantics to optional-chain delete ([#661](https://github.com/pmatos/jsse/issues/661)) ([7e698cd](https://github.com/pmatos/jsse/commit/7e698cda14853f88e3d67f8dd772878b98db306f))
* **disposable:** suspend at DisposeResources awaits instead of draining inline ([#666](https://github.com/pmatos/jsse/issues/666)) ([181dbe9](https://github.com/pmatos/jsse/commit/181dbe9f0fabb93c52dda73c8aeb8e9c96b27dd3))
* **disposable:** suspend for-of head disposal, fix await-using-of parse bug ([#699](https://github.com/pmatos/jsse/issues/699)) ([ac2bbd6](https://github.com/pmatos/jsse/commit/ac2bbd65d4b7b2dbfca990253832c3ae491d75be))
* **disposable:** suspend nested await using blocks at disposal in try/loop/switch bodies ([#688](https://github.com/pmatos/jsse/issues/688)) ([da44e1e](https://github.com/pmatos/jsse/commit/da44e1e6dcd40756e89810dc49d0aaa97d6ea33b))
* **gc:** root pending async-generator requests so next() promises survive collection ([#706](https://github.com/pmatos/jsse/issues/706)) ([e8910e8](https://github.com/pmatos/jsse/commit/e8910e8b53d1a8eed2aa8481cbb77672025f195b))
* **generators:** await return operands and park async-generator disposal unwinds ([#734](https://github.com/pmatos/jsse/issues/734)) ([40cb764](https://github.com/pmatos/jsse/commit/40cb764229cb49d3b03dde0191e990cfb5c55b23))
* **generators:** bind yield/yield* pattern results with the decl's kind ([#739](https://github.com/pmatos/jsse/issues/739)) ([e3caf29](https://github.com/pmatos/jsse/commit/e3caf2936665e9965819766f5a7cabf571b4b5ea))
* **generators:** decompose class heritage/computed keys to close InlineYield gap ([#658](https://github.com/pmatos/jsse/issues/658)) ([98ee017](https://github.com/pmatos/jsse/commit/98ee0170486cba70959f1e54a2051255858e4bac))
* **generators:** don't drain microtasks on synchronous async settle paths ([#714](https://github.com/pmatos/jsse/issues/714)) ([f84dc6f](https://github.com/pmatos/jsse/commit/f84dc6fc40d4c12e298a66fe441cf9da708c60f9))
* **generators:** give await-using blocks real scope states in async functions ([#701](https://github.com/pmatos/jsse/issues/701)) ([557f67d](https://github.com/pmatos/jsse/commit/557f67daf3dcc6c6a2d3ffc20642647ce873574f))
* **generators:** give lowered blocks/loops/catch their own scope per entry ([#703](https://github.com/pmatos/jsse/issues/703)) ([a129a72](https://github.com/pmatos/jsse/commit/a129a725d4652c0b6858853c0451b0ed96e97b86))
* **generators:** honor break/continue that escape yield-free try/with ([#690](https://github.com/pmatos/jsse/issues/690)) ([30b1535](https://github.com/pmatos/jsse/commit/30b1535d04e31d083de1d816e55a2abedd351f19))
* **generators:** lower await in object-pattern defaults to suspension states ([#728](https://github.com/pmatos/jsse/issues/728)) ([d718e14](https://github.com/pmatos/jsse/commit/d718e14ef33c161684009e343ba2df853cb47494))
* **generators:** lower await/yield in delete, update, assignment and logical operands ([#731](https://github.com/pmatos/jsse/issues/731)) ([c9dad85](https://github.com/pmatos/jsse/commit/c9dad8526c5ac7e88e59f0c972fdc957f579e43d))
* **generators:** lower yield-free break/continue in switch cases of state-machine bodies ([#672](https://github.com/pmatos/jsse/issues/672)) ([0cbc10a](https://github.com/pmatos/jsse/commit/0cbc10a4e8a7d644a8f6568437bc478d4b07656e))
* **generators:** park async-generator requests at AwaitReturn, drop inline drains ([#732](https://github.com/pmatos/jsse/issues/732)) ([9039412](https://github.com/pmatos/jsse/commit/903941257a9fae5cee084c96d043008f087bb53b))
* **generators:** recurse into containers when detecting for-await suspension ([#737](https://github.com/pmatos/jsse/issues/737)) ([96a8325](https://github.com/pmatos/jsse/commit/96a83253dcd030af3f10c2006a3ecc8d95bbc05b))
* **generators:** route condition and async return-expression throws through enclosing try ([#682](https://github.com/pmatos/jsse/issues/682)) ([5e8ee75](https://github.com/pmatos/jsse/commit/5e8ee75b7ab1306b379498df05aeda0ce2986bf0))
* **generators:** route switch discriminant/case-test throws through enclosing try ([#664](https://github.com/pmatos/jsse/issues/664)) ([a86201e](https://github.com/pmatos/jsse/commit/a86201e93565ddfb2a52d8f106f843d1d539d91e))
* **generators:** run finalizers for break/continue leaving a yielding try ([#717](https://github.com/pmatos/jsse/issues/717)) ([cb19f1a](https://github.com/pmatos/jsse/commit/cb19f1a803c017fada4538feb3d4f839eabffd2a))
* **generators:** run for-in loops that contain yield or await ([#691](https://github.com/pmatos/jsse/issues/691)) ([3361b81](https://github.com/pmatos/jsse/commit/3361b81578d1a4b4fac990403c96bf2a1f135d04))
* **generators:** scope pending iterator-close list to each generator activation ([#657](https://github.com/pmatos/jsse/issues/657)) ([d79e1f3](https://github.com/pmatos/jsse/commit/d79e1f38726ebbcc5bcf4135026cfacd986d1a6f))
* **generators:** scope pending throw/return to owning try context ([#740](https://github.com/pmatos/jsse/issues/740)) ([4b9789f](https://github.com/pmatos/jsse/commit/4b9789f8e025a43e322bfd84e9382b6534744b3a))
* **generators:** stop hanging on await in destructuring-assignment patterns ([#741](https://github.com/pmatos/jsse/issues/741)) ([4c2b2d9](https://github.com/pmatos/jsse/commit/4c2b2d942539d7ec43b64aa564527982e9776558))
* **generators:** suspend async-generator for-of unwind disposal on throw ([#743](https://github.com/pmatos/jsse/issues/743)) ([cd9dd60](https://github.com/pmatos/jsse/commit/cd9dd60bf17b74fdeb4f192e0b14185f371a1dee))
* **generators:** suspend await using disposal in async generators ([#715](https://github.com/pmatos/jsse/issues/715)) ([223be07](https://github.com/pmatos/jsse/commit/223be0781112bed5637b0894c5bbdb783b0c6086))
* **generators:** suspend every yield* step in async generators at its Await ([#723](https://github.com/pmatos/jsse/issues/723)) ([2acae0e](https://github.com/pmatos/jsse/commit/2acae0e38f6ddd533d259581dc0f6c41213f0621))
* **generators:** suspend for-await head in async generators instead of draining microtasks ([#720](https://github.com/pmatos/jsse/issues/720)) ([0a15c59](https://github.com/pmatos/jsse/commit/0a15c59afdccfc71f5478bb040ea0b6fc05d7dda))
* **generators:** suspend inline yield and yield* in async generators ([#729](https://github.com/pmatos/jsse/issues/729)) ([3a5c266](https://github.com/pmatos/jsse/commit/3a5c266c3b232edfdd9f952c1dacc39b6e3fd67c))
* **generators:** suspend yield in var/let/const pattern defaults ([#744](https://github.com/pmatos/jsse/issues/744)) ([1ec69dc](https://github.com/pmatos/jsse/commit/1ec69dc4fb918f8a8e7a3f11cc5d417c97d9662e))
* **generators:** treat yield/await in switch case tests as suspension points ([#692](https://github.com/pmatos/jsse/issues/692)) ([5ca7aa3](https://github.com/pmatos/jsse/commit/5ca7aa360019a3b9570b050f0a29e20badeb5a60))
* **interpreter:** calibrate CALL_DEPTH_*/EVAL_DEPTH_LIMIT per build profile ([#736](https://github.com/pmatos/jsse/issues/736)) ([7bc2caa](https://github.com/pmatos/jsse/commit/7bc2caa631e5f9979f7c2d51c91cbc8017c38850))
* **intl:** remove spurious Intl.Locale.supportedLocalesOf static ([#662](https://github.com/pmatos/jsse/issues/662)) ([0674e9a](https://github.com/pmatos/jsse/commit/0674e9a7c4f5789052215cbedb04b0c07d32c2ec))
* **lexer:** round non-decimal numeric literals wider than 64 bits ([#698](https://github.com/pmatos/jsse/issues/698)) ([6b519ee](https://github.com/pmatos/jsse/commit/6b519ee3efc5a999e38176b5caab9be9efffae87))
* **number:** compute exact radix digits for magnitudes >= 2^63 ([#700](https://github.com/pmatos/jsse/issues/700)) ([661efa8](https://github.com/pmatos/jsse/commit/661efa84bac0d7ae974c71ac3fbc35ca5e38450d))
* **optional-chain:** throw on private access with non-object base ([#660](https://github.com/pmatos/jsse/issues/660)) ([f2f9faa](https://github.com/pmatos/jsse/commit/f2f9faa1617556bbdc88167265b0755b8deeca25))
* **parseInt:** round integers above 2^53 once instead of per digit ([#676](https://github.com/pmatos/jsse/issues/676)) ([dcc896c](https://github.com/pmatos/jsse/commit/dcc896c88dd8655520ac65c4bf06dfab98527fcd))
* **parser:** reject script-level function/lexical redeclaration and unleak JetStream harness ([#673](https://github.com/pmatos/jsse/issues/673)) ([b6860cc](https://github.com/pmatos/jsse/commit/b6860ccb6e4964b5d8e69b95f4aa9e3069879709))
* **parser:** reject strict-reserved name on async function expression ([#667](https://github.com/pmatos/jsse/issues/667)) ([90f6c77](https://github.com/pmatos/jsse/commit/90f6c7771bcd362fcff6cc3fdba9f78696b44a03))
* **scripts:** fix JetStream runner self, preload and async-failure gaps ([#681](https://github.com/pmatos/jsse/issues/681)) ([3db1026](https://github.com/pmatos/jsse/commit/3db1026c0f4869a81bf3a3d7d448cce99325fc5c))
* **temporal:** constrain overflow in PlainYearMonth.prototype.toPlainDate ([#668](https://github.com/pmatos/jsse/issues/668)) ([fa77ea9](https://github.com/pmatos/jsse/commit/fa77ea932dd04464af7987fcdd615196145ddfb2))
* **temporal:** evaluate POSIX TZ footer rules for named-zone offsets ([#659](https://github.com/pmatos/jsse/issues/659)) ([4d03526](https://github.com/pmatos/jsse/commit/4d0352646465d1836faaed52aedee7a4a3551b98))


### Features

* **console:** add error, warn, info, debug methods ([#702](https://github.com/pmatos/jsse/issues/702)) ([a056795](https://github.com/pmatos/jsse/commit/a0567954c60ba98648964a840604da12202cc844))

## [0.8.2](https://github.com/pmatos/jsse/compare/v0.8.1...v0.8.2) (2026-09-20)


### Performance Improvements

* **json:** skip descriptor-object round trip in enumerable_own_keys ([#656](https://github.com/pmatos/jsse/issues/656)) ([ef13eef](https://github.com/pmatos/jsse/commit/ef13eeff869c7fb8ac8e37d7378f6fa921603531))

## [0.8.1](https://github.com/pmatos/jsse/compare/v0.8.0...v0.8.1) (2026-09-18)


### Bug Fixes

* **symphonika:** gate implement stage on an open PR, not just a commit ([#627](https://github.com/pmatos/jsse/issues/627)) ([af0ac25](https://github.com/pmatos/jsse/commit/af0ac25446fb147528cd1c0c575527fabf16c09c))
* **temporal:** validate Instant.prototype.toString timeZone against tz database ([#630](https://github.com/pmatos/jsse/issues/630)) ([ff4b96a](https://github.com/pmatos/jsse/commit/ff4b96ae285d5c9783683524508263c479cffbed))

# [0.8.0](https://github.com/pmatos/jsse/compare/v0.7.0...v0.8.0) (2026-09-13)


### Bug Fixes

* **ast:** walk mandatory AST passes with an explicit stack ([#613](https://github.com/pmatos/jsse/issues/613)) ([0c35f1f](https://github.com/pmatos/jsse/commit/0c35f1f304bc31361f88308e841a634c4ecabae3))
* **iterator:** keep concat current iterator GC-rooted ([#604](https://github.com/pmatos/jsse/issues/604)) ([4a01fdd](https://github.com/pmatos/jsse/commit/4a01fdd99d57e81f17f2afd289ffacc89a9ead3c)), closes [#585](https://github.com/pmatos/jsse/issues/585) [#585](https://github.com/pmatos/jsse/issues/585) [#609](https://github.com/pmatos/jsse/issues/609) [#610](https://github.com/pmatos/jsse/issues/610) [#584](https://github.com/pmatos/jsse/issues/584)
* **parser:** calibrate MAX_PARSE_DEPTH per build profile ([#606](https://github.com/pmatos/jsse/issues/606)) ([8d6151a](https://github.com/pmatos/jsse/commit/8d6151af6415127c8831be032be7f561516546a2)), closes [#598](https://github.com/pmatos/jsse/issues/598) [#607](https://github.com/pmatos/jsse/issues/607) [#599](https://github.com/pmatos/jsse/issues/599)
* **parser:** restore context counters when a body parse fails ([#602](https://github.com/pmatos/jsse/issues/602)) ([626ff19](https://github.com/pmatos/jsse/commit/626ff19ae8dae159f48717f1031a3bee42aa2578))


### Features

* **bytecode:** compile new expressions and compound member assignment ([#611](https://github.com/pmatos/jsse/issues/611)) ([4267239](https://github.com/pmatos/jsse/commit/4267239f7b2a2fb2032119c20089efa27a16699f))

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
