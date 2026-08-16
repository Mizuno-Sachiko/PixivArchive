#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
if [ ! -f Cargo.toml ] || [ ! -f frontend/package.json ]; then
  echo "check-generated.sh must run from a PixivArchive source tree" >&2
  exit 1
fi

check_tmp="$(mktemp -d)"
cleanup() {
  status=$?
  trap - EXIT
  rm -rf -- "$check_tmp"
  exit "$status"
}
trap cleanup EXIT

cargo run --quiet -p pixivarchive-admin -- \
  export-openapi "$check_tmp/pixivarchive.json"
if ! cmp -s openapi/pixivarchive.json "$check_tmp/pixivarchive.json"; then
  echo "openapi/pixivarchive.json is stale" >&2
  diff -u openapi/pixivarchive.json "$check_tmp/pixivarchive.json" || true
  exit 1
fi

cargo run --quiet -p pixivarchive-admin -- \
  export-rule-catalog "$check_tmp/rule-catalog.generated.ts"
if ! cmp -s \
  frontend/src/lib/api/rule-catalog.generated.ts \
  "$check_tmp/rule-catalog.generated.ts"; then
  echo "frontend/src/lib/api/rule-catalog.generated.ts is stale" >&2
  diff -u \
    frontend/src/lib/api/rule-catalog.generated.ts \
    "$check_tmp/rule-catalog.generated.ts" || true
  exit 1
fi

pnpm --dir frontend exec openapi-typescript \
  "$check_tmp/pixivarchive.json" \
  -o "$check_tmp/schema.d.ts"
if ! cmp -s frontend/src/lib/api/schema.d.ts "$check_tmp/schema.d.ts"; then
  echo "frontend/src/lib/api/schema.d.ts is stale" >&2
  diff -u frontend/src/lib/api/schema.d.ts "$check_tmp/schema.d.ts" || true
  exit 1
fi
