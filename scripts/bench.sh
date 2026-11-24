#!/usr/bin/env bash
# Reproducible benchmarks (BENCHMARKS.md): a PostgreSQL 16 container with 1,000,000 made-up shop orders, then the median of
# 5 runs of each measurement, printed as markdown with the date, versions and hardware. macOS; needs Docker and python3.
#   scripts/bench.sh               headless, nothing on screen: TUI (in a pty), MCP server (stdio), core (1M-row fetch)
#   scripts/bench.sh --gui         Qurry GUI instead: OPENS ITS WINDOW and asks you to connect and run two queries
#   scripts/bench.sh --compare     DataGrip and DBeaver the same way (a missing app is skipped with a note); OPENS WINDOWS
#   --runs N                       runs per measurement (default 5)
# The container is qurry-demo-bench on 127.0.0.1:$BENCH_PORT (default 54329), removed with its volume on exit (ctrl-c too).
# Builds go to $CARGO_TARGET_DIR (default target/). GUI apps: $QURRY_APP, $DATAGRIP_APP, $DBEAVER_APP override the paths.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

usage() { sed -n '2,9p' "$0" | sed 's/^# \{0,1\}//' >&2; exit 2; }
runs=5 gui=()
while [ $# -gt 0 ]; do
  case $1 in
    --gui | --compare) gui+=("$1") ;;
    --runs) [ $# -ge 2 ] || usage; runs=$2; shift ;;
    -h | --help) usage ;;
    *) usage ;;
  esac
  shift
done

port=${BENCH_PORT:-54329}
ctr=qurry-demo-bench
[ "$(uname)" = Darwin ] || { echo "macOS only (footprint, sysctl, CoreGraphics)" >&2; exit 1; }
for tool in docker python3 cargo; do command -v "$tool" >/dev/null || { echo "needs $tool" >&2; exit 1; }; done
case $port in 15432 | 25432) echo "port $port is reserved on this machine: set BENCH_PORT" >&2; exit 1 ;; esac
if lsof -nP -iTCP:"$port" -sTCP:LISTEN >/dev/null 2>&1; then
  echo "port $port is in use: set BENCH_PORT to a free one" >&2
  exit 1
fi
if [ ${#gui[@]} -gt 0 ]; then
  [ -t 0 ] || { echo "--gui / --compare ask questions: run them in a terminal" >&2; exit 1; }
  cat >&2 <<'EOF'

  WARNING: --gui / --compare OPEN APP WINDOWS on this screen and ask you to connect and run two queries in each, 5 times.
  Quit Qurry, DataGrip and DBeaver first. Qurry runs with a scratch home folder (your data sources and tabs aren't touched);
  DataGrip uses its own settings (its license) and opens a scratch project with the acme-bench data source.

EOF
  read -r -p "  Press Enter to start, ctrl-c to stop. " _
fi

tmp=$(mktemp -d)
cleanup() {
  docker rm -f -v "$ctr" >/dev/null 2>&1 || true
  rm -rf "$tmp"
}
trap cleanup EXIT
trap 'exit 130' INT TERM

# Binaries: copies, since another checkout sharing CARGO_TARGET_DIR may relink them meanwhile
mkdir -p "$tmp/bin"
if [ ${#gui[@]} -eq 0 ]; then
  echo "Building (release) …" >&2
  target=${CARGO_TARGET_DIR:-$root/target}
  cargo build --release --quiet -p qurry-tui -p qurry-mcp
  cargo build --release --quiet -p qurry-core --example fetch_bench
  cp "$target/release/qurry-tui" "$target/release/qurry-mcp" "$target/release/examples/fetch_bench" "$tmp/bin/"
else
  swiftc -O scripts/bench/winwait.swift -o "$tmp/bin/winwait"
fi

echo "Starting $ctr on 127.0.0.1:$port and loading 1,000,000 rows …" >&2
docker rm -f -v "$ctr" >/dev/null 2>&1 || true
docker run -d --name "$ctr" -e POSTGRES_HOST_AUTH_METHOD=trust -p "127.0.0.1:$port:5432" postgres:16 >/dev/null
# During initdb the server listens on its socket only: TCP answers once it is really up
for _ in $(seq 1 120); do
  docker exec "$ctr" pg_isready -h 127.0.0.1 -U postgres -q 2>/dev/null && break
  sleep 0.5
done
docker exec "$ctr" createdb -h 127.0.0.1 -U postgres shop
docker exec -i "$ctr" psql -h 127.0.0.1 -U postgres -d shop -q -v ON_ERROR_STOP=1 -1 <scripts/bench/setup.sql

# The scratch config folder: one data source, visible to the MCP server (Read)
mkdir -p "$tmp/config"
cat >"$tmp/config/connections.json" <<EOF
[{"id": "acme-bench", "name": "acme-bench", "kind": "postgres", "host": "127.0.0.1", "port": $port, "user": "postgres", "database": "shop", "options": {"mcp": "read"}}]
EOF

python3 scripts/bench/bench.py --port "$port" --config "$tmp/config" --bin "$tmp/bin" --runs "$runs" ${gui[@]+"${gui[@]}"}
