// What `hereby runtests` does after its build tasks: parse the runner flags, write test.config, start mocha on
// the runner bundle, and consume or leave .failed-tests. TS_RUN_JS swaps the bundle for a fixture spec;
// TS_PARALLEL=1 is `hereby runtests-parallel` (run.js becomes the host that forks --workers workers).
import { runConsoleTests } from "./scripts/build/tests.mjs";

try {
    await runConsoleTests(
        process.env.TS_RUN_JS || "built/local/run.js",
        process.env.TS_DEFAULT_REPORTER || "mocha-fivemat-progress-reporter",
        /*runInParallel*/ process.env.TS_PARALLEL === "1",
    );
}
catch (e) {
    console.error(`${e.name}: ${e.message}`);
}
