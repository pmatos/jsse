// Test262 host hooks for Node.js (loaded via --require)
'use strict';

const vm = require('vm');

globalThis.print = function(x) {
    console.log(x);
};

(function() {
    const reportList = [];
    const workerList = [];

    function setup$262(ctx, isNewRealm) {
        const obj = {
            global: ctx,
            gc: function() {
                if (typeof gc === 'function') {
                    gc();
                } else {
                    throw new Error('gc() not available');
                }
            },
            createRealm: function() {
                const sandbox = vm.createContext({
                    print: globalThis.print,
                    console: console
                });
                if (typeof gc === 'function') sandbox.gc = gc;
                setup$262(sandbox, true);
                return sandbox.$262;
            },
            evalScript: function(code) {
                if (isNewRealm) {
                    return vm.runInContext(code, ctx);
                }
                return (0, eval)(code);
            },
            detachArrayBuffer: function(buf) {
                structuredClone(buf, { transfer: [buf] });
            },
            agent: {
                start: function(script) {
                    const { Worker } = require('worker_threads');
                    const workerScript = `
                        const { parentPort } = require('worker_threads');
                        globalThis.$262 = {
                            agent: {
                                receiveBroadcast: function(cb) {
                                    parentPort.on('message', function(msg) {
                                        if (msg.type === 'broadcast') {
                                            cb(msg.sab);
                                        }
                                    });
                                },
                                report: function(value) {
                                    parentPort.postMessage({ type: 'report', value: String(value) });
                                },
                                sleep: function(ms) {
                                    Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, Number(ms));
                                },
                                leaving: function() {},
                                monotonicNow: function() {
                                    return performance.now();
                                }
                            }
                        };
                        ${script}
                    `;
                    const w = new Worker(workerScript, { eval: true });
                    w.on('message', function(msg) {
                        if (msg.type === 'report') {
                            reportList.push(msg.value);
                        }
                    });
                    w.on('error', function() {});
                    workerList.push(w);
                },
                broadcast: function(sab) {
                    for (const w of workerList) {
                        w.postMessage({ type: 'broadcast', sab: sab });
                    }
                },
                getReport: function() {
                    if (reportList.length > 0) {
                        return reportList.shift();
                    }
                    return null;
                },
                sleep: function(ms) {
                    Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, Number(ms));
                },
                monotonicNow: function() {
                    return performance.now();
                }
            }
        };

        if (isNewRealm) {
            ctx.$262 = obj;
        } else {
            globalThis.$262 = obj;
        }
    }

    setup$262(globalThis, false);
})();
