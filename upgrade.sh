#!/usr/bin/env bash
set -euo pipefail

usage() {
  echo "usage: upgrade.sh (--latest|<vMAJOR.MINOR.PATCH>) --force"
}

if [ "$#" -eq 1 ] && { [ "$1" = "-h" ] || [ "$1" = "--help" ]; }; then
  usage
  exit 0
fi
if [ "$#" -ne 2 ] || [ "$2" != "--force" ]; then
  usage >&2
  exit 2
fi

requested_version="$1"
if [ "$requested_version" != "--latest" ] \
  && ! [[ "$requested_version" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  usage >&2
  exit 2
fi

swap_state=0
failure_reported=0
preserve_upgrade_root=0
upgrade_root=""
cleanup() {
  local status=$?
  trap - EXIT
  if [ -n "$upgrade_root" ] && [ "$preserve_upgrade_root" -eq 0 ]; then
    rm -rf -- "$upgrade_root"
  fi
  if [ "$status" -ne 0 ] && [ "$failure_reported" -eq 0 ]; then
    echo "PixivArchive: upgrade failed; current runtime unchanged" >&2
  fi
  exit "$status"
}
trap cleanup EXIT

for command in curl sha256sum tar mktemp mv cp readlink; do
  if ! command -v "$command" >/dev/null 2>&1; then
    echo "$command is required for native upgrades" >&2
    exit 1
  fi
done

install_root_logical="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -L)"
install_root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
install_parent="$(dirname "$install_root")"
install_name="$(basename "$install_root")"
env_file="$install_root/.env"
if [ ! -f "$env_file" ]; then
  echo ".env is missing from $install_root" >&2
  exit 1
fi
if [ "$install_root_logical" != "$install_root" ] || [ -L "$install_root_logical" ]; then
  echo "the native installation root cannot be a symbolic link" >&2
  exit 1
fi

runtime_process_is_running() {
  local executable="$1"
  local expected proc_executable
  [ -x "$executable" ] || return 1
  expected="$(readlink -f "$executable")"
  for proc_executable in /proc/[0-9]*/exe; do
    if [ "$(readlink -f "$proc_executable" 2>/dev/null || true)" = "$expected" ]; then
      return 0
    fi
  done
  return 1
}

for service in web worker; do
  if runtime_process_is_running "$install_root/bin/pixivarchive-$service"; then
    echo "PixivArchive is running; stop it before upgrading" >&2
    exit 1
  fi
done

if [ "$requested_version" = "--latest" ]; then
  printf 'PixivArchive: resolving latest stable release\n'
  latest_release_url="$(curl \
    --proto '=https' \
    --tlsv1.2 \
    --fail \
    --location \
    --silent \
    --show-error \
    --output /dev/null \
    --write-out '%{url_effective}' \
    https://github.com/Mizuno-Sachiko/PixivArchive/releases/latest)"
  latest_release_url="${latest_release_url%/}"
  version="${latest_release_url##*/}"
  if ! [[ "$version" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "the latest GitHub Release did not resolve to a stable version tag" >&2
    exit 1
  fi
  printf 'Resolved latest stable release: %s\n' "$version"
else
  version="$requested_version"
fi
printf 'PixivArchive: target version is %s\n' "$version"

umask 077
upgrade_root="$(mktemp -d "$install_parent/.${install_name}-upgrade.XXXXXX")"
archive="$upgrade_root/pixivarchive-linux-x86_64.tar.gz"
checksum="$archive.sha256"
new_root="$upgrade_root/pixivarchive"
release_url="https://github.com/Mizuno-Sachiko/PixivArchive/releases/download/$version"
previous_root="$upgrade_root/previous"

printf 'PixivArchive: downloading release\n'
curl \
  --proto '=https' \
  --tlsv1.2 \
  --fail \
  --location \
  --silent \
  --show-error \
  --retry 3 \
  --retry-delay 2 \
  --output "$archive" \
  "$release_url/$(basename "$archive")"
curl \
  --proto '=https' \
  --tlsv1.2 \
  --fail \
  --location \
  --silent \
  --show-error \
  --retry 3 \
  --retry-delay 2 \
  --output "$checksum" \
  "$release_url/$(basename "$checksum")"

published_sha="$(awk 'NR == 1 { print $1 }' "$checksum")"
if ! [[ "$published_sha" =~ ^[0-9a-f]{64}$ ]]; then
  echo "the published SHA-256 file is invalid" >&2
  exit 1
fi
actual_sha="$(sha256sum "$archive" | cut -d' ' -f1)"
if [ "$actual_sha" != "$published_sha" ]; then
  echo "the downloaded release archive failed SHA-256 verification" >&2
  exit 1
fi
printf 'PixivArchive: SHA-256 verified\n'

archive_entries="$(tar -tzf "$archive")"
if grep -Eq '(^/|(^|/)\.\.(/|$))' <<<"$archive_entries"; then
  echo "the release archive contains an unsafe path" >&2
  exit 1
fi
if ! awk -F/ 'NF > 0 && $1 != "pixivarchive" { exit 1 }' \
  <<<"$archive_entries"; then
  echo "the release archive has an unexpected root" >&2
  exit 1
fi

tar --no-same-owner -xzf "$archive" -C "$upgrade_root"
for required in \
  bin/pixivarchive-web \
  bin/pixivarchive-worker \
  bin/pixivarchive-admin \
  frontend/200.html \
  start.sh \
  stop.sh \
  upgrade.sh \
  SOURCE_STATE; do
  if [ ! -e "$new_root/$required" ]; then
    echo "the release archive is missing pixivarchive/$required" >&2
    exit 1
  fi
done
for executable in \
  bin/pixivarchive-web \
  bin/pixivarchive-worker \
  bin/pixivarchive-admin \
  start.sh \
  stop.sh \
  upgrade.sh; do
  if [ ! -x "$new_root/$executable" ]; then
    echo "the release file is not executable: pixivarchive/$executable" >&2
    exit 1
  fi
done
printf 'PixivArchive: release archive verified\n'

cp -a "$env_file" "$new_root/.env"
old_env_sha="$(sha256sum "$env_file" | cut -d' ' -f1)"
new_env_sha="$(sha256sum "$new_root/.env" | cut -d' ' -f1)"
if [ "$old_env_sha" != "$new_env_sha" ]; then
  echo ".env changed while preparing the native upgrade" >&2
  exit 1
fi
rollback_swap() {
  local status="${1:-$?}"
  local restored=0
  local restart_status
  trap - ERR
  if [ "$swap_state" -eq 0 ]; then
    exit "$status"
  fi

  failure_reported=1
  if [ "$swap_state" -eq 2 ] && [ -d "$install_root" ] && [ -d "$previous_root" ]; then
    if mv "$install_root" "$upgrade_root/failed-install" \
      && mv "$previous_root" "$install_root"; then
      restored=1
    fi
  elif [ "$swap_state" -eq 1 ] && [ ! -e "$install_root" ] && [ -d "$previous_root" ]; then
    if mv "$previous_root" "$install_root"; then
      restored=1
    fi
  fi
  if [ "$restored" -eq 1 ]; then
    if bash "$install_root/start.sh"; then
      echo "PixivArchive: upgrade failed; previous runtime restored and started" >&2
      exit "$status"
    else
      restart_status=$?
      echo "PixivArchive: previous runtime restored but failed to start" >&2
      exit "$restart_status"
    fi
  else
    preserve_upgrade_root=1
    echo "PixivArchive: upgrade failed; automatic restore did not complete" >&2
    echo "PixivArchive: recovery files remain at $upgrade_root" >&2
  fi
  exit "$status"
}
trap 'rollback_swap $?' ERR

printf 'PixivArchive: replacing runtime files\n'
mv "$install_root" "$previous_root"
swap_state=1
mv "$new_root" "$install_root"
swap_state=2
test -f "$install_root/.env"
test -x "$install_root/start.sh"
test -x "$install_root/stop.sh"
test -x "$install_root/upgrade.sh"

printf 'PixivArchive: runtime files updated to %s\n' "$version"
printf 'PixivArchive: starting %s\n' "$version"
if bash "$install_root/start.sh"; then
  trap - ERR
else
  start_status=$?
  printf 'PixivArchive: %s failed to start\n' "$version" >&2
  rollback_swap "$start_status"
fi

printf 'PixivArchive: upgrade to %s completed\n' "$version"
