#!/usr/bin/env bash
set -euo pipefail

install_root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
env_file="$install_root/.env"
runtime_dir="$install_root/.runtime"

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

nohup "$install_root/bin/pixivarchive-web" >>"$runtime_dir/web.log" 2>&1 &
web_pid=$!
printf '%s\n' "$web_pid" >"$runtime_dir/web.pid"
started_pid_files+=("$runtime_dir/web.pid")
kill -0 "$web_pid"

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

trap - ERR
printf 'PixivArchive Web started with PID %s\n' "$web_pid"
printf 'PixivArchive Worker started with PID %s\n' "$worker_pid"
