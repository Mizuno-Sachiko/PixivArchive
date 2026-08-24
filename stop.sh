#!/usr/bin/env bash
set -euo pipefail

usage() {
  echo "usage: stop.sh"
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
runtime_dir="$install_root/.runtime"

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

stop_process() {
  local name="$1"
  local service="$2"
  local pid_file="$runtime_dir/$service.pid"
  local executable="$install_root/bin/pixivarchive-$service"
  if [ ! -f "$pid_file" ]; then
    printf 'PixivArchive: %s is not running\n' "$name"
    return
  fi

  local pid
  pid="$(cat "$pid_file")"
  if ! [[ "$pid" =~ ^[0-9]+$ ]]; then
    echo "invalid PID file: $pid_file" >&2
    return 1
  fi
  if ! kill -0 "$pid" 2>/dev/null; then
    rm -f "$pid_file"
    printf 'PixivArchive: %s is not running\n' "$name"
    return
  fi
  if ! process_matches "$pid" "$executable"; then
    echo "PID file does not belong to PixivArchive $name: $pid_file" >&2
    return 1
  fi

  printf 'PixivArchive: stopping %s (PID %s)\n' "$name" "$pid"
  kill -TERM "$pid"
  local attempt
  for attempt in $(seq 0 300); do
    if ! kill -0 "$pid" 2>/dev/null; then
      rm -f "$pid_file"
      printf 'PixivArchive: %s stopped after %d.%d seconds\n' \
        "$name" "$((attempt / 10))" "$((attempt % 10))"
      return
    fi
    if [ "$attempt" -eq 50 ]; then
      printf 'PixivArchive: %s is still shutting down\n' "$name"
    fi
    if [ "$attempt" -eq 300 ]; then
      break
    fi
    sleep 0.1
  done
  echo "PixivArchive: $name did not stop within 30 seconds" >&2
  return 1
}

stop_process Worker worker
stop_process Web web
printf 'PixivArchive: all processes stopped\n'
