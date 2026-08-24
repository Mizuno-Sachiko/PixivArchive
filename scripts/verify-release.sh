#!/usr/bin/env bash
set -euo pipefail

allow_dirty=0
skip_e2e=0
while [ "$#" -gt 0 ]; do
  case "$1" in
    --allow-dirty)
      allow_dirty=1
      ;;
    --skip-e2e)
      skip_e2e=1
      ;;
    *)
      echo "usage: scripts/verify-release.sh [--allow-dirty] [--skip-e2e]" >&2
      exit 2
      ;;
  esac
  shift
done

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
if [ ! -f Cargo.toml ] || [ ! -f frontend/package.json ] || [ ! -f README.md ]; then
  echo "verify-release.sh must run from a PixivArchive source tree" >&2
  exit 1
fi

source_dirty=0
if [ -n "$(git status --porcelain --untracked-files=normal)" ]; then
  source_dirty=1
fi
if [ "$source_dirty" -eq 1 ] && [ "$allow_dirty" -ne 1 ]; then
  echo "the release source tree is dirty; commit it or pass --allow-dirty for local verification" >&2
  exit 1
fi

pnpm --dir frontend install --frozen-lockfile

cargo fmt --all -- --check
bats scripts/tests/test-db.bats
bash scripts/prepare-sqlx.sh --check
bash scripts/test-db.sh --migrate -- \
  env SQLX_OFFLINE=true cargo test --workspace --all-targets
env SQLX_OFFLINE=true cargo clippy --workspace --all-targets -- -D warnings

bash scripts/check-generated.sh
node scripts/check-doc-links.mjs

pnpm --dir frontend format:check
pnpm --dir frontend lint
pnpm --dir frontend check
pnpm --dir frontend test
rm -rf -- frontend/.svelte-kit frontend/build
pnpm --dir frontend build
if [ "$skip_e2e" -ne 1 ]; then
  pnpm --dir frontend test:e2e:run
fi

bats scripts/tests/deploy.bats

build_args=(--reuse-frontend-build)
if [ "$source_dirty" -eq 1 ]; then
  build_args+=(--allow-dirty)
fi
bash scripts/build-release.sh "${build_args[@]}"

archive="dist/pixivarchive-linux-x86_64.tar.gz"
checksum="${archive}.sha256"
(
  cd dist
  sha256sum -c "$(basename "$checksum")"
)

archive_entries="$(tar -tzf "$archive")"
for required in \
  pixivarchive/bin/pixivarchive-web \
  pixivarchive/bin/pixivarchive-worker \
  pixivarchive/bin/pixivarchive-admin \
  pixivarchive/frontend/200.html \
  pixivarchive/.env.example \
  pixivarchive/LICENSE \
  pixivarchive/start.sh \
  pixivarchive/stop.sh \
  pixivarchive/upgrade.sh \
  pixivarchive/README.md \
  pixivarchive/SOURCE_STATE; do
  if ! grep -Fxq "$required" <<<"$archive_entries"; then
    echo "release archive is missing $required" >&2
    exit 1
  fi
done

if grep -Eq '^pixivarchive/migrations(/|$)' <<<"$archive_entries"; then
  echo "release archive contains migration source files" >&2
  exit 1
fi

if grep -Eq '^pixivarchive/(Dockerfile|compose\.build\.yaml)$' <<<"$archive_entries"; then
  echo "release archive contains source-build deployment files" >&2
  exit 1
fi

if grep -Eq '^pixivarchive/(compose\.yaml|docs|assets)(/|$)' <<<"$archive_entries"; then
  echo "release archive contains deployment or documentation sources" >&2
  exit 1
fi

printf 'PixivArchive release verification passed.\n'
