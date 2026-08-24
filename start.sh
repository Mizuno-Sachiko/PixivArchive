#!/usr/bin/env bash
set -euo pipefail

usage() {
  echo "usage: start.sh"
}

if [ "$#" -gt 0 ]; then
  if [ "$#" -eq 1 ] && { [ "$1" = "-h" ] || [ "$1" = "--help" ]; }; then
    usage
    exit 0
  fi
  usage >&2
  exit 2
fi

install_root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
env_file="$install_root/.env"
runtime_dir="$install_root/.runtime"

printf 'PixivArchive: checking configuration\n'

if [ ! -f "$env_file" ]; then
  echo ".env is missing; copy .env.example and configure it first" >&2
  exit 1
fi

set -a
# shellcheck disable=SC1090
. "$env_file"
set +a

if [ -z "${DATABASE_URL:-}" ]; then
  echo "DATABASE_URL is required for native deployment" >&2
  exit 1
fi
if [ -z "${PIXIVARCHIVE_ADMIN_PASSWORD:-}" ]; then
  echo "PIXIVARCHIVE_ADMIN_PASSWORD is required" >&2
  exit 1
fi
if [ -z "${PIXIVARCHIVE_MEDIA_ROOT:-}" ]; then
  echo "PIXIVARCHIVE_MEDIA_ROOT is required" >&2
  exit 1
fi
if ! command -v curl >/dev/null 2>&1; then
  echo "curl is required for native startup readiness checks" >&2
  exit 1
fi

export PIXIVARCHIVE_STATIC_ROOT="$install_root/frontend"
export PIXIVARCHIVE_WEB_BIND="${PIXIVARCHIVE_WEB_BIND:-0.0.0.0:7088}"
export RUST_LOG="${RUST_LOG:-pixivarchive=info,tower_http=info}"

umask 077
mkdir -p "$runtime_dir"

process_matches() {
  local pid="$1"
  local executable="$2"
  local expected
  expected="$(readlink -f "$executable")"
  if [ "$(readlink -f "/proc/$pid/exe" 2>/dev/null || true)" = "$expected" ]; then
    return 0
  fi
  tr '\0' '\n' <"/proc/$pid/cmdline" 2>/dev/null | grep -Fxq -- "$expected"
}

running_pid() {
  local pid_file="$1"
  local executable="$2"
  [ -f "$pid_file" ] || return 1
  local pid
  pid="$(cat "$pid_file")"
  [[ "$pid" =~ ^[0-9]+$ ]] || return 2
  kill -0 "$pid" 2>/dev/null || return 1
  process_matches "$pid" "$executable" || return 2
}

for service in web worker; do
  executable="$install_root/bin/pixivarchive-$service"
  if running_pid "$runtime_dir/$service.pid" "$executable"; then
    echo "PixivArchive $service is already running" >&2
    exit 1
  else
    status=$?
  fi
  if [ "$status" -eq 2 ]; then
    echo "PID file does not belong to PixivArchive $service: $runtime_dir/$service.pid" >&2
    exit 1
  fi
  rm -f "$runtime_dir/$service.pid"
done

readiness_base_url() {
  local bind="$1"
  local host port
  if [[ "$bind" =~ ^\[([^]]+)\]:([0-9]+)$ ]]; then
    host="${BASH_REMATCH[1]}"
    port="${BASH_REMATCH[2]}"
  elif [[ "$bind" =~ ^([^:]+):([0-9]+)$ ]]; then
    host="${BASH_REMATCH[1]}"
    port="${BASH_REMATCH[2]}"
  else
    echo "PIXIVARCHIVE_WEB_BIND must contain a host and port" >&2
    return 1
  fi

  case "$host" in
    0.0.0.0) host="127.0.0.1" ;;
    ::) host="::1" ;;
  esac
  if [[ "$host" == *:* ]]; then
    host="[$host]"
  fi
  printf 'http://%s:%s' "$host" "$port"
}

base_url="$(readiness_base_url "$PIXIVARCHIVE_WEB_BIND")"
readiness_url="$base_url/health/ready"

printf 'PixivArchive: preparing database and installation\n'
"$install_root/bin/pixivarchive-admin" prepare

started_pid_files=()
stop_started_processes() {
  local pid_file pid
  for pid_file in "${started_pid_files[@]}"; do
    [ -f "$pid_file" ] || continue
    pid="$(cat "$pid_file")"
    if [[ "$pid" =~ ^[0-9]+$ ]] && kill -0 "$pid" 2>/dev/null; then
      kill -TERM "$pid" 2>/dev/null || true
    fi
    rm -f "$pid_file"
  done
}
trap stop_started_processes ERR

printf 'PixivArchive: starting Web\n'
nohup "$install_root/bin/pixivarchive-web" >>"$runtime_dir/web.log" 2>&1 &
web_pid=$!
printf '%s\n' "$web_pid" >"$runtime_dir/web.pid"
started_pid_files+=("$runtime_dir/web.pid")
kill -0 "$web_pid"

printf 'PixivArchive: starting Worker\n'
nohup "$install_root/bin/pixivarchive-worker" >>"$runtime_dir/worker.log" 2>&1 &
worker_pid=$!
printf '%s\n' "$worker_pid" >"$runtime_dir/worker.pid"
started_pid_files+=("$runtime_dir/worker.pid")
kill -0 "$worker_pid"

sleep 0.2
for service in web worker; do
  pid_file="$runtime_dir/$service.pid"
  executable="$install_root/bin/pixivarchive-$service"
  if ! running_pid "$pid_file" "$executable"; then
    echo "PixivArchive $service exited during startup" >&2
    false
  fi
done

printf 'PixivArchive: waiting for Web readiness\n'
ready=0
for _ in $(seq 1 300); do
  if ! running_pid "$runtime_dir/web.pid" "$install_root/bin/pixivarchive-web" \
    || ! running_pid "$runtime_dir/worker.pid" "$install_root/bin/pixivarchive-worker"; then
    break
  fi
  if curl --noproxy '*' --silent --output /dev/null --fail "$readiness_url"; then
    ready=1
    break
  fi
  sleep 0.1
done
if [ "$ready" -ne 1 ]; then
  echo "PixivArchive: readiness check failed at $readiness_url" >&2
  false
fi

trap - ERR
printf 'PixivArchive: Web started (PID %s)\n' "$web_pid"
printf 'PixivArchive: Worker started (PID %s)\n' "$worker_pid"
printf 'PixivArchive: ready at %s\n' "$base_url"
