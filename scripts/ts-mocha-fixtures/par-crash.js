// The TypeScript runner as a parallel host, with worker 0 made to fail when its first task arrives:
// CRASH_MODE=exit (process.exit(3)), kill (SIGKILL), throw (an uncaught exception, which the worker reports
// itself over IPC) or reject (an unhandled rejection). The host forks process.argv[1], i.e. this file, so
// the workers load it too; they are the processes started with --config=<task-config<i>.json>.
const config = process.argv.find(a => a.startsWith("--config="));
if (config && config.includes("task-config0.json")) {
    process.once("message", () => {
        console.log("worker 0 is about to fail");
        switch (process.env.CRASH_MODE) {
            case "exit": process.exit(3);
            case "kill": process.kill(process.pid, "SIGKILL");
            case "throw": setTimeout(() => { throw new Error("worker boom"); }, 0); break;
            case "reject": Promise.reject(new Error("worker reject")); break;
        }
    });
}
require(require("path").resolve(process.cwd(), "built/local/run.js"));
