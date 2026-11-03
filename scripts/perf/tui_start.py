#!/usr/bin/env python3
"""TUI cold start: from exec to the first complete frame, in a pseudo-terminal (160×40) with an empty scratch --config-dir.
The first frame is complete when the status line's last words ("F1 keys", escape sequences removed) arrive: ratatui writes top to bottom.
The pty answers the terminal queries crossterm sends at start (primary device attributes, cursor position) the way a terminal does.
Prints p50 / p95 / min-max over the runs, and `--version` (process start and exit only) for comparison.

Run:  python3 scripts/perf/tui_start.py <qurry-tui binary> [runs]   (scripts/perf/tui.sh builds and passes it)
"""
import fcntl, math, os, pty, re, select, shutil, signal, struct, subprocess, sys, tempfile, termios, time

MARK = b"F1 keys"


def first_frame(exe, cfg):
    t0 = time.perf_counter()
    pid, fd = pty.fork()
    if pid == 0:
        os.environ["TERM"] = "xterm-256color"
        os.execv(exe, [exe, "--config-dir", cfg, "--no-update-check"])
    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", 40, 160, 0, 0))
    buf = b""
    try:
        while time.perf_counter() - t0 < 10:
            r, _, _ = select.select([fd], [], [], 0.5)
            if not r:
                continue
            chunk = os.read(fd, 65536)
            buf += chunk
            if b"\x1b[c" in chunk or b"\x1b[0c" in chunk:
                os.write(fd, b"\x1b[?62;22c")
            if b"\x1b[6n" in chunk:
                os.write(fd, b"\x1b[1;1R")
            if MARK in re.sub(rb"\x1b\[[0-9;?]*[A-Za-z]", b"", buf):
                return (time.perf_counter() - t0) * 1000
        raise RuntimeError("no first frame within 10 s: " + repr(buf[-300:]))
    finally:
        os.kill(pid, signal.SIGKILL)
        os.waitpid(pid, 0)
        os.close(fd)


def version(exe):
    t0 = time.perf_counter()
    subprocess.run([exe, "--version"], stdout=subprocess.DEVNULL, check=True)
    return (time.perf_counter() - t0) * 1000


def stats(xs):
    xs = sorted(xs)
    pct = lambda q: xs[max(1, math.ceil(len(xs) * q)) - 1]
    return f"p50 {pct(0.5):7.1f} ms  p95 {pct(0.95):7.1f} ms  min-max {xs[0]:.1f}-{xs[-1]:.1f} ms  n {len(xs)}"


exe = sys.argv[1]
runs = int(sys.argv[2]) if len(sys.argv) > 2 else 20
starts, versions = [], []
for i in range(runs + 2):
    cfg = tempfile.mkdtemp(prefix="qurry-perf-")
    try:
        ms = first_frame(exe, cfg)
    finally:
        shutil.rmtree(cfg, ignore_errors=True)
    v = version(exe)
    if i >= 2:  # the first two warm the page cache
        starts.append(ms)
        versions.append(v)
print(f"PERF {'TUI cold start: exec → first frame':<44} {stats(starts)}")
print(f"PERF {'TUI --version: exec → exit':<44} {stats(versions)}")
