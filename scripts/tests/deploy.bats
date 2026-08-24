#!/usr/bin/env bats

setup() {
  REPO_ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  cd "$REPO_ROOT"
  TEST_ROOT="$(mktemp -d)"
  export TEST_TRACE="$TEST_ROOT/trace"
}

teardown() {
  if [ -n "${UPGRADE_RUNNING_PID:-}" ]; then
    kill "$UPGRADE_RUNNING_PID" >/dev/null 2>&1 || true
    wait "$UPGRADE_RUNNING_PID" 2>/dev/null || true
  fi
  if [ -x "$TEST_ROOT/stop.sh" ]; then
    bash "$TEST_ROOT/stop.sh" >/dev/null 2>&1 || true
  fi
  rm -rf -- "$TEST_ROOT"
}

make_native_fixture() {
  NATIVE_COMMAND_BIN="$TEST_ROOT/native-command-bin"
  mkdir -p "$TEST_ROOT/bin" "$TEST_ROOT/frontend" "$NATIVE_COMMAND_BIN"
  cp start.sh stop.sh "$TEST_ROOT/"
  cat >"$TEST_ROOT/.env" <<'EOF'
DATABASE_URL=postgresql://pixivarchive:test@127.0.0.1:5432/pixivarchive
PIXIVARCHIVE_ADMIN_PASSWORD=test-password
PIXIVARCHIVE_MEDIA_ROOT=/srv/pixivarchive/media
PIXIVARCHIVE_WEB_BIND=127.0.0.1:17088
EOF
  cat >"$TEST_ROOT/bin/pixivarchive-admin" <<'EOF'
#!/usr/bin/env bash
printf 'prepare\n' >>"$TEST_TRACE"
exit "${PREPARE_EXIT:-0}"
EOF
  for service in web worker; do
    cat >"$TEST_ROOT/bin/pixivarchive-$service" <<EOF
#!/usr/bin/env bash
printf '$service\\n' >>"\$TEST_TRACE"
if [ "\${FAIL_SERVICE:-}" = "$service" ]; then exit 12; fi
trap '/usr/bin/sleep "\${TERM_DELAY:-0}"; exit 0' TERM
while :; do /usr/bin/sleep 0.01; done
EOF
  done
  cat >"$NATIVE_COMMAND_BIN/curl" <<'EOF'
#!/usr/bin/env bash
exit "${READY_EXIT:-0}"
EOF
  cat >"$NATIVE_COMMAND_BIN/sleep" <<'EOF'
#!/usr/bin/env bash
if [ "${1:-}" = "0.1" ]; then
  exec /usr/bin/sleep 0.001
fi
exec /usr/bin/sleep "$@"
EOF
  chmod +x "$TEST_ROOT/bin/"* "$TEST_ROOT/"*.sh "$NATIVE_COMMAND_BIN/"*
  export NATIVE_COMMAND_BIN
  export PATH="$NATIVE_COMMAND_BIN:$PATH"
}

make_upgrade_fixture() {
  UPGRADE_INSTALL="$TEST_ROOT/pixivarchive"
  UPGRADE_PAYLOAD="$TEST_ROOT/payload"
  UPGRADE_BIN="$TEST_ROOT/upgrade-bin"
  UPGRADE_ARCHIVE="$TEST_ROOT/pixivarchive-linux-x86_64.tar.gz"
  UPGRADE_CHECKSUM="$UPGRADE_ARCHIVE.sha256"
  UPGRADE_CURL_LOG="$TEST_ROOT/upgrade-curl.log"
  UPGRADE_START_LOG="$TEST_ROOT/upgrade-start.log"
  mkdir -p \
    "$UPGRADE_INSTALL/bin" \
    "$UPGRADE_INSTALL/frontend" \
    "$UPGRADE_PAYLOAD/pixivarchive/bin" \
    "$UPGRADE_PAYLOAD/pixivarchive/frontend" \
    "$UPGRADE_BIN"
  cp upgrade.sh "$UPGRADE_INSTALL/"
  printf '%s\n' 'preserved-environment' >"$UPGRADE_INSTALL/.env"
  printf '%s\n' 'old-runtime' >"$UPGRADE_INSTALL/old-marker"
  printf '%s\n' 'git_commit=old' >"$UPGRADE_INSTALL/SOURCE_STATE"

  cat >"$UPGRADE_INSTALL/start.sh" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' old >>"$UPGRADE_START_LOG"
exit "${UPGRADE_OLD_START_EXIT:-0}"
EOF
  cp /usr/bin/sleep "$UPGRADE_INSTALL/bin/pixivarchive-web"
  cp /usr/bin/sleep "$UPGRADE_INSTALL/bin/pixivarchive-worker"

  cp stop.sh upgrade.sh "$UPGRADE_PAYLOAD/pixivarchive/"
  cat >"$UPGRADE_PAYLOAD/pixivarchive/start.sh" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' new >>"$UPGRADE_START_LOG"
exit "${UPGRADE_NEW_START_EXIT:-0}"
EOF
  for binary in pixivarchive-admin pixivarchive-web pixivarchive-worker; do
    printf '%s\n' '#!/usr/bin/env bash' 'exit 0' \
      >"$UPGRADE_PAYLOAD/pixivarchive/bin/$binary"
  done
  printf '%s\n' 'new-runtime' \
    >"$UPGRADE_PAYLOAD/pixivarchive/frontend/200.html"
  printf '%s\n' 'git_commit=new' \
    >"$UPGRADE_PAYLOAD/pixivarchive/SOURCE_STATE"
  chmod +x \
    "$UPGRADE_INSTALL/start.sh" \
    "$UPGRADE_INSTALL/upgrade.sh" \
    "$UPGRADE_INSTALL/bin/"* \
    "$UPGRADE_PAYLOAD/pixivarchive/"*.sh \
    "$UPGRADE_PAYLOAD/pixivarchive/bin/"*
  tar \
    --owner=1001 \
    --group=1001 \
    --numeric-owner \
    -C "$UPGRADE_PAYLOAD" \
    -czf "$UPGRADE_ARCHIVE" \
    pixivarchive
  (
    cd "$TEST_ROOT"
    sha256sum "$(basename "$UPGRADE_ARCHIVE")" \
      >"$(basename "$UPGRADE_CHECKSUM")"
  )

  cat >"$UPGRADE_BIN/curl" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
destination=
url=
while [ "$#" -gt 0 ]; do
  case "$1" in
    -o|--output)
      destination="$2"
      shift 2
      ;;
    http://*|https://*)
      url="$1"
      shift
      ;;
    *)
      shift
      ;;
  esac
done
printf '%s\n' "$url" >>"$UPGRADE_CURL_LOG"
case "$url" in
  */releases/latest)
    printf '%s' "${UPGRADE_LATEST_URL:-https://github.com/Mizuno-Sachiko/PixivArchive/releases/tag/v0.2.0}"
    ;;
  *.tar.gz.sha256) cp "$UPGRADE_CHECKSUM" "$destination" ;;
  *.tar.gz) cp "$UPGRADE_ARCHIVE" "$destination" ;;
  *) exit 64 ;;
esac
SH
  chmod +x "$UPGRADE_BIN/curl"
  export UPGRADE_INSTALL UPGRADE_ARCHIVE UPGRADE_CHECKSUM
  export UPGRADE_CURL_LOG UPGRADE_START_LOG UPGRADE_BIN
}

make_release_script_fixture() {
  RELEASE_FIXTURE="$TEST_ROOT/release-fixture"
  RELEASE_BIN="$RELEASE_FIXTURE/test-bin"
  RELEASE_LOG="$RELEASE_FIXTURE/calls.log"
  mkdir -p \
    "$RELEASE_BIN" \
    "$RELEASE_FIXTURE/scripts" \
    "$RELEASE_FIXTURE/frontend/node_modules"
  cp scripts/verify-release.sh scripts/build-release.sh "$RELEASE_FIXTURE/scripts/"
  touch \
    "$RELEASE_FIXTURE/Cargo.toml" \
    "$RELEASE_FIXTURE/README.md" \
    "$RELEASE_FIXTURE/frontend/package.json" \
    "$RELEASE_LOG"

  cat >"$RELEASE_BIN/git" <<'EOF'
#!/usr/bin/env bash
case "$1" in
  status)
    exit 0
    ;;
  rev-parse)
    printf '%s\n' '24ffcc9'
    ;;
  *)
    exit 2
    ;;
esac
EOF
  cat >"$RELEASE_BIN/pnpm" <<'EOF'
#!/usr/bin/env bash
printf 'pnpm %s\n' "$*" >>"$RELEASE_TEST_LOG"
EOF
  cat >"$RELEASE_BIN/cargo" <<'EOF'
#!/usr/bin/env bash
printf 'cargo %s\n' "$*" >>"$RELEASE_TEST_LOG"
exit 37
EOF
  chmod +x "$RELEASE_BIN/"*
  export RELEASE_FIXTURE RELEASE_BIN RELEASE_LOG
}

@test "deployment files live at the repository root" {
  [ ! -e deploy ]
  for path in .env.example Dockerfile compose.yaml compose.build.yaml start.sh stop.sh upgrade.sh; do
    [ -f "$path" ]
  done
}

@test "native lifecycle scripts expose help and reject unknown arguments" {
  help_root="$TEST_ROOT/help"
  mkdir -p "$help_root"
  cp start.sh stop.sh upgrade.sh "$help_root/"
  for script in start stop; do
    run bash "$help_root/$script.sh" --help
    [ "$status" -eq 0 ]
    [ "$output" = "usage: $script.sh" ]

    run bash "$help_root/$script.sh" --unknown
    [ "$status" -eq 2 ]
    [ "$output" = "usage: $script.sh" ]
  done

  run bash "$help_root/upgrade.sh" --help
  [ "$status" -eq 0 ]
  [ "$output" = "usage: upgrade.sh (--latest|<vMAJOR.MINOR.PATCH>) --force" ]

  run bash "$help_root/upgrade.sh" --unknown
  [ "$status" -eq 2 ]
  [ "$output" = "usage: upgrade.sh (--latest|<vMAJOR.MINOR.PATCH>) --force" ]
}

@test "native upgrade requires an explicit version and overwrite flag" {
  run bash upgrade.sh v0.2.0

  [ "$status" -eq 2 ]
  [ "$output" = "usage: upgrade.sh (--latest|<vMAJOR.MINOR.PATCH>) --force" ]
}

@test "native upgrade resolves the latest stable release" {
  make_upgrade_fixture

  run env \
    PATH="$UPGRADE_BIN:$PATH" \
    UPGRADE_LATEST_URL='https://github.com/Mizuno-Sachiko/PixivArchive/releases/tag/v0.2.0' \
    bash "$UPGRADE_INSTALL/upgrade.sh" --latest --force

  [ "$status" -eq 0 ]
  [[ "$output" == *"PixivArchive: resolving latest stable release"* ]]
  [[ "$output" == *"Resolved latest stable release: v0.2.0"* ]]
  [[ "$output" == *"PixivArchive: target version is v0.2.0"* ]]
  [[ "$output" == *"PixivArchive: SHA-256 verified"* ]]
  [[ "$output" == *"PixivArchive: starting v0.2.0"* ]]
  [[ "$output" == *"PixivArchive: upgrade to v0.2.0 completed"* ]]
  [ "$(cat "$UPGRADE_INSTALL/SOURCE_STATE")" = "git_commit=new" ]
  [ "$(cat "$UPGRADE_START_LOG")" = "new" ]
  [ "$(wc -l <"$UPGRADE_CURL_LOG")" -eq 3 ]
}

@test "native upgrade rejects an invalid latest release tag before replacement" {
  make_upgrade_fixture

  run env \
    PATH="$UPGRADE_BIN:$PATH" \
    UPGRADE_LATEST_URL='https://github.com/Mizuno-Sachiko/PixivArchive/releases/latest' \
    bash "$UPGRADE_INSTALL/upgrade.sh" --latest --force

  [ "$status" -ne 0 ]
  [[ "$output" == *"the latest GitHub Release did not resolve to a stable version tag"* ]]
  [[ "$output" == *"PixivArchive: upgrade failed; current runtime unchanged"* ]]
  [ -f "$UPGRADE_INSTALL/old-marker" ]
  [ "$(cat "$UPGRADE_INSTALL/SOURCE_STATE")" = "git_commit=old" ]
}

@test "native upgrade verifies the release and preserves the environment" {
  make_upgrade_fixture
  original_env_hash="$(sha256sum "$UPGRADE_INSTALL/.env" | cut -d' ' -f1)"

  run env PATH="$UPGRADE_BIN:$PATH" \
    bash "$UPGRADE_INSTALL/upgrade.sh" v0.2.0 --force

  [ "$status" -eq 0 ]
  [ "$(cat "$UPGRADE_INSTALL/SOURCE_STATE")" = "git_commit=new" ]
  [ "$(sha256sum "$UPGRADE_INSTALL/.env" | cut -d' ' -f1)" = "$original_env_hash" ]
  [ ! -e "$UPGRADE_INSTALL/old-marker" ]
  [ "$(cat "$UPGRADE_START_LOG")" = "new" ]
  ! find "$TEST_ROOT" -maxdepth 1 -type d -name 'pixivarchive.before-*' | grep -q .
  ! find "$TEST_ROOT" -maxdepth 1 -type d -name '.pixivarchive-upgrade.*' | grep -q .
  [ "$(stat -c '%u:%g' "$UPGRADE_INSTALL/bin/pixivarchive-web")" = "$(id -u):$(id -g)" ]
  [ "$(wc -l <"$UPGRADE_CURL_LOG")" -eq 2 ]
}

@test "native upgrade refuses to replace a running runtime" {
  make_upgrade_fixture
  "$UPGRADE_INSTALL/bin/pixivarchive-web" 300 &
  UPGRADE_RUNNING_PID=$!

  run env PATH="$UPGRADE_BIN:$PATH" \
    bash "$UPGRADE_INSTALL/upgrade.sh" v0.2.0 --force

  [ "$status" -ne 0 ]
  [[ "$output" == *"PixivArchive is running; stop it before upgrading"* ]]
  [ "$(cat "$UPGRADE_INSTALL/SOURCE_STATE")" = "git_commit=old" ]
  [ ! -s "$UPGRADE_CURL_LOG" ]
  [ ! -e "$UPGRADE_START_LOG" ]
}

@test "native upgrade restores and starts the previous runtime when startup fails" {
  make_upgrade_fixture

  run env \
    PATH="$UPGRADE_BIN:$PATH" \
    UPGRADE_NEW_START_EXIT=17 \
    bash "$UPGRADE_INSTALL/upgrade.sh" v0.2.0 --force

  [ "$status" -eq 17 ]
  [[ "$output" == *"PixivArchive: v0.2.0 failed to start"* ]]
  [[ "$output" == *"PixivArchive: upgrade failed; previous runtime restored and started"* ]]
  [ "$(cat "$UPGRADE_START_LOG")" = $'new\nold' ]
  [ "$(cat "$UPGRADE_INSTALL/SOURCE_STATE")" = "git_commit=old" ]
  [ -f "$UPGRADE_INSTALL/old-marker" ]
  ! find "$TEST_ROOT" -maxdepth 1 -type d -name 'pixivarchive.before-*' | grep -q .
  ! find "$TEST_ROOT" -maxdepth 1 -type d -name '.pixivarchive-upgrade.*' | grep -q .
}

@test "native upgrade leaves the current runtime untouched on checksum failure" {
  make_upgrade_fixture
  printf '%064d  %s\n' 0 "$(basename "$UPGRADE_ARCHIVE")" >"$UPGRADE_CHECKSUM"

  run env PATH="$UPGRADE_BIN:$PATH" \
    bash "$UPGRADE_INSTALL/upgrade.sh" v0.2.0 --force

  [ "$status" -ne 0 ]
  [[ "$output" == *"PixivArchive: upgrade failed; current runtime unchanged"* ]]
  [ -f "$UPGRADE_INSTALL/old-marker" ]
  [ "$(cat "$UPGRADE_INSTALL/SOURCE_STATE")" = "git_commit=old" ]
  ! find "$TEST_ROOT" -maxdepth 1 -type d -name 'pixivarchive.before-v0.2.0-*' | grep -q .
}

@test "native upgrade reports that the previous runtime was restored" {
  make_upgrade_fixture
  cat >"$UPGRADE_BIN/mv" <<'EOF'
#!/usr/bin/env bash
if [ "${UPGRADE_FAIL_AFTER_SWAP:-0}" = "1" ] \
  && [ "$#" -eq 2 ] \
  && [[ "$1" == */.pixivarchive-upgrade.*/pixivarchive ]] \
  && [ "$2" = "$UPGRADE_INSTALL" ]; then
  /usr/bin/mv "$@"
  rm -f "$2/start.sh"
  exit 0
fi
exec /usr/bin/mv "$@"
EOF
  chmod +x "$UPGRADE_BIN/mv"

  run env \
    PATH="$UPGRADE_BIN:$PATH" \
    UPGRADE_FAIL_AFTER_SWAP=1 \
    bash "$UPGRADE_INSTALL/upgrade.sh" v0.2.0 --force

  [ "$status" -ne 0 ]
  [[ "$output" == *"PixivArchive: upgrade failed; previous runtime restored and started"* ]]
  [[ "$output" != *"PixivArchive: upgrade failed; current runtime unchanged"* ]]
  [ -f "$UPGRADE_INSTALL/old-marker" ]
  [ "$(cat "$UPGRADE_INSTALL/SOURCE_STATE")" = "git_commit=old" ]
  [ "$(cat "$UPGRADE_START_LOG")" = "old" ]
}

@test "native upgrade rejects a symbolic-link installation root" {
  make_upgrade_fixture
  linked_install="$TEST_ROOT/pixivarchive-link"
  ln -s "$UPGRADE_INSTALL" "$linked_install"

  run env PATH="$UPGRADE_BIN:$PATH" \
    bash "$linked_install/upgrade.sh" v0.2.0 --force

  [ "$status" -ne 0 ]
  [[ "$output" == *"the native installation root cannot be a symbolic link"* ]]
  [[ "$output" == *"PixivArchive: upgrade failed; current runtime unchanged"* ]]
  [ -f "$UPGRADE_INSTALL/old-marker" ]
}

@test "docker build context excludes independent test assets" {
  command -v docker >/dev/null || skip "docker is unavailable"
  docker buildx version >/dev/null 2>&1 || skip "docker buildx is unavailable"
  docker info >/dev/null 2>&1 || skip "docker daemon is unavailable"

  context_output="$TEST_ROOT/context-output"
  cat >"$TEST_ROOT/Dockerfile.context" <<'EOF'
FROM scratch
COPY . /
EOF

  run docker buildx build \
    --file "$TEST_ROOT/Dockerfile.context" \
    --output "type=local,dest=$context_output" \
    .
  [ "$status" -eq 0 ]

  for path in \
    apps/web/tests/api_contract.rs \
    crates/application/tests/auth.rs \
    crates/pixiv/src/mapper_tests.rs \
    fixtures/pixiv/illust.json \
    frontend/tests/login.spec.ts \
    frontend/playwright.config.ts \
    frontend/scripts/run-playwright.mjs \
    frontend/src/lib/api/client.test.ts \
    assets/screenshots/gallery-overview.png \
    scripts/verify-release.sh; do
    [ ! -e "$context_output/$path" ]
  done

  for path in \
    .sqlx \
    Cargo.toml \
    apps/web/src/lib.rs \
    crates/pixiv/src/lib.rs \
    frontend/package.json \
    frontend/src/app.html \
    migrations/0001_initial.sql; do
    [ -e "$context_output/$path" ]
  done
}

@test "one environment template owns common settings and leaves deployment passwords explicit" {
  run grep -Fx 'PIXIVARCHIVE_ADMIN_PASSWORD=' .env.example
  [ "$status" -eq 0 ]
  run grep -F 'PIXIVARCHIVE_MEDIA_HOST_PATH=/srv/pixivarchive/media' .env.example
  [ "$status" -eq 0 ]
  run grep -F '# PIXIVARCHIVE_MEDIA_ROOT=/srv/pixivarchive/media' .env.example
  [ "$status" -eq 0 ]
  run grep -F 'PIXIVARCHIVE_WEB_BIND=0.0.0.0:7088' .env.example
  [ "$status" -eq 0 ]
  run grep -Fx '# POSTGRES_PASSWORD=' .env.example
  [ "$status" -eq 0 ]
  run grep -F '# DATABASE_URL=postgresql://pixivarchive:password@127.0.0.1:5432/pixivarchive' .env.example
  [ "$status" -eq 0 ]
  run grep -E 'PIXIVARCHIVE_(PUBLIC_ORIGIN|CACHE_ROOT|TOTP_KEY|PIXIV_COOKIE_KEY)' .env.example
  [ "$status" -eq 1 ]
  run grep -F 'PIXIVARCHIVE_IMAGE' .env.example
  [ "$status" -eq 1 ]
  run grep -E 'PIXIVARCHIVE_(PIXIV_USE_SYSTEM_PROXY|PIXIV_USER_AGENT|VIPSTHUMBNAIL|WEBP_AVAILABLE|AVIF_AVAILABLE|REFLINK_AVAILABLE)' .env.example
  [ "$status" -eq 1 ]
}

@test "prebuilt compose rejects an unchanged administrator password" {
  command -v docker >/dev/null || skip "docker compose is unavailable"
  docker compose version >/dev/null 2>&1 || skip "docker compose is unavailable"
  mkdir -p "$TEST_ROOT/media"
  run env -u PIXIVARCHIVE_ADMIN_PASSWORD \
    PIXIVARCHIVE_MEDIA_HOST_PATH="$TEST_ROOT/media" \
    POSTGRES_PASSWORD=test \
    docker compose --env-file .env.example -f compose.yaml config
  [ "$status" -ne 0 ]
  [[ "$output" == *"PIXIVARCHIVE_ADMIN_PASSWORD is required"* ]]
}

@test "prebuilt compose owns PostgreSQL preparation and persistent external data" {
  run grep -F 'postgres:17-bookworm' compose.yaml
  [ "$status" -eq 0 ]
  run grep -F 'service_completed_successfully' compose.yaml
  [ "$status" -eq 0 ]
  run grep -F 'pixivarchive-admin prepare' compose.yaml
  [ "$status" -eq 0 ]
  run grep -F 'pixivarchive-postgres:/var/lib/postgresql/data' compose.yaml
  [ "$status" -eq 0 ]
  run grep -F 'image: ghcr.io/mizuno-sachiko/pixivarchive:0.2.0' compose.yaml
  [ "$status" -eq 0 ]
  run grep -F 'PIXIVARCHIVE_MEDIA_ROOT: /data/media' compose.yaml
  [ "$status" -eq 0 ]
  run grep -F 'source: ${PIXIVARCHIVE_MEDIA_HOST_PATH:?PIXIVARCHIVE_MEDIA_HOST_PATH is required}' compose.yaml
  [ "$status" -eq 0 ]
  run grep -F 'target: /data/media' compose.yaml
  [ "$status" -eq 0 ]
  run grep -F 'create_host_path: false' compose.yaml
  [ "$status" -eq 0 ]
  run grep -F 'PIXIVARCHIVE_IMAGE' compose.yaml
  [ "$status" -eq 1 ]
  run grep -F '${PIXIVARCHIVE_MEDIA_ROOT:' compose.yaml
  [ "$status" -eq 1 ]
  run grep -F 'dockerfile: Dockerfile' compose.yaml
  [ "$status" -eq 1 ]
}

@test "local build compose overrides only application image construction" {
  run grep -F 'dockerfile: Dockerfile' compose.build.yaml
  [ "$status" -eq 0 ]
  [ "$output" = "      dockerfile: Dockerfile" ]
  run grep -F 'pull_policy: build' compose.build.yaml
  [ "$status" -eq 0 ]
  [ "$output" = "    pull_policy: build" ]
  run grep -F 'pull_policy: never' compose.build.yaml
  [ "$status" -eq 0 ]
  [ "$(printf '%s\n' "$output" | wc -l)" -eq 2 ]
  run grep -F 'image: pixivarchive:local' compose.build.yaml
  [ "$status" -eq 0 ]
  [ "$(printf '%s\n' "$output" | wc -l)" -eq 3 ]
  run grep -F 'HTTP_PROXY: ${HTTP_PROXY:-}' compose.build.yaml
  [ "$status" -eq 0 ]
  run grep -F 'DOCKER_BUILD_' .env.example compose.build.yaml
  [ "$status" -eq 1 ]
  run grep -F 'postgres:' compose.build.yaml
  [ "$status" -eq 1 ]
  run grep -F 'pixivarchive-admin prepare' compose.build.yaml
  [ "$status" -eq 1 ]
}

@test "native start prepares before launching Web and Worker" {
  prepare_line="$(grep -n 'pixivarchive-admin" prepare' start.sh | cut -d: -f1)"
  web_line="$(grep -n 'pixivarchive-web"' start.sh | tail -n 1 | cut -d: -f1)"
  worker_line="$(grep -n 'pixivarchive-worker"' start.sh | tail -n 1 | cut -d: -f1)"

  [ -n "$prepare_line" ]
  [ -n "$web_line" ]
  [ -n "$worker_line" ]
  [ "$prepare_line" -lt "$web_line" ]
  [ "$prepare_line" -lt "$worker_line" ]
  run grep -F 'DATABASE_URL is required for native deployment' start.sh
  [ "$status" -eq 0 ]
  run grep -E 'systemctl|sysctl|caddy|apt(-get)?|useradd|chown' start.sh stop.sh
  [ "$status" -eq 1 ]
}

@test "native stop manages only the recorded PixivArchive processes" {
  run grep -F 'stop_process Web web' stop.sh
  [ "$status" -eq 0 ]
  run grep -F 'stop_process Worker worker' stop.sh
  [ "$status" -eq 0 ]
  run grep -F 'kill -TERM' stop.sh
  [ "$status" -eq 0 ]
}

@test "native startup does not launch services when preparation fails" {
  make_native_fixture

  run env PREPARE_EXIT=42 bash "$TEST_ROOT/start.sh"

  [ "$status" -eq 42 ]
  [ "$(cat "$TEST_TRACE")" = "prepare" ]
  [ ! -e "$TEST_ROOT/.runtime/web.pid" ]
  [ ! -e "$TEST_ROOT/.runtime/worker.pid" ]
}

@test "native startup launches both services and stop terminates them" {
  make_native_fixture

  run bash "$TEST_ROOT/start.sh"
  [ "$status" -eq 0 ]
  [ "${lines[0]}" = "PixivArchive: checking configuration" ]
  [ "${lines[1]}" = "PixivArchive: preparing database and installation" ]
  [ "${lines[2]}" = "PixivArchive: starting Web" ]
  [ "${lines[3]}" = "PixivArchive: starting Worker" ]
  [ "${lines[4]}" = "PixivArchive: waiting for Web readiness" ]
  [[ "$output" == *"PixivArchive: ready at http://127.0.0.1:17088"* ]]
  for _ in $(seq 1 50); do
    [ "$(wc -l <"$TEST_TRACE")" -ge 3 ] && break
    sleep 0.02
  done
  [ "$(sort "$TEST_TRACE")" = $'prepare\nweb\nworker' ]

  run bash "$TEST_ROOT/stop.sh"
  [ "$status" -eq 0 ]
  [[ "$output" == *"PixivArchive: stopping Worker (PID "* ]]
  [[ "$output" == *"PixivArchive: stopping Web (PID "* ]]
  [[ "$output" == *"PixivArchive: all processes stopped"* ]]
  [ ! -e "$TEST_ROOT/.runtime/web.pid" ]
  [ ! -e "$TEST_ROOT/.runtime/worker.pid" ]
}

@test "native stop reports a delayed shutdown once per process" {
  make_native_fixture
  run env TERM_DELAY=0.25 bash "$TEST_ROOT/start.sh"
  [ "$status" -eq 0 ]

  run bash "$TEST_ROOT/stop.sh"

  [ "$status" -eq 0 ]
  [ "$(printf '%s\n' "${lines[@]}" | grep -Fc 'PixivArchive: Worker is still shutting down')" -eq 1 ]
  [ "$(printf '%s\n' "${lines[@]}" | grep -Fc 'PixivArchive: Web is still shutting down')" -eq 1 ]
  [[ "$output" == *"PixivArchive: Worker stopped after "*" seconds"* ]]
  [[ "$output" == *"PixivArchive: Web stopped after "*" seconds"* ]]
  [[ "$output" == *"PixivArchive: all processes stopped"* ]]
}

@test "native startup cleans up when readiness never succeeds" {
  make_native_fixture

  run env READY_EXIT=22 bash "$TEST_ROOT/start.sh"

  [ "$status" -ne 0 ]
  [[ "$output" == *"PixivArchive: readiness check failed at http://127.0.0.1:17088/health/ready"* ]]
  [ ! -e "$TEST_ROOT/.runtime/web.pid" ]
  [ ! -e "$TEST_ROOT/.runtime/worker.pid" ]
}

@test "native startup stops its Web process when Worker exits" {
  make_native_fixture

  run env FAIL_SERVICE=worker bash "$TEST_ROOT/start.sh"

  [ "$status" -ne 0 ]
  [ ! -e "$TEST_ROOT/.runtime/web.pid" ]
  [ ! -e "$TEST_ROOT/.runtime/worker.pid" ]
}

@test "native stop refuses a PID owned by another process" {
  make_native_fixture
  mkdir -p "$TEST_ROOT/.runtime"
  sleep 30 &
  foreign_pid=$!
  printf '%s\n' "$foreign_pid" >"$TEST_ROOT/.runtime/web.pid"

  run bash "$TEST_ROOT/stop.sh"

  [ "$status" -ne 0 ]
  kill -0 "$foreign_pid"
  kill "$foreign_pid"
  wait "$foreign_pid" 2>/dev/null || true
}

@test "release builder honors an external Cargo target directory" {
  run grep -F 'cargo_target_dir="${CARGO_TARGET_DIR:-$repo_root/target}"' scripts/build-release.sh
  [ "$status" -eq 0 ]
  run grep -F 'source_binary="$cargo_target_dir/$target/release/$binary"' scripts/build-release.sh
  [ "$status" -eq 0 ]
}

@test "release builder only exposes build and package options" {
  run bash scripts/build-release.sh --unknown
  [ "$status" -eq 2 ]
  [ "$output" = "usage: scripts/build-release.sh [--allow-dirty] [--reuse-frontend-build]" ]

  run grep -E 'cargo (fmt|test|clippy)|prepare-sqlx|check-generated|check-doc-links|frontend (format:check|lint|check|test)' scripts/build-release.sh
  [ "$status" -eq 1 ]
}

@test "release verification refreshes frontend dependencies even when the cache exists" {
  make_release_script_fixture

  run env \
    PATH="$RELEASE_BIN:$PATH" \
    RELEASE_TEST_LOG="$RELEASE_LOG" \
    bash "$RELEASE_FIXTURE/scripts/verify-release.sh"

  [ "$status" -eq 37 ]
  [ "$(sed -n '1p' "$RELEASE_LOG")" = "pnpm --dir frontend install --frozen-lockfile" ]
  [ "$(sed -n '2p' "$RELEASE_LOG")" = "cargo fmt --all -- --check" ]
}

@test "release builder refreshes dependencies before rebuilding the frontend" {
  make_release_script_fixture

  run env \
    PATH="$RELEASE_BIN:$PATH" \
    RELEASE_TEST_LOG="$RELEASE_LOG" \
    bash "$RELEASE_FIXTURE/scripts/build-release.sh"

  [ "$status" -eq 37 ]
  [ "$(sed -n '1p' "$RELEASE_LOG")" = "pnpm --dir frontend install --frozen-lockfile" ]
  [ "$(sed -n '2p' "$RELEASE_LOG")" = "pnpm --dir frontend build" ]
  [ "$(sed -n '3p' "$RELEASE_LOG")" = "cargo build --locked --release --target x86_64-unknown-linux-musl -p pixivarchive-web -p pixivarchive-worker -p pixivarchive-admin" ]
}

@test "release builder reuses an already verified frontend without reinstalling" {
  make_release_script_fixture
  mkdir -p "$RELEASE_FIXTURE/frontend/build"
  touch "$RELEASE_FIXTURE/frontend/build/200.html"

  run env \
    PATH="$RELEASE_BIN:$PATH" \
    RELEASE_TEST_LOG="$RELEASE_LOG" \
    bash "$RELEASE_FIXTURE/scripts/build-release.sh" --reuse-frontend-build

  [ "$status" -eq 37 ]
  ! grep -q '^pnpm ' "$RELEASE_LOG"
  [ "$(sed -n '1p' "$RELEASE_LOG")" = "cargo build --locked --release --target x86_64-unknown-linux-musl -p pixivarchive-web -p pixivarchive-worker -p pixivarchive-admin" ]
}

@test "release builder packages only the native runtime" {
  run grep -F 'cp -a migrations "$package_root/migrations"' scripts/build-release.sh
  [ "$status" -eq 1 ]
  run grep -F 'install -m 0644 .env.example "$package_root/"' scripts/build-release.sh
  [ "$status" -eq 0 ]
  run grep -F 'install -m 0644 LICENSE "$package_root/"' scripts/build-release.sh
  [ "$status" -eq 0 ]
  run grep -F 'COPY LICENSE LICENSE' Dockerfile
  [ "$status" -eq 0 ]
  run grep -F 'compose.yaml "$package_root/"' scripts/build-release.sh
  [ "$status" -eq 1 ]
  run grep -F 'cp -a docs "$package_root/docs"' scripts/build-release.sh
  [ "$status" -eq 1 ]
  run grep -F 'pixivarchive/compose.yaml' scripts/verify-release.sh
  [ "$status" -eq 1 ]
  run grep -F 'pixivarchive/docs/' scripts/verify-release.sh
  [ "$status" -eq 1 ]
  run grep -F 'pixivarchive/assets/' scripts/verify-release.sh
  [ "$status" -eq 1 ]
  run grep -F 'pixivarchive/deploy/' scripts/verify-release.sh
  [ "$status" -eq 1 ]
  run grep -F 'release archive contains migration source files' scripts/verify-release.sh
  [ "$status" -eq 0 ]
  run grep -F 'release archive contains source-build deployment files' scripts/verify-release.sh
  [ "$status" -eq 0 ]
  run grep -F 'release archive contains deployment or documentation sources' scripts/verify-release.sh
  [ "$status" -eq 0 ]
}

@test "release archive ownership is normalized and verified" {
  run grep -F -- '--owner=0' scripts/build-release.sh
  [ "$status" -eq 0 ]
  run grep -F -- '--group=0' scripts/build-release.sh
  [ "$status" -eq 0 ]
  run grep -F -- '--numeric-owner' scripts/build-release.sh
  [ "$status" -eq 0 ]
  run grep -F 'release archive contains non-root ownership' scripts/build-release.sh
  [ "$status" -eq 0 ]
}

@test "release checks generated contracts without writing tracked files" {
  run grep -F 'bash scripts/check-generated.sh' scripts/build-release.sh
  [ "$status" -eq 1 ]
  run grep -F 'bash scripts/check-generated.sh' scripts/verify-release.sh
  [ "$status" -eq 0 ]
  run grep -F 'export-openapi openapi/pixivarchive.json' scripts/build-release.sh
  [ "$status" -eq 1 ]
  run grep -F 'export-rule-catalog frontend/src/lib/api/rule-catalog.generated.ts' scripts/build-release.sh
  [ "$status" -eq 1 ]
}

@test "release workflow publishes immutable version and commit tags" {
  run grep -F 'packages: write' .github/workflows/release.yml
  [ "$status" -eq 0 ]
  run grep -F 'type=semver,pattern={{version}}' .github/workflows/release.yml
  [ "$status" -eq 0 ]
  run grep -F 'type=sha,format=long' .github/workflows/release.yml
  [ "$status" -eq 0 ]
  run grep -F 'docker/build-push-action@v7' .github/workflows/release.yml
  [ "$status" -eq 0 ]
  run grep -F 'type=raw,value=latest' .github/workflows/release.yml
  [ "$status" -eq 1 ]
  run grep -F 'run: bash scripts/build-release.sh' .github/workflows/release.yml
  [ "$status" -eq 0 ]
}

@test "text files keep LF endings across Windows and Linux checkouts" {
  run grep -Fx '* text=auto eol=lf' .gitattributes
  [ "$status" -eq 0 ]
}

@test "release verification builds the frontend once and checks browsers and archive integrity" {
  run bash -c 'grep -Fc "pnpm --dir frontend build" scripts/verify-release.sh'
  [ "$status" -eq 0 ]
  [ "$output" -eq 1 ]
  run grep -F 'rm -rf -- frontend/.svelte-kit frontend/build' scripts/verify-release.sh
  [ "$status" -eq 0 ]
  run grep -F 'pnpm --dir frontend test:e2e:run' scripts/verify-release.sh
  [ "$status" -eq 0 ]
  run grep -F 'build_args=(--reuse-frontend-build)' scripts/verify-release.sh
  [ "$status" -eq 0 ]
  run grep -F 'bats scripts/tests/deploy.bats' scripts/verify-release.sh
  [ "$status" -eq 0 ]
  run grep -F 'bats scripts/tests/test-db.bats' scripts/verify-release.sh
  [ "$status" -eq 0 ]
  run grep -F 'sha256sum -c' scripts/verify-release.sh
  [ "$status" -eq 0 ]
  run grep -F 'tar -tzf' scripts/verify-release.sh
  [ "$status" -eq 0 ]
}
