import glob, json, os, statistics, subprocess, sys, time

engines = json.loads(sys.argv[1])
bench_dir, out, repeats = sys.argv[2], sys.argv[3], int(sys.argv[4])
TIMEOUT = 120


def loadavg():
    return open("/proc/loadavg").read().split()[:3]


def timed(cmd):
    t = time.perf_counter()
    p = subprocess.Popen(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        p.wait(timeout=TIMEOUT)
    except subprocess.TimeoutExpired:
        p.kill()
        p.wait()
        return None, None
    return time.perf_counter() - t, p.returncode


res = {"started": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
       "loadavg_start": loadavg(), "repeats": repeats, "timeout_s": TIMEOUT,
       "results": {}, "loadavg1_before": {}}
for b in sorted(glob.glob(os.path.join(bench_dir, "bench_*.js"))):
    name = os.path.basename(b)[:-3]
    res["loadavg1_before"][name] = float(loadavg()[0])
    for label, argv in engines.items():
        times, rcs, timed_out = [], [], False
        for _ in range(repeats):
            dt, rc = timed(argv + [b])
            if dt is None:
                timed_out = True
                break
            times.append(dt)
            rcs.append(rc)
        ent = {"all_s": times, "returncodes": rcs, "timed_out": timed_out}
        if times:
            ent.update(median_s=statistics.median(times), min_s=min(times), max_s=max(times))
        res["results"].setdefault(label, {})[name] = ent
        print(name, label, ent.get("median_s"), rcs[:1], "TIMEOUT" if timed_out else "", flush=True)
res["loadavg_end"] = loadavg()
res["ended"] = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
json.dump(res, open(out, "w"), indent=1)
