#!/usr/bin/env python3
"""Cross-check jsse --node against Node on stdio, tty and readline (#1232).

Every tty-fixtures/*.fixture.js runs in the modes its header lists, on jsse
(`--node`) and on Node, under `env -i`, with the stdio kind of the mode:

  pipe  stdin, stdout and stderr are pipes (stdin carries INPUT)
  file  stdin, stdout and stderr are regular files
  pty   all three are one pseudo-terminal of SIZE (INPUT is typed on it)

Header lines:  // MODES: pipe file pty    // INPUT: text with \\n \\r \\xNN
               // ENV: K=V ...           // SIZE: ROWSxCOLS   // DELAY: seconds

The observation is exit status plus the bytes written. It must be identical on
both engines and equal to expected/<case>.<mode>.txt, which `--update` writes
from Node. Without Node the expected files alone are the oracle.
"""

import argparse
import os
import shutil
import subprocess
import sys
import tempfile
import zlib

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
PTY_RUN = os.path.join(HERE, "pty-run.py")
BASE_ENV = {
    "PATH": os.environ.get("PATH", "/usr/bin:/bin"),
    "HOME": "/nonexistent",
    "TERM": "xterm-256color",
    "LANG": "C.UTF-8",
}


def headers(path):
    out = {
        "MODES": "pipe file",
        "INPUT": "",
        "ENV": "",
        "SIZE": "24x80",
        "DELAY": "0.3",
    }
    with open(path) as f:
        for line in f:
            if not line.startswith("// "):
                break
            key, _, value = line[3:].partition(":")
            if key.strip() in out:
                out[key.strip()] = value.strip()
    return out


def unescape(text):
    return text.encode().decode("unicode_escape").encode("latin-1")


def show(data):
    if len(data) > 4096:
        digest = zlib.crc32(data)
        return f"[{len(data)} bytes crc32 {digest}]\n"
    return (
        data.decode("utf-8", "backslashreplace")
        .replace("\x1b", "\\e")
        .replace("\r", "\\r")
    )


def run(engine, script, mode, hdr, work):
    env = dict(BASE_ENV)
    for word in hdr["ENV"].split():
        key, _, value = word.partition("=")
        env[key] = value
    cmd = engine + [script]
    data = unescape(hdr["INPUT"])
    if mode == "pty":
        rows, cols = hdr["SIZE"].split("x")
        argv = (
            [
                sys.executable,
                PTY_RUN,
                "--rows",
                rows,
                "--cols",
                cols,
                "--input",
                hdr["INPUT"],
                "--delay",
                hdr["DELAY"],
                "--",
            ]
            + ["env", "-i"]
            + [f"{k}={v}" for k, v in env.items()]
            + cmd
        )
        p = subprocess.run(argv, capture_output=True, timeout=60)
        return f"exit={p.returncode}\n--- tty\n{show(p.stdout)}"
    if mode == "pipe":
        p = subprocess.run(
            cmd, input=data, capture_output=True, env=env, cwd=work, timeout=60
        )
        out, err = p.stdout, p.stderr
    else:
        paths = [os.path.join(work, n) for n in ("in", "out", "err")]
        with open(paths[0], "wb") as f:
            f.write(data)
        with (
            open(paths[0], "rb") as fin,
            open(paths[1], "wb") as fout,
            open(paths[2], "wb") as ferr,
        ):
            p = subprocess.run(
                cmd, stdin=fin, stdout=fout, stderr=ferr, env=env, cwd=work, timeout=60
            )
        out, err = open(paths[1], "rb").read(), open(paths[2], "rb").read()
    return f"exit={p.returncode}\n--- stdout\n{show(out)}--- stderr\n{show(err)}"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument(
        "--jsse",
        default=os.environ.get("JSSE", os.path.join(ROOT, "target/release/jsse")),
    )
    ap.add_argument("--node", action="store_true", help="run on Node only")
    ap.add_argument(
        "--no-cross-check", action="store_true", help="jsse only, against expected/"
    )
    ap.add_argument("--update", action="store_true", help="write expected/ from Node")
    ap.add_argument("--only", help="run only this case name")
    args = ap.parse_args()

    node = shutil.which("node")
    cross = node is not None and not args.no_cross_check
    if node is None and (args.node or args.update):
        sys.exit("node is required for --node and --update")
    exp_dir = os.path.join(HERE, "expected")
    os.makedirs(exp_dir, exist_ok=True)
    failed = 0
    count = 0
    with tempfile.TemporaryDirectory() as work:
        for name in sorted(os.listdir(HERE)):
            if not name.endswith(".fixture.js"):
                continue
            case = name[: -len(".fixture.js")]
            if args.only and args.only != case:
                continue
            hdr = headers(os.path.join(HERE, name))
            script = os.path.join(work, case + ".js")
            shutil.copy(os.path.join(HERE, name), script)
            for mode in hdr["MODES"].split():
                label = f"{case} [{mode}]"
                count += 1
                try:
                    got = (
                        None
                        if args.node
                        else run([args.jsse, "--node"], script, mode, hdr, work)
                    )
                    ref = (
                        run([node, "--no-warnings"], script, mode, hdr, work)
                        if (cross or args.node or args.update)
                        else None
                    )
                except subprocess.TimeoutExpired:
                    print(f"FAIL {label}: timeout")
                    failed += 1
                    continue
                path = os.path.join(exp_dir, f"{case}.{mode}.txt")
                if args.update:
                    open(path, "w").write(ref)
                    print(f"wrote {label}")
                    continue
                if args.node:
                    print(f"ok   {label} (node)")
                    continue
                want = open(path).read() if os.path.exists(path) else None
                if want is None:
                    print(f"FAIL {label}: no expected output (run with --update)")
                    failed += 1
                elif got != want:
                    print(
                        f"FAIL {label}: differs from expected\n--- expected\n{want}--- jsse\n{got}"
                    )
                    failed += 1
                elif ref is not None and ref != want:
                    print(
                        f"FAIL {label}: node differs from expected\n--- expected\n{want}--- node\n{ref}"
                    )
                    failed += 1
                else:
                    print(f"ok   {label}")
    print(f"node-tty oracle: {count - failed}/{count} passed")
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
