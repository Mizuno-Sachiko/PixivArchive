#!/usr/bin/env bash
set -euo pipefail

migrate=0
if [ "${1:-}" = "--migrate" ]; then
  migrate=1
  shift
fi

if [ "${1:-}" != "--" ]; then
  echo "usage: scripts/test-db.sh [--migrate] -- <command>" >&2
  exit 2
fi
shift

if [ "$#" -eq 0 ]; then
  echo "test-db.sh requires a child command" >&2
  exit 2
fi

role="pixivarchive_test"
password="pixivarchive_test"
database="pixivarchive_test_$(date +%Y%m%d%H%M%S)_$$_$(od -An -N4 -tx4 /dev/urandom | tr -d ' \n')"
runuser_bin="/usr/sbin/runuser"

detect_pg_port() {
  if [ -n "${PGPORT:-}" ]; then
    printf '%s\n' "$PGPORT"
    return
  fi

  if command -v pg_lsclusters >/dev/null 2>&1; then
    pg_lsclusters --no-header 2>/dev/null | awk '$4 == "online" { print $3; exit }'
    return
  fi

  printf '%s\n' "5432"
}

validate_pg_port() {
  local port="$1"
  if ! [[ "$port" =~ ^[0-9]+$ ]] || [ "$port" -lt 1 ] || [ "$port" -gt 65535 ]; then
    echo "PostgreSQL port must be an integer from 1 to 65535" >&2
    exit 2
  fi
}

run_createdb() {
  if [ -n "${TEST_DB_LOG:-}" ]; then
    createdb -h 127.0.0.1 -p "$pg_port" -O "$role" "$database"
  elif [ "$(id -u)" -eq 0 ] && id postgres >/dev/null 2>&1; then
    "$runuser_bin" -u postgres -- createdb -p "$pg_port" -O "$role" "$database"
  else
    createdb -h 127.0.0.1 -p "$pg_port" -U postgres -O "$role" "$database"
  fi
}

run_dropdb() {
  if [ -n "${TEST_DB_LOG:-}" ]; then
    dropdb -h 127.0.0.1 -p "$pg_port" --if-exists "$database"
  elif [ "$(id -u)" -eq 0 ] && id postgres >/dev/null 2>&1; then
    "$runuser_bin" -u postgres -- dropdb -p "$pg_port" --if-exists "$database"
  else
    dropdb -h 127.0.0.1 -p "$pg_port" -U postgres --if-exists "$database"
  fi
}

cleanup() {
  status=$?
  if [ "${database_created:-0}" -eq 1 ]; then
    run_dropdb || true
  fi
  exit "$status"
}

pg_port="$(detect_pg_port)"
if [ -z "$pg_port" ]; then
  pg_port="5432"
fi
validate_pg_port "$pg_port"

if ! pg_isready -h 127.0.0.1 -p "$pg_port" -q >/dev/null 2>&1; then
  service postgresql start >/dev/null 2>&1 || true
fi

if ! pg_isready -h 127.0.0.1 -p "$pg_port" -q >/dev/null 2>&1; then
  echo "PostgreSQL is not accepting local connections" >&2
  exit 1
fi

run_createdb
database_created=1
trap cleanup EXIT

export DATABASE_URL="postgres://${role}:${password}@127.0.0.1:${pg_port}/${database}"
export PIXIVARCHIVE_TEST_DATABASE_URL="$DATABASE_URL"

if [ "$migrate" -eq 1 ]; then
  cargo sqlx migrate run --source migrations
fi

"$@"
