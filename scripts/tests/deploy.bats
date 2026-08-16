#!/usr/bin/env bats

setup() {
  REPO_ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  cd "$REPO_ROOT"
  TEST_ROOT="$(mktemp -d)"
  export TEST_TRACE="$TEST_ROOT/trace"
}

teardown() {
  if [ -x "$TEST_ROOT/stop.sh" ]; then
    bash "$TEST_ROOT/stop.sh" >/dev/null 2>&1 || true
  fi
  rm -rf -- "$TEST_ROOT"
}

make_native_fixture() {
  mkdir -p "$TEST_ROOT/bin" "$TEST_ROOT/frontend"
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
trap 'exit 0' TERM
while :; do sleep 1; done
EOF
  done
  chmod +x "$TEST_ROOT/bin/"* "$TEST_ROOT/"*.sh
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
  for path in .env.example Dockerfile compose.yaml compose.build.yaml start.sh stop.sh; do
    [ -f "$path" ]
  done
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
  run grep -F 'image: ghcr.io/mizuno-sachiko/pixivarchive:0.1.0' compose.yaml
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
  for _ in $(seq 1 50); do
    [ "$(wc -l <"$TEST_TRACE")" -ge 3 ] && break
    sleep 0.02
  done
  [ "$(sort "$TEST_TRACE")" = $'prepare\nweb\nworker' ]

  run bash "$TEST_ROOT/stop.sh"
  [ "$status" -eq 0 ]
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
