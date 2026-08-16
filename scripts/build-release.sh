#!/usr/bin/env bash
set -euo pipefail

allow_dirty=0
reuse_frontend_build=0
while [ "$#" -gt 0 ]; do
  case "$1" in
    --allow-dirty)
      allow_dirty=1
      ;;
    --reuse-frontend-build)
      reuse_frontend_build=1
      ;;
    *)
      echo "usage: scripts/build-release.sh [--allow-dirty] [--reuse-frontend-build]" >&2
      exit 2
      ;;
  esac
  shift
done

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
if [ ! -f Cargo.toml ] || [ ! -f frontend/package.json ] || [ ! -f README.md ]; then
  echo "build-release.sh must run from a PixivArchive source tree" >&2
  exit 1
fi
if [ "$allow_dirty" -ne 1 ] && [ -n "$(git status --porcelain --untracked-files=normal)" ]; then
  echo "the release source tree is dirty; commit it or pass --allow-dirty for a local artifact" >&2
  exit 1
fi

target="x86_64-unknown-linux-musl"
cargo_target_dir="${CARGO_TARGET_DIR:-$repo_root/target}"
case "$cargo_target_dir" in
  /*) ;;
  *) cargo_target_dir="$repo_root/$cargo_target_dir" ;;
esac
git_commit="$(git rev-parse HEAD)"
release_tmp="$(mktemp -d)"
package_root="$release_tmp/pixivarchive"
cleanup() {
  status=$?
  trap - EXIT
  rm -rf -- "$release_tmp"
  exit "$status"
}
trap cleanup EXIT

if [ "$reuse_frontend_build" -eq 1 ]; then
  if [ ! -f frontend/build/200.html ]; then
    echo "frontend/build is missing; omit --reuse-frontend-build to create it" >&2
    exit 1
  fi
else
  pnpm --dir frontend install --frozen-lockfile
  pnpm --dir frontend build
fi

env \
  SQLX_OFFLINE=true \
  PIXIVARCHIVE_GIT_COMMIT="$git_commit" \
  cargo build --locked --release --target "$target" \
  -p pixivarchive-web \
  -p pixivarchive-worker \
  -p pixivarchive-admin

install -d "$package_root/bin"
for binary in pixivarchive-web pixivarchive-worker pixivarchive-admin; do
  source_binary="$cargo_target_dir/$target/release/$binary"
  if readelf -l "$source_binary" | grep -q 'Requesting program interpreter'; then
    echo "$binary is dynamically linked and is not a portable musl release" >&2
    exit 1
  fi
  install -m 0755 "$source_binary" "$package_root/bin/$binary"
done

cp -a frontend/build "$package_root/frontend"
install -m 0644 .env.example "$package_root/"
install -m 0644 LICENSE "$package_root/"
install -m 0755 start.sh stop.sh "$package_root/"
install -m 0644 README.md "$package_root/README.md"
{
  printf 'git_commit=%s\n' "$git_commit"
  if [ -n "$(git status --porcelain --untracked-files=normal)" ]; then
    printf 'source_tree=dirty\n'
  else
    printf 'source_tree=clean\n'
  fi
} >"$package_root/SOURCE_STATE"

install -d dist
archive="dist/pixivarchive-linux-x86_64.tar.gz"
checksum="${archive}.sha256"
tar -C "$release_tmp" -czf "$archive" pixivarchive
(
  cd dist
  sha256sum "$(basename "$archive")" >"$(basename "$checksum")"
)
printf '%s\n' "$archive"
