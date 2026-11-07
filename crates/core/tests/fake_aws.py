#!/usr/bin/env python3
import json, os, socket, subprocess, sys, threading, time
MODE = "@MODE@"
HERE = os.path.dirname(os.path.abspath(__file__))
def dead():
    # The session died while the plugin runs on (sleep, a network change, ended on the AWS side): it accepts, nothing goes through
    return os.path.exists(os.path.join(HERE, f"dead-{os.getpid()}"))
def stalled(started):
    # The connections open before hold-<pid> was written stall until it is removed, new ones go through (a slow session)
    try:
        with open(os.path.join(HERE, f"hold-{os.getpid()}")) as f:
            return started < float(f.read())
    except (OSError, ValueError):
        return False
def pipe(a, b, started):
    try:
        while True:
            d = a.recv(65536)
            if not d:
                break
            while stalled(started):
                time.sleep(0.05)
            if not dead():
                b.sendall(d)
    except OSError:
        pass
    for s in (a, b):
        try:
            s.shutdown(socket.SHUT_RDWR)
        except OSError:
            pass
if sys.argv[1:2] == ["--plugin"]:
    host, port, local = sys.argv[2], int(sys.argv[3]), int(sys.argv[4])
    srv = socket.socket()
    srv.bind(("127.0.0.1", local))
    srv.listen(16)
    if MODE != "silent":
        print(f"Port {local} opened for sessionId qurry-test-0123456789abcdef.", flush=True)
    print("Waiting for connections...", flush=True)
    held = []
    while True:
        c, _ = srv.accept()
        if dead():
            held.append(c)
            continue
        print("\nConnection accepted for session [qurry-test-0123456789abcdef]", flush=True)
        try:
            u = socket.create_connection((host, port))
        except OSError:
            print("Connection to destination port failed, check SSM Agent logs.", flush=True)
            c.close()
            continue
        started = time.time()
        threading.Thread(target=pipe, args=(c, u, started), daemon=True).start()
        threading.Thread(target=pipe, args=(u, c, started), daemon=True).start()
args = sys.argv[1:]
if args[:2] == ["ssm", "terminate-session"]:
    with open(os.path.join(HERE, "terminated"), "a") as f:
        f.write(json.dumps(args) + "\n")
    if os.path.exists(os.path.join(HERE, "terminate-hangs")):
        with open(os.path.join(HERE, "terminate-pid"), "w") as f:
            f.write(f"{os.getpid()}")
        time.sleep(3600)
    with open(os.path.join(HERE, "terminate-pid"), "w") as f:
        f.write(f"{os.getpid()}")
    print(json.dumps({"SessionId": args[3]}))
    sys.exit(0)
with open(os.path.join(HERE, "pids"), "w") as f:
    f.write(f"{os.getpid()}")
if MODE == "onpath":
    import shutil
    if not shutil.which("session-manager-plugin"):
        MODE = "plugin"
    else:
        MODE = "ok"
with open(os.path.join(HERE, "argv.json"), "w") as f:
    json.dump(args, f)
params = dict(kv.split("=", 1) for kv in args[args.index("--parameters") + 1].split(","))
if MODE in ("ok", "silent"):
    with open(os.path.join(HERE, "starts"), "a") as f:
        f.write(f"{os.getpid()}\n")
    print("\nStarting session with SessionId: qurry-test-0123456789abcdef", flush=True)
    p = subprocess.Popen([sys.executable, os.path.abspath(__file__), "--plugin", params["host"], params["portNumber"], params["localPortNumber"]])
    with open(os.path.join(HERE, "pids"), "w") as f:
        f.write(f"{os.getpid()} {p.pid}")
    sys.exit(p.wait())
if MODE == "stuck":
    print("\nStarting session with SessionId: qurry-test-0123456789abcdef", flush=True)
if MODE in ("hang", "stuck"):
    time.sleep(3600)
if MODE == "plugin":
    sys.stderr.write("\nSessionManagerPlugin is not found. Please refer to SessionManager Documentation here: http://docs.aws.amazon.com/console/systems-manager/session-manager-plugin-not-found\n")
elif MODE == "sso":
    sys.stderr.write("\nError when retrieving token from sso: Token has expired and refresh failed\n")
elif MODE == "early":
    sys.stderr.write("\nAn error occurred (TargetNotConnected) when calling the StartSession operation: i-0123456789abcdef0 is not connected.\n")
sys.exit(255)
