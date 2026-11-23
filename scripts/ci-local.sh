#!/usr/bin/env bash
# Runs the jobs of .github/workflows/ci.yml on this machine, against Docker containers it starts itself (qurry-demo-ci-<service>,
# on free 127.0.0.1 ports) and removes on exit, ctrl-c included.
#   scripts/ci-local.sh                         rust, compat, web, then e2e on chromium and on webkit
#   scripts/ci-local.sh rust|compat|web         that job only
#   scripts/ci-local.sh e2e [chromium|webkit]   Playwright, one browser (chromium when none is given), one worker
# Needs Docker (rust, compat) and pnpm (web, e2e); e2e installs its Playwright browser (as ci.yml does) when this Playwright version has none.
# The step commands (incl. the --skip lists) are read from ci.yml by step name; the containers, the test env and the e2e command are
# copied from it: ci.yml and this script change together. Builds go to $CARGO_TARGET_DIR (default target/), with rust-toolchain.toml's toolchain.
# shellcheck disable=SC2317 # on_exit and the readiness checks are called through trap or passed as commands
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
ci=.github/workflows/ci.yml

usage() { echo "usage: scripts/ci-local.sh [rust|compat|web|e2e [chromium|webkit]]" >&2; exit 2; }
case "${1:-all}" in
  all) [ $# -le 1 ] || usage; jobs="rust compat web e2e:chromium e2e:webkit" ;;
  rust | compat | web) [ $# -eq 1 ] || usage; jobs=$1 ;;
  e2e)
    [ $# -le 2 ] || usage
    case "${2:-chromium}" in chromium | webkit) jobs="e2e:${2:-chromium}" ;; *) usage ;; esac
    ;;
  *) usage ;;
esac

export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$root/target}"
# ci.yml sets this for the rust and compat jobs: test executables live too deep for the lookup next to the executable
export QURRY_DRIVERS_DIR="$CARGO_TARGET_DIR/debug"
# Only the servers started here: a QURRY_TEST_* from the calling shell would point the tests elsewhere
for v in $(compgen -e | grep '^QURRY_TEST_' || true); do unset "$v"; done

ran=() skipped=() failed=() ctrs=()

summary() {
  echo
  echo "==> ci-local summary"
  local l
  for l in ${ran[@]+"${ran[@]}"}; do echo "  ran      $l"; done
  for l in ${skipped[@]+"${skipped[@]}"}; do echo "  skipped  $l"; done
  for l in ${failed[@]+"${failed[@]}"}; do echo "  FAILED   $l"; done
}

remove_ctrs() {
  [ ${#ctrs[@]} -gt 0 ] || return 0
  # -v: the databases' anonymous data volumes go with them (without it every run left one per container behind)
  docker rm -f -v "${ctrs[@]}" >/dev/null 2>&1 || true
  ctrs=()
}

on_exit() {
  local status=$?
  trap - EXIT INT TERM
  remove_ctrs
  summary
  exit "$status"
}
trap on_exit EXIT
trap 'echo; echo "ci-local: interrupted" >&2; exit 130' INT
trap 'exit 143' TERM

# The run: of a step in a job of ci.yml: `step_run <job> <step name>`; with an empty name, every unnamed `- run:` of the job
step_run() {
  awk -v job="$1" -v name="$2" '
    /^  [A-Za-z0-9_-]+:/ { injob = ($0 == "  " job ":"); want = 0; block = 0; next }
    !injob { next }
    block {
      if ($0 ~ /^          / || $0 ~ /^ *$/) { print substr($0, 11); next }
      block = 0; want = 0
    }
    /^      - / {
      want = 0
      if (name == "" && $0 ~ /^      - run: /) { print substr($0, 14); next }
      if (name != "" && $0 == "      - name: " name) want = 1
      next
    }
    want && /^        run: / {
      r = substr($0, 14)
      if (r == "|") block = 1; else { print r; want = 0 }
    }
  ' "$ci"
}

# Runs step `<name>` of job `<job>` from ci.yml like GitHub does (bash -eo pipefail), with extra VAR=value pairs after the name.
# Builds go to $CARGO_TARGET_DIR: the step's `--target-dir target` and target/debug/ paths are rewritten to it
step() {
  local job=$1 name=$2 script
  shift 2
  script=$(step_run "$job" "$name")
  [ -n "$script" ] || { echo "ci-local: no step '$name' in job $job of $ci: update scripts/ci-local.sh with ci.yml" >&2; exit 1; }
  # shellcheck disable=SC2016 # $CARGO_TARGET_DIR is expanded by the step's shell
  script=$(printf '%s\n' "$script" | sed -e 's#--target-dir target#--target-dir "$CARGO_TARGET_DIR"#g' -e 's#\([ "]\)target/debug/#\1$CARGO_TARGET_DIR/debug/#g')
  run "$job: $name" env ${1+"$@"} bash --noprofile --norc -eo pipefail -c "$script"
}

# `run <label> <command...>`: runs it and records the result
run() {
  local label=$1
  shift
  echo
  echo "==> $label"
  if "$@"; then
    ran+=("$label")
  else
    failed+=("$label")
    return 1
  fi
}

# Free 127.0.0.1 ports from 26000 up: nothing listening (lsof), never 15432 or 25432
next_port=26000
free_port() {
  local p=$next_port
  while [ "$p" = 15432 ] || [ "$p" = 25432 ] || lsof -nP -iTCP:"$p" -sTCP:LISTEN >/dev/null 2>&1; do p=$((p + 1)); done
  next_port=$((p + 1))
  port=$p
}

# `start <service> <container port> <docker run args...>`: qurry-demo-ci-<service> on a free port, in PORT_<service>.
# A container that doesn't start is reported, and PORT_<service> stays empty (its use_server then fails)
start() {
  local svc=$1 cport=$2 port
  shift 2
  free_port
  echo "==> starting qurry-demo-ci-$svc on 127.0.0.1:$port"
  ctrs+=("qurry-demo-ci-$svc")
  if docker run -d --name "qurry-demo-ci-$svc" -p "127.0.0.1:$port:$cport" "$@" >/dev/null; then
    printf -v "PORT_$svc" '%s' "$port"
  else
    printf -v "PORT_$svc" '%s' ""
    echo "ci-local: qurry-demo-ci-$svc did not start" >&2
  fi
}

# `ready <service> <seconds> <check command...>`: waits until the check passes; `health` checks the container's health status
ready() {
  local svc=$1 secs=$2 port_var="PORT_$1" i
  shift 2
  [ -n "${!port_var}" ] || return 1
  for ((i = 0; i < secs; i += 2)); do
    "$@" && return 0
    [ "$(docker inspect -f '{{.State.Running}}' "qurry-demo-ci-$svc" 2>/dev/null)" = true ] || break
    sleep 2
  done
  docker logs "qurry-demo-ci-$svc" 2>&1 | tail -20 >&2
  return 1
}
health() { [ "$(docker inspect -f '{{.State.Health.Status}}' "qurry-demo-ci-$1")" = healthy ]; }
http_ok() { curl -sf "$@" >/dev/null 2>&1; }

# `use_server <service> <seconds> <ENV> <value with {port}> <check...>`: adds ENV=value to test_env when the server is up, else records
# the failure (its log is above) and returns 1: in ci.yml a service that doesn't come up fails the job, and with ENV unset some tests
# fall back to a default port (the Redis ones to 127.0.0.1:16379, where a contributor's own Redis may run, and FLUSHDB it)
use_server() {
  local svc=$1 secs=$2 var=$3 val=$4 port_var="PORT_$1"
  shift 4
  if ready "$svc" "$secs" "$@"; then
    test_env+=("$var=${val//\{port\}/${!port_var}}")
  else
    failed+=("qurry-demo-ci-$svc: didn't start or come up within ${secs}s")
    return 1
  fi
}

need_docker() {
  docker info >/dev/null 2>&1 || { echo "ci-local: Docker isn't running: start Docker Desktop (or run scripts/ci-local.sh web)" >&2; exit 1; }
}

# The rust job: services as in ci.yml (PostgreSQL 16 with pgvector and pg_stat_statements, MySQL 8.4, Redis 7, ClickHouse 24.8, Mongo 7,
# Qdrant, Chroma, Milvus standalone), started before the build so they come up meanwhile
job_rust() {
  start pg 5432 -e POSTGRES_HOST_AUTH_METHOD=trust --health-cmd "pg_isready -h 127.0.0.1 -U postgres" --health-interval 2s --health-retries 30 \
    pgvector/pgvector:pg16 postgres -c shared_preload_libraries=pg_stat_statements -c pg_stat_statements.track=all
  start mysql 3306 -e MYSQL_ROOT_PASSWORD=dbtool -e MYSQL_DATABASE=shop \
    --health-cmd "mysqladmin ping -h 127.0.0.1 -pdbtool" --health-interval 2s --health-retries 60 mysql:8.4
  start redis 6379 --health-cmd "redis-cli ping" --health-interval 2s --health-retries 30 redis:7-alpine
  start clickhouse 8123 -e CLICKHOUSE_USER=qurry -e CLICKHOUSE_PASSWORD=pw -e CLICKHOUSE_DEFAULT_ACCESS_MANAGEMENT=1 \
    --health-cmd "wget -qO- http://127.0.0.1:8123/ping" --health-interval 2s --health-retries 60 clickhouse/clickhouse-server:24.8
  start mongo 27017 -e MONGO_INITDB_ROOT_USERNAME=qurry -e MONGO_INITDB_ROOT_PASSWORD=pw \
    --health-cmd "mongosh --quiet --eval 'db.runCommand({ping:1})'" --health-interval 2s --health-retries 60 mongo:7
  start qdrant 6333 qdrant/qdrant:v1.12.4
  start chroma 8000 chromadb/chroma:1.0.15
  start milvus 19530 -e ETCD_USE_EMBED=true -e ETCD_DATA_DIR=/var/lib/milvus/etcd -e COMMON_STORAGETYPE=local -e DEPLOY_MODE=STANDALONE \
    milvusdb/milvus:v2.5.4 milvus run standalone

  step rust "qurry-shared stays pure (no tokio, sqlx or redis)" || return 1
  step rust "Build drivers (plugin tests run against them)" || return 1
  step rust "Clippy" || return 1

  # Tests use USER as the PG user
  test_env=(USER=postgres)
  local down=
  use_server pg 120 QURRY_TEST_PG "127.0.0.1:{port}:postgres:postgres" health pg || down=1
  use_server mysql 120 QURRY_TEST_MY "127.0.0.1:{port}:root:dbtool:shop" health mysql || down=1
  use_server redis 60 QURRY_TEST_REDIS "127.0.0.1:{port}" health redis || down=1
  use_server clickhouse 120 QURRY_TEST_CH "127.0.0.1:{port}:qurry:pw:default" health clickhouse || down=1
  use_server mongo 120 QURRY_TEST_MONGO "127.0.0.1:{port}:qurry:pw:qurrytest" health mongo || down=1
  use_server qdrant 60 QURRY_TEST_QDRANT "127.0.0.1:{port}:::qdrant" http_ok "http://127.0.0.1:${PORT_qdrant:-0}/healthz" || down=1
  use_server chroma 60 QURRY_TEST_CHROMA "127.0.0.1:{port}:::default_database" http_ok "http://127.0.0.1:${PORT_chroma:-0}/api/v2/heartbeat" || down=1
  [ -z "$down" ] || return 1
  # Same check and wait as ci.yml's Wait for Milvus, which doesn't fail the job: past it the Milvus tests (plugin and RAG) skip, as they
  # return early without QURRY_TEST_MILVUS
  milvus_up() { curl -sf -XPOST "127.0.0.1:${PORT_milvus:-0}/v2/vectordb/collections/list" -d '{}' 2>/dev/null | grep -q '"code":0'; }
  if ready milvus 180 milvus_up; then
    test_env+=("QURRY_TEST_MILVUS=127.0.0.1:$PORT_milvus:::default")
  else
    skipped+=("qurry-demo-ci-milvus: didn't come up within 180s, so the Milvus tests (plugin and RAG) skip (QURRY_TEST_MILVUS unset)")
  fi

  step rust "Unit tests" || return 1
  step rust "Integration tests (PostgreSQL / MySQL / Redis)" "${test_env[@]}" || return 1
  step rust "Integration tests (TUI, MCP)" "${test_env[@]}" || return 1
}

# The compat job: the older servers of ci.yml (PostgreSQL 12, MySQL 5.7, ClickHouse 22.8, Mongo 5, SQL Server 2017)
job_compat() {
  start pg12 5432 -e POSTGRES_HOST_AUTH_METHOD=trust --health-cmd "pg_isready -U postgres" --health-interval 2s --health-retries 30 postgres:12-alpine
  start mysql57 3306 -e MYSQL_ROOT_PASSWORD=dbtool -e MYSQL_DATABASE=shop \
    --health-cmd "mysqladmin ping -h 127.0.0.1 -pdbtool" --health-interval 2s --health-retries 60 mysql:5.7
  start clickhouse228 8123 -e CLICKHOUSE_USER=qurry -e CLICKHOUSE_PASSWORD=pw -e CLICKHOUSE_DEFAULT_ACCESS_MANAGEMENT=1 \
    --health-cmd "wget -qO- http://127.0.0.1:8123/ping" --health-interval 2s --health-retries 60 clickhouse/clickhouse-server:22.8
  start mongo5 27017 -e MONGO_INITDB_ROOT_USERNAME=qurry -e MONGO_INITDB_ROOT_PASSWORD=pw \
    --health-cmd "mongo --quiet --eval 'db.runCommand({ping:1})'" --health-interval 2s --health-retries 60 mongo:5
  local mssql=1
  case "$(uname -m)" in
    arm64 | aarch64)
      mssql=
      skipped+=("qurry-demo-ci-mssql2017: mcr.microsoft.com/mssql/server:2017-latest has no arm64 build and doesn't run emulated, so the SQL Server tests of compat skip (QURRY_TEST_MSSQL unset)")
      ;;
    *) start mssql2017 1433 -e ACCEPT_EULA=Y -e "MSSQL_SA_PASSWORD=Qurry_pw1!" mcr.microsoft.com/mssql/server:2017-latest ;;
  esac

  step compat "Build drivers" || return 1

  test_env=(USER=postgres)
  local down=
  use_server pg12 120 QURRY_TEST_PG "127.0.0.1:{port}:postgres:postgres" health pg12 || down=1
  use_server mysql57 180 QURRY_TEST_MY "127.0.0.1:{port}:root:dbtool:shop" health mysql57 || down=1
  use_server clickhouse228 120 QURRY_TEST_CH "127.0.0.1:{port}:qurry:pw:default" health clickhouse228 || down=1
  use_server mongo5 120 QURRY_TEST_MONGO "127.0.0.1:{port}:qurry:pw:qurrytest" health mongo5 || down=1
  if [ -n "$mssql" ]; then
    # As ci.yml's Wait for SQL Server: the port answers, then 10 s more for the server to accept logins
    mssql_up() { nc -z 127.0.0.1 "${PORT_mssql2017:-0}" >/dev/null 2>&1 && sleep 10; }
    use_server mssql2017 120 QURRY_TEST_MSSQL "127.0.0.1:{port}:sa:Qurry_pw1!:master" mssql_up || down=1
  fi
  [ -z "$down" ] || return 1

  step compat "Integration tests on older servers" "${test_env[@]}" || return 1
}

# The web job: every unnamed run step of it in ci.yml (pnpm install, test, build, build-storybook)
job_web() {
  local cmd
  while IFS= read -r cmd; do
    run "web: $cmd" bash --noprofile --norc -eo pipefail -c "$cmd" || return 1
  done < <(step_run web "")
}

# One browser of the e2e job, one worker (ci.yml: two shards of two workers each). The WASM first, like ci.yml's wasm action, the
# browser of the installed Playwright version (nothing to do when it's there), and the dev server on a free port (E2E_PORT, playwright.config.ts)
job_e2e() {
  local project=$1 port
  run "e2e $project: pnpm install" pnpm install --frozen-lockfile --prefer-offline || return 1
  run "e2e $project: pnpm wasm" pnpm wasm || return 1
  run "e2e $project: npx playwright install $project" npx playwright install "$project" || return 1
  free_port
  run "e2e $project: npx playwright test --project=$project --workers=1" env E2E_PORT="$port" npx playwright test --project="$project" --workers=1 || return 1
}

case " $jobs " in *" rust "* | *" compat "*) need_docker ;; esac
if [ -n "$(docker ps -aq --filter name=qurry-demo-ci- 2>/dev/null || true)" ]; then
  # Not ours to remove: they belong to another run (one at a time)
  trap - EXIT
  echo "ci-local: a qurry-demo-ci-* container already exists (another run, or one left over): wait for it, or remove it with" >&2
  echo "  docker rm -f -v \$(docker ps -aq --filter name=qurry-demo-ci-)" >&2
  exit 1
fi

status=0
for j in $jobs; do
  case $j in
    rust) job_rust || status=1 ;;
    compat) job_compat || status=1 ;;
    web) job_web || status=1 ;;
    e2e:*) job_e2e "${j#e2e:}" || status=1 ;;
  esac
  remove_ctrs
done
exit "$status"
