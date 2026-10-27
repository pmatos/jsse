#!/usr/bin/env python3
"""Run a command with its stdio on a pseudo-terminal and print what it wrote.

usage: pty-run.py [--rows N] [--cols N] [--input TEXT] [--wait-raw] [--stdout-pipe] -- cmd args...

The child gets the pty as stdin/stdout/stderr (with --stdout-pipe, stdout is a
pipe and only stdin/stderr are the tty). --input is written to the terminal
when the child is ready: it left canonical mode, or --delay
seconds passed (\\n and \\r escapes allowed; a trailing \\x04 sends EOF).
With --wait-raw the delay is no use as a fallback and input waits for raw mode
alone: a slow start must not let the terminal's line discipline see the keys
first (^C would kill the child, ^D end its input, and echo would differ).
Output is the raw bytes the child produced, with the tty's output processing
(\\n to \\r\\n) left in place.
"""

import os
import pty
import sys
import select
import struct
import fcntl
import termios
import argparse
import time
import signal

ap = argparse.ArgumentParser()
ap.add_argument("--rows", type=int, default=24)
ap.add_argument("--cols", type=int, default=80)
ap.add_argument("--input", default="")
ap.add_argument("--delay", type=float, default=0.3)
ap.add_argument("--timeout", type=float, default=20)
ap.add_argument("--wait-raw", action="store_true")
ap.add_argument("--stdout-pipe", action="store_true")
ap.add_argument("cmd", nargs=argparse.REMAINDER)
args = ap.parse_args()
cmd = args.cmd[1:] if args.cmd and args.cmd[0] == "--" else args.cmd

master, slave = pty.openpty()
fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", args.rows, args.cols, 0, 0))
pipe_r = pipe_w = None
if args.stdout_pipe:
    pipe_r, pipe_w = os.pipe()

pid = os.fork()
if pid == 0:
    os.setsid()
    fcntl.ioctl(slave, termios.TIOCSCTTY, 0)
    os.dup2(slave, 0)
    os.dup2(pipe_w if pipe_w is not None else slave, 1)
    os.dup2(slave, 2)
    for fd in (master, slave, pipe_r, pipe_w):
        if fd is not None and fd > 2:
            os.close(fd)
    os.execvp(cmd[0], cmd)
os.close(slave)
if pipe_w is not None:
    os.close(pipe_w)


def child_ready():
    try:
        return not termios.tcgetattr(master)[3] & termios.ICANON
    except termios.error:
        return False


data = args.input.encode().decode("unicode_escape").encode("latin-1")
out = b""
sent = False
start = time.time()
fds = [master] + ([pipe_r] if pipe_r is not None else [])
while True:
    elapsed = time.time() - start
    if not sent and (child_ready() or (elapsed >= args.delay and not args.wait_raw)):
        if data:
            os.write(master, data)
        sent = True
    r, _, _ = select.select(fds, [], [], 0.05)
    done = False
    for fd in r:
        try:
            chunk = os.read(fd, 65536)
        except OSError:
            chunk = b""
        if not chunk:
            fds.remove(fd)
            if fd == master or not fds:
                done = True
        else:
            out += chunk
    if done or not fds:
        break
    if time.time() - start > args.timeout:
        os.kill(pid, signal.SIGKILL)
        out += b"\n[pty-run: timeout]\n"
        break
_, status = os.waitpid(pid, 0)
sys.stdout.buffer.write(out)
sys.stdout.buffer.flush()
sys.exit(os.waitstatus_to_exitcode(status))
