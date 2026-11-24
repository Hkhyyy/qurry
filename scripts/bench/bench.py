#!/usr/bin/env python3
"""Measurements for scripts/bench.sh (macOS, Python 3 standard library only). bench.sh builds the binaries, starts the database
and writes the scratch config folder; this file runs each tool, takes the median of the runs and prints the markdown tables.

Memory is phys_footprint, the number Activity Monitor shows, read with footprint(1): "idle" is its current value, "peak" the
kernel's high-water mark of it (phys_footprint_peak), summed over the tool's processes.

Headless (nothing on screen):
  TUI   in a pseudo-terminal (160 x 40, a fresh empty --config-dir each run) with a small screen model, so a step ends when its
        words are on the status line: launch -> first frame, idle memory 10 s after the data source connected, the 1k-row
        query end to end (run, then load the second page into the grid), the 1M-row query (first page, then the grid's
        100,000-row cap) and the peak memory of the whole run.
  MCP   over stdio: spawn -> initialize reply, idle memory 10 s after it, the query tool with 1,000 rows (the first call
        connects, the next five are warm), and the peak memory.
  core  crates/core/examples/fetch_bench.rs: all 1M rows through a console session, 500 rows a page, kept in memory.
GUI (--gui, --compare, instead of the headless runs; opens windows, asks the person at the keyboard to connect and run the
queries): gui_run().
"""
import argparse, codecs, fcntl, json, os, pty, re, select, shutil, signal, statistics, struct, subprocess, sys, tempfile, termios, time, unicodedata
from pathlib import Path

SOURCE = "acme-bench"  # id and name of the data source bench.sh writes into the scratch config folder
Q1K = "SELECT * FROM shop.orders ORDER BY id LIMIT 1000"
Q1M = "SELECT * FROM shop.orders"
IDLE_S = 10
ROWS, COLS = 40, 160


def sh(*cmd):
    return subprocess.run(cmd, capture_output=True, text=True).stdout.strip()


def footprint(pids):
    """(now, peak) phys_footprint in bytes, summed over the processes still alive"""
    args = ["footprint", "-f", "bytes", "--noCategories"]
    for p in pids:
        args += ["-p", str(p)]
    out = sh(*args)
    now = sum(int(n) for n in re.findall(r"phys_footprint: (\d+) B", out))
    peak = sum(int(n) for n in re.findall(r"phys_footprint_peak: (\d+) B", out))
    return now, peak


def load():
    return os.getloadavg()[0]


# ---------- a terminal screen, enough for ratatui's output ----------
# CSI, OSC, a charset choice, or another two-byte escape (no match: cut at the end of a read)
ESC = re.compile(r"\x1b(?:\[[0-?]*[ -/]*[@-~]|\][^\x07\x1b]*(?:\x07|\x1b\\)|[()][0-9A-Za-z]|[^\[\]()])")


class Screen:
    def __init__(self):
        self.cells = [[" "] * COLS for _ in range(ROWS)]
        self.y = self.x = 0
        self.dec = codecs.getincrementaldecoder("utf-8")("replace")
        self.pending = ""

    def feed(self, data):
        s = self.pending + self.dec.decode(data)
        self.pending = ""
        i = 0
        while i < len(s):
            c = s[i]
            if c == "\x1b":
                m = ESC.match(s, i)
                if not m:
                    self.pending = s[i:]
                    return
                seq = m.group(0)
                if seq.startswith("\x1b[") and seq.endswith("H"):
                    p = [int(n) if n else 1 for n in seq[2:-1].split(";")] + [1, 1]
                    self.y, self.x = min(p[0], ROWS) - 1, min(p[1], COLS) - 1
                elif seq in ("\x1b[2J", "\x1b[J"):
                    self.cells = [[" "] * COLS for _ in range(ROWS)]
                i = m.end()
                continue
            if c == "\r":
                self.x = 0
            elif c == "\n":
                self.y = min(self.y + 1, ROWS - 1)
            elif c >= " ":
                if self.x < COLS:
                    self.cells[self.y][self.x] = c
                self.x += 2 if unicodedata.east_asian_width(c) in "WF" else 1
            i += 1

    def line(self, y):
        return "".join(self.cells[y]).rstrip()

    def text(self):
        return "\n".join(self.line(y) for y in range(ROWS))


class Tui:
    """qurry-tui in a pty, answering the terminal queries crossterm sends (as scripts/perf/tui_start.py does)"""

    def __init__(self, exe, args):
        self.t0 = time.perf_counter()
        self.pid, self.fd = pty.fork()
        if self.pid == 0:
            os.environ["TERM"] = "xterm-256color"
            os.execv(exe, [exe, *args])
        fcntl.ioctl(self.fd, termios.TIOCSWINSZ, struct.pack("HHHH", ROWS, COLS, 0, 0))
        self.screen = Screen()

    def status(self):
        return self.screen.line(ROWS - 1)

    def read(self, timeout):
        if select.select([self.fd], [], [], timeout)[0]:
            chunk = os.read(self.fd, 1 << 16)
            if b"\x1b[c" in chunk or b"\x1b[0c" in chunk:
                os.write(self.fd, b"\x1b[?62;22c")
            if b"\x1b[6n" in chunk:
                os.write(self.fd, b"\x1b[1;1R")
            self.screen.feed(chunk)

    def wait(self, pattern, where=None, timeout=120):
        """Read until the status line (or where()) matches; ms since t0"""
        rx, where = re.compile(pattern), where or self.status
        end = time.perf_counter() + timeout
        while not rx.search(where()):
            if time.perf_counter() > end:
                raise RuntimeError(f"TUI: no /{pattern}/ within {timeout} s; status line: {self.status()!r}")
            self.read(0.2)
        return (time.perf_counter() - self.t0) * 1000

    def wait_change(self, before, timeout=120):
        end = time.perf_counter() + timeout
        while self.status() == before:
            if time.perf_counter() > end:
                raise RuntimeError(f"TUI: the status line stayed {before!r} for {timeout} s")
            self.read(0.2)

    def drain(self, seconds):
        """Keep reading (so the TUI never blocks on a full pty) for a while"""
        end = time.perf_counter() + seconds
        while (left := end - time.perf_counter()) > 0:
            self.read(left)

    def send(self, data, start=False):
        if start:
            self.t0 = time.perf_counter()
        os.write(self.fd, data)

    def kill(self):
        os.kill(self.pid, signal.SIGKILL)
        os.waitpid(self.pid, 0)
        os.close(self.fd)


F5, SHIFT_TAB, CTRL_L, END = b"\x1b[15~", b"\x1b[Z", b"\x0c", b"G"


def paste(text):
    return b"\x1b[200~" + text.encode() + b"\x1b[201~"


def tui_run(exe, url):
    cfg = tempfile.mkdtemp(prefix="qurry-bench-tui-")
    t = Tui(exe, ["--config-dir", cfg, "--no-update-check", url])
    r = {}

    def query(sql, done):
        """New console, paste, run; then the grid's End key (it loads the next page near the last row) until the status line
        says `done`. → (ms to the first page, ms to `done`)"""
        t.send(CTRL_L)
        t.drain(0.5)
        t.send(paste(sql))
        t.drain(0.3)
        t.send(F5, start=True)
        first = t.wait(r"row\(s\) retrieved in")
        t.send(SHIFT_TAB)
        while not re.search(done, t.status()):
            before = t.status()
            t.send(END)
            t.wait_change(before)
        return first, (time.perf_counter() - t.t0) * 1000

    try:
        r["launch"] = t.wait(r"F1 keys")
        # Connected: the explorer shows the URL's database under the data source
        t.wait(r"⛁ shop default", where=t.screen.text)
        t.drain(IDLE_S)
        r["idle"] = footprint([t.pid])[0]
        _, r["q1k"] = query(Q1K, r"\b1000 rows")
        r["q1m_first"], r["q1m_cap"] = query(Q1M, r"Showing the first 100000 rows")
        r["peak"] = footprint([t.pid])[1]
    finally:
        t.kill()
        shutil.rmtree(cfg, ignore_errors=True)
    return r


# ---------- MCP over stdio ----------
def mcp_run(exe, cfg, warm=5):
    t0 = time.perf_counter()
    p = subprocess.Popen([exe, "--config-dir", cfg], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, bufsize=1)
    n = 0

    def call(method, params=None):
        nonlocal n
        n += 1
        p.stdin.write(json.dumps({"jsonrpc": "2.0", "id": n, "method": method, "params": params or {}}) + "\n")
        p.stdin.flush()
        reply = json.loads(p.stdout.readline())
        if "error" in reply or reply.get("result", {}).get("isError"):
            raise RuntimeError(f"MCP {method}: {reply}")
        return reply["result"]

    r = {}
    try:
        call("initialize", {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "bench", "version": "1"}})
        r["start"] = (time.perf_counter() - t0) * 1000
        p.stdin.write(json.dumps({"jsonrpc": "2.0", "method": "notifications/initialized"}) + "\n")
        p.stdin.flush()
        call("tools/list")
        time.sleep(IDLE_S)
        r["idle"] = footprint([p.pid])[0]
        times = []
        for _ in range(warm + 1):
            t = time.perf_counter()
            res = call("tools/call", {"name": "query", "arguments": {"data_source": SOURCE, "sql": Q1K, "limit": 1000}})
            times.append((time.perf_counter() - t) * 1000)
            if len(json.loads(res["content"][0]["text"])["rows"]) != 1000:
                raise RuntimeError("MCP query: not 1,000 rows")
        r["query_cold"], r["query_warm"] = times[0], statistics.median(times[1:])
        r["peak"] = footprint([p.pid])[1]
    finally:
        p.stdin.close()
        p.wait()
    return r


# ---------- core ----------
def core_run(exe, cfg):
    p = subprocess.Popen([exe, cfg, SOURCE, Q1M], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
    try:
        r = json.loads(p.stdout.readline())
        peak = footprint([p.pid])[1]  # it waits for stdin, still holding the rows
    finally:
        p.stdin.close()
        p.wait()
    if r["rows"] != 1_000_000:
        raise RuntimeError(f"core: {r['rows']} rows")
    return {"first": r["first_page_ms"], "all": r["all_rows_ms"], "peak": peak}


# ---------- GUI (opens windows) ----------
def bundle(app, key):
    return sh("plutil", "-extract", key, "raw", str(Path(app) / "Contents/Info.plist"))


def gui_apps(a):
    """[(name, .app path, or None when it isn't installed)]"""
    apps = []
    if a.gui:
        apps.append(("Qurry GUI", os.environ.get("QURRY_APP", "/Applications/Qurry.app")))
    if a.compare:
        apps.append(("DataGrip", os.environ.get("DATAGRIP_APP", "/Applications/DataGrip.app")))
        found = [p for p in ("/Applications/DBeaver.app", "/Applications/DBeaverLite.app", "/Applications/DBeaverEE.app", "/Applications/DBeaverUltimate.app") if Path(p).exists()]
        apps.append(("DBeaver", os.environ.get("DBEAVER_APP", found[0] if found else "/Applications/DBeaver.app")))
    return [(n, p if Path(p).exists() else None) for n, p in apps]


def datagrip_project(root, port):
    """A DataGrip project folder whose only data source is acme-bench (no password: the container trusts local connections)"""
    idea = Path(root) / "acme-bench" / ".idea"
    idea.mkdir(parents=True)
    uuid = "6f1b2c3d-0000-4000-8000-acbe4c400001"
    (idea / "dataSources.xml").write_text(f"""<?xml version="1.0" encoding="UTF-8"?>
<project version="4">
  <component name="DataSourceManagerImpl" format="xml" multifile-model="true">
    <data-source source="LOCAL" name="{SOURCE}" uuid="{uuid}">
      <driver-ref>postgresql</driver-ref>
      <synchronize>true</synchronize>
      <jdbc-driver>org.postgresql.Driver</jdbc-driver>
      <jdbc-url>jdbc:postgresql://127.0.0.1:{port}/shop</jdbc-url>
      <working-dir>$ProjectFileDir$</working-dir>
    </data-source>
  </component>
</project>
""")
    (idea / "dataSources.local.xml").write_text(f"""<?xml version="1.0" encoding="UTF-8"?>
<project version="4">
  <component name="dataSourceStorageLocal">
    <data-source name="{SOURCE}" uuid="{uuid}">
      <secret-storage>master_key</secret-storage>
      <user-name>postgres</user-name>
    </data-source>
  </component>
</project>
""")
    return str(idea.parent)


def launch_args(name, exe, a, scratch):
    """→ (argv, env). Qurry: a scratch home folder (your data sources, tabs and settings are neither read nor changed) with the
    acme-bench data source in it. DataGrip: its own settings (and license), on a project folder holding acme-bench. DBeaver:
    acme-bench passed on its command line, not saved"""
    env = dict(os.environ)
    if name == "Qurry GUI":
        home = Path(scratch) / "qurry-home"
        cfg = home / "Library/Application Support/com.hyunkyo.dbtool"
        if not cfg.exists():
            cfg.mkdir(parents=True)
            shutil.copy(Path(a.config) / "connections.json", cfg / "connections.json")
        env.update(HOME=str(home), CFFIXED_USER_HOME=str(home), QURRY_NO_UPDATE_CHECK="1")
        return [exe], env
    if name == "DataGrip":
        project = Path(scratch) / "acme-bench"
        return [exe, str(project) if project.exists() else datagrip_project(scratch, a.port)], env
    con = f"name={SOURCE}|driver=postgresql|host=127.0.0.1|port={a.port}|database=shop|user=postgres|save=false|connect=false"
    return [exe, "-con", con], env


def tree(pid):
    """pid and its descendants"""
    kids = {}
    for line in sh("ps", "-A", "-o", "pid=,ppid=").splitlines():
        c, p = map(int, line.split())
        kids.setdefault(p, []).append(c)
    out, todo = [], [pid]
    while todo:
        p = todo.pop()
        out.append(p)
        todo += kids.get(p, [])
    return out


def webkit():
    """WebKit's helper processes (WebContent, Networking, GPU): launchd starts them, so they aren't the app's children"""
    return {int(l.split()[0]) for l in sh("ps", "-A", "-o", "pid=,comm=").splitlines() if "com.apple.WebKit." in l}


def ask(text):
    return input(f"    {text}\n    > ").strip()


def ask_ms(text):
    while True:
        v = ask(text + " (ms; Enter to skip)").replace(",", "")
        if not v:
            return None
        try:
            return float(v)
        except ValueError:
            print("    a number, e.g. 85")


def gui_run(name, app, a, scratch, winwait, prompts):
    exe = str(Path(app) / "Contents/MacOS" / bundle(app, "CFBundleExecutable"))
    if sh("pgrep", "-f", exe):
        raise RuntimeError(f"{name} is running: quit it first ({exe})")
    argv, env = launch_args(name, exe, a, scratch)
    before = webkit()
    t0 = time.clock_gettime_ns(time.CLOCK_UPTIME_RAW)
    p = subprocess.Popen(argv, env=env, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    r = {}
    try:
        shown = sh(winwait, str(p.pid), "180")
        if not shown:
            raise RuntimeError(f"{name}: no window within 180 s")
        r["launch"] = (int(shown) - t0) / 1e6
        if not prompts:
            return r
        if name == "Qurry GUI":
            pids = lambda: tree(p.pid) + sorted(webkit() - before)
        else:
            pids = lambda: tree(p.pid)
        print(f"    {name}: window after {r['launch']:,.0f} ms")
        ask(f"1/3  Connect to the {SOURCE} data source (expand it until its schemas show), then press Enter here.")
        print(f"    idle {IDLE_S} s …", flush=True)
        time.sleep(IDLE_S)
        r["idle"] = footprint(pids())[0]
        r["q1k"] = ask_ms(f"2/3  In a query console run\n         {Q1K}\n    and scroll to the last row. The time the app reports for the query")
        r["mem1k"] = footprint(pids())[0]
        r["q1m"] = ask_ms(f"3/3  In the same console run\n         {Q1M}\n    The time the app reports for it (its first page)")
        time.sleep(2)
        r["peak"] = footprint(pids())[1]
        return {k: v for k, v in r.items() if v is not None}
    finally:
        p.terminate()
        try:
            p.wait(30)
        except subprocess.TimeoutExpired:
            p.kill()
            p.wait()


def gui(a):
    scratch = tempfile.mkdtemp(prefix="qurry-bench-gui-")
    winwait = str(Path(a.bin) / "winwait")
    cols, notes = {}, []
    try:
        for name, app in gui_apps(a):
            if not app:
                notes.append(f"{name}: not installed, skipped")
                continue
            version = bundle(app, "CFBundleShortVersionString")
            print(f"\n{name} {version} ({app}): one launch to warm up, then {a.runs} runs", file=sys.stderr)
            gui_run(name, app, a, scratch, winwait, prompts=False)
            out, loads = {}, []
            for i in range(a.runs):
                print(f"  run {i + 1} of {a.runs}", file=sys.stderr)
                loads.append(load())
                for k, v in gui_run(name, app, a, scratch, winwait, prompts=True).items():
                    out.setdefault(k, []).append(v)
            cols[f"{name} {version}"] = out
            notes.append(f"{name}: load average {min(loads):.1f} – {max(loads):.1f}")
    finally:
        shutil.rmtree(scratch, ignore_errors=True)
    lines = []
    if cols:
        cell = lambda xs, fmt: f"**{fmt(statistics.median(xs))}** ({fmt(min(xs))} – {fmt(max(xs))})" if xs else "—"
        rows = [
            ("Launch → first window", "launch", ms),
            ("Idle memory, one PostgreSQL source connected", "idle", mb),
            ("1,000-row query: time the app reports", "q1k", ms),
            ("Memory with the 1,000 rows loaded", "mem1k", mb),
            ("1M-row query: time the app reports (first page)", "q1m", ms),
            ("Peak memory (sum of the processes' peaks)", "peak", mb),
        ]
        lines += ["| Measurement | " + " | ".join(cols) + " |", "|---" * (len(cols) + 1) + "|"]
        lines += [f"| {what} | " + " | ".join(cell(c.get(k, []), fmt) for c in cols.values()) + " |" for what, k, fmt in rows]
    return lines + [""] + [f"- {n}" for n in notes]


# ---------- output ----------
def ms(v):
    return f"{v:,.0f} ms" if v >= 100 else f"{v:.1f} ms"


def mb(v):
    return f"{v / 1048576:,.0f} MB" if v >= 100 * 1048576 else f"{v / 1048576:.1f} MB"


def series(name, fn, runs):
    """fn() → dict of measurements; one warm-up run (page cache, the server's buffers), then `runs` runs → {key: [values]}, loads"""
    print(f"  {name}: warm-up", end="", flush=True, file=sys.stderr)
    fn()
    out, loads = {}, []
    for i in range(runs):
        print(f", {i + 1}", end="", flush=True, file=sys.stderr)
        loads.append(load())
        for k, v in fn().items():
            out.setdefault(k, []).append(v)
    print(file=sys.stderr)
    return out, loads


def row(what, xs, fmt, loads):
    return f"| {what} | **{fmt(statistics.median(xs))}** | {fmt(min(xs))} – {fmt(max(xs))} | {min(loads):.1f} – {max(loads):.1f} |"


def headless(a):
    b = Path(a.bin)
    url = f"postgres://postgres@127.0.0.1:{a.port}/shop"
    print("Measuring (headless):", file=sys.stderr)
    t, tl = series("TUI", lambda: tui_run(str(b / "qurry-tui"), url), a.runs)
    m, ml = series("MCP", lambda: mcp_run(str(b / "qurry-mcp"), a.config), a.runs)
    c, cl = series("core", lambda: core_run(str(b / "fetch_bench"), a.config), a.runs)
    return [
        "| Measurement | Median | Min – max | Load average |",
        "|---|---|---|---|",
        row("TUI: launch → first frame", t["launch"], ms, tl),
        row("TUI: idle memory, one PostgreSQL source connected", t["idle"], mb, tl),
        row("TUI: 1,000-row query end to end (run → all rows in the grid)", t["q1k"], ms, tl),
        row("TUI: 1M-row query → first 500 rows on screen", t["q1m_first"], ms, tl),
        row("TUI: 1M-row query → grid filled to its 100,000-row cap", t["q1m_cap"], ms, tl),
        row("TUI: peak memory (whole run, 100,000 rows loaded)", t["peak"], mb, tl),
        row("MCP: spawn → initialize reply", m["start"], ms, ml),
        row("MCP: idle memory", m["idle"], mb, ml),
        row("MCP: `query` 1,000 rows, first call (connects)", m["query_cold"], ms, ml),
        row("MCP: `query` 1,000 rows, warm", m["query_warm"], ms, ml),
        row("MCP: peak memory", m["peak"], mb, ml),
        row("core: 1M rows → first page (500 rows)", c["first"], ms, cl),
        row("core: 1M rows → all fetched (500 a page, kept in memory)", c["all"], ms, cl),
        row("core: 1M rows, peak memory", c["peak"], mb, cl),
    ]


def environment(a, gui_only):
    gb = int(sh("sysctl", "-n", "hw.memsize")) / 2**30
    pg = sh("docker", "exec", "qurry-demo-bench", "postgres", "--version").replace("postgres (PostgreSQL) ", "")
    root = Path(__file__).resolve().parents[2]
    commit = sh("git", "-C", str(root), "describe", "--always", "--dirty", "--exclude", "*")
    return [
        f"- Date: {time.strftime('%Y-%m-%d %H:%M %Z')}",
        f"- Machine: {sh('sysctl', '-n', 'hw.model')}, {sh('sysctl', '-n', 'machdep.cpu.brand_string')}, {sh('sysctl', '-n', 'hw.ncpu')} cores, {gb:.0f} GB; macOS {sh('sw_vers', '-productVersion')}",
        f"- Qurry: the apps' versions head the columns" if gui_only else f"- Qurry: {sh(str(Path(a.bin) / 'qurry-tui'), '--version')} built from commit {commit} (release profile)",
        f"- Database: PostgreSQL {pg} in Docker {sh('docker', 'version', '--format', '{{.Server.Version}}')}, `shop.orders` 1,000,000 rows (scripts/bench/setup.sql)",
        f"- Median of {a.runs} runs after one warm-up run; load average = the 1-minute load when each run started (min – max)",
    ]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", required=True)
    ap.add_argument("--config", required=True, help="scratch config folder with the acme-bench data source")
    ap.add_argument("--bin", required=True, help="folder with qurry-tui, qurry-mcp and fetch_bench (headless) or winwait (--gui, --compare)")
    ap.add_argument("--runs", type=int, default=5)
    ap.add_argument("--gui", action="store_true")
    ap.add_argument("--compare", action="store_true")
    a = ap.parse_args()
    gui_only = a.gui or a.compare
    print("\n".join(environment(a, gui_only) + [""] + (gui(a) if gui_only else headless(a))))


if __name__ == "__main__":
    main()
