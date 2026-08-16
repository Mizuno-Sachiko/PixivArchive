#!/usr/bin/env bash
set -euo pipefail

if [ "${1:-}" = "--check" ]; then
  exec bash scripts/test-db.sh --migrate -- cargo sqlx prepare --check --workspace -- --all-targets
fi

if [ "$#" -ne 0 ]; then
  echo "usage: scripts/prepare-sqlx.sh [--check]" >&2
  exit 2
fi

exec bash scripts/test-db.sh --migrate -- cargo sqlx prepare --workspace -- --all-targets
