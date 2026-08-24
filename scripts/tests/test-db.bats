#!/usr/bin/env bats

setup() {
  TEST_ROOT="$(mktemp -d)"
  BIN_DIR="$TEST_ROOT/bin"
  LOG_FILE="$TEST_ROOT/calls.log"
  mkdir -p "$BIN_DIR"
  touch "$LOG_FILE"
  chmod 755 "$TEST_ROOT" "$BIN_DIR"
  chmod 666 "$LOG_FILE"
  export PATH="$BIN_DIR:$PATH"
  export TEST_DB_LOG="$LOG_FILE"

  cat >"$BIN_DIR/pg_isready" <<'SH'
#!/usr/bin/env bash
log="${TEST_DB_LOG:-$ROOT_BRANCH_LOG}"
echo "pg_isready $*" >>"$log"
exit 0
SH
  cat >"$BIN_DIR/createdb" <<'SH'
#!/usr/bin/env bash
log="${TEST_DB_LOG:-$ROOT_BRANCH_LOG}"
echo "createdb $*" >>"$log"
exit 0
SH
  cat >"$BIN_DIR/dropdb" <<'SH'
#!/usr/bin/env bash
log="${TEST_DB_LOG:-$ROOT_BRANCH_LOG}"
echo "dropdb $*" >>"$log"
exit 0
SH
  cat >"$BIN_DIR/psql" <<'SH'
#!/usr/bin/env bash
log="${TEST_DB_LOG:-$ROOT_BRANCH_LOG}"
echo "psql $*" >>"$log"
exit 0
SH
  cat >"$BIN_DIR/pg_lsclusters" <<'SH'
#!/usr/bin/env bash
echo "17 main 15001 online postgres /var/lib/postgresql/17/main /var/log/postgresql/postgresql-17-main.log"
SH
  cat >"$BIN_DIR/cargo" <<'SH'
#!/usr/bin/env bash
echo "cargo $*" >>"$TEST_DB_LOG"
if [ "$1" = "sqlx" ] && [ "$2" = "migrate" ]; then
  printf '%s\n' "$DATABASE_URL" >"$TEST_ROOT/migrated-url"
fi
if [ "$1" = "sqlx" ] && [ "$2" = "prepare" ]; then
  printf '%s\n' "$DATABASE_URL" >"$TEST_ROOT/prepared-url"
fi
exit 0
SH
  cat >"$BIN_DIR/id" <<'SH'
#!/usr/bin/env bash
case "$1" in
  -u)
    echo 0
    ;;
  postgres)
    exit 0
    ;;
  *)
    /usr/bin/id "$@"
    ;;
esac
SH
  cat >"$BIN_DIR/runuser" <<'SH'
#!/usr/bin/env bash
log="${TEST_DB_LOG:-$ROOT_BRANCH_LOG}"
echo "runuser $*" >>"$log"
exit 0
SH
  cat >"$BIN_DIR/su" <<'SH'
#!/usr/bin/env bash
log="${TEST_DB_LOG:-$ROOT_BRANCH_LOG}"
echo "su $*" >>"$log"
exit 0
SH
  chmod +x "$BIN_DIR"/*
  export TEST_ROOT
  export ROOT_BRANCH_LOG="$LOG_FILE"
}

teardown() {
  rm -rf "$TEST_ROOT"
}

assert_no_database_tool_calls() {
  [ ! -e "$LOG_FILE" ] || ! grep -Eq 'pg_isready|createdb|dropdb|psql|cargo sqlx|runuser|su ' "$LOG_FILE"
}

@test "test-db creates a unique database, exports the url, and drops it after success" {
  run bash scripts/test-db.sh -- bash -c 'printf "%s\n" "$DATABASE_URL" >"$TEST_ROOT/child-url"; test "$DATABASE_URL" = "$PIXIVARCHIVE_TEST_DATABASE_URL"'

  [ "$status" -eq 0 ]
  grep -q '^postgres://pixivarchive_test:pixivarchive_test@127.0.0.1:15001/pixivarchive_test_' "$TEST_ROOT/child-url"
  grep -q 'pg_isready -h 127.0.0.1 -p 15001' "$LOG_FILE"
  grep -q 'createdb .*pixivarchive_test_' "$LOG_FILE"
  grep -q 'dropdb .*pixivarchive_test_' "$LOG_FILE"
}

@test "test-db uses the configured cluster port while PostgreSQL is stopped" {
  cat >"$BIN_DIR/pg_lsclusters" <<'SH'
#!/usr/bin/env bash
echo "17 main 15001 down postgres /var/lib/postgresql/17/main /var/log/postgresql/postgresql-17-main.log"
SH
  chmod +x "$BIN_DIR/pg_lsclusters"

  run bash scripts/test-db.sh -- bash -c 'printf "%s\n" "$DATABASE_URL" >"$TEST_ROOT/child-url"'

  [ "$status" -eq 0 ]
  grep -q '^postgres://pixivarchive_test:pixivarchive_test@127.0.0.1:15001/pixivarchive_test_' "$TEST_ROOT/child-url"
}

@test "test-db applies migrations before the child command when requested" {
  run bash scripts/test-db.sh --migrate -- bash -c 'test -s "$TEST_ROOT/migrated-url"'

  [ "$status" -eq 0 ]
  grep -q 'cargo sqlx migrate run --source migrations' "$LOG_FILE"
}

@test "test-db drops the database when the child command fails" {
  run bash scripts/test-db.sh -- bash -c 'exit 23'

  [ "$status" -eq 23 ]
  grep -q 'dropdb .*pixivarchive_test_' "$LOG_FILE"
}

@test "test-db uses different database names for consecutive invocations" {
  run bash -c 'bash scripts/test-db.sh -- bash -c "printenv DATABASE_URL >\"\$TEST_ROOT/one-url\"" && bash scripts/test-db.sh -- bash -c "printenv DATABASE_URL >\"\$TEST_ROOT/two-url\""'

  [ "$status" -eq 0 ]
  [ "$(cat "$TEST_ROOT/one-url")" != "$(cat "$TEST_ROOT/two-url")" ]
}

@test "test-db supports parallel invocations with different database names" {
  run bash -c 'bash scripts/test-db.sh -- bash -c "printenv DATABASE_URL >\"\$TEST_ROOT/parallel-one-url\"" & p1=$!; bash scripts/test-db.sh -- bash -c "printenv DATABASE_URL >\"\$TEST_ROOT/parallel-two-url\"" & p2=$!; wait "$p1"; wait "$p2"'

  [ "$status" -eq 0 ]
  [ "$(cat "$TEST_ROOT/parallel-one-url")" != "$(cat "$TEST_ROOT/parallel-two-url")" ]
}

@test "prepare-sqlx checks metadata against a migrated isolated database" {
  run bash scripts/prepare-sqlx.sh --check

  [ "$status" -eq 0 ]
  grep -q 'cargo sqlx migrate run --source migrations' "$LOG_FILE"
  grep -q 'cargo sqlx prepare --check --workspace -- --all-targets' "$LOG_FILE"
}

@test "test-db rejects an unsafe PGPORT before invoking database tools" {
  PGPORT='15001; touch /tmp/pixivarchive-test-db-bad-port' run bash scripts/test-db.sh -- bash -c 'exit 0'

  [ "$status" -eq 2 ]
  [ ! -e /tmp/pixivarchive-test-db-bad-port ]
  assert_no_database_tool_calls
}

@test "test-db rejects out-of-range PGPORT values" {
  PGPORT=70000 run bash scripts/test-db.sh -- bash -c 'exit 0'

  [ "$status" -eq 2 ]
  assert_no_database_tool_calls
}

@test "test-db rejects invalid discovered cluster ports" {
  cat >"$BIN_DIR/pg_lsclusters" <<'SH'
#!/usr/bin/env bash
echo "17 main 15001;touch-/tmp/bad online postgres /var/lib/postgresql/17/main /var/log/postgresql/postgresql-17-main.log"
SH
  chmod +x "$BIN_DIR/pg_lsclusters"

  run bash scripts/test-db.sh -- bash -c 'exit 0'

  [ "$status" -eq 2 ]
  assert_no_database_tool_calls
}

@test "test-db root branch rejects unsafe PGPORT before invoking database tools" {
  PGPORT='15001; touch /tmp/pixivarchive-test-db-root-bad-port' run env -u TEST_DB_LOG ROOT_BRANCH_LOG="$ROOT_BRANCH_LOG" PATH="$PATH" bash scripts/test-db.sh -- bash -c 'exit 0'

  [ "$status" -eq 2 ]
  [ ! -e /tmp/pixivarchive-test-db-root-bad-port ]
  assert_no_database_tool_calls
}

@test "test-db root branch rejects invalid discovered ports before invoking database tools" {
  cat >"$BIN_DIR/pg_lsclusters" <<'SH'
#!/usr/bin/env bash
echo "17 main 15001;touch-/tmp/bad online postgres /var/lib/postgresql/17/main /var/log/postgresql/postgresql-17-main.log"
SH
  chmod +x "$BIN_DIR/pg_lsclusters"

  run env -u TEST_DB_LOG ROOT_BRANCH_LOG="$ROOT_BRANCH_LOG" PATH="$PATH" bash scripts/test-db.sh -- bash -c 'exit 0'

  [ "$status" -eq 2 ]
  assert_no_database_tool_calls
}

@test "test-db root branch uses runuser argv form for create and drop" {
  run env -u TEST_DB_LOG ROOT_BRANCH_LOG="$ROOT_BRANCH_LOG" PATH="$PATH" bash scripts/test-db.sh -- bash -c 'test "$DATABASE_URL" = "$PIXIVARCHIVE_TEST_DATABASE_URL"'

  [ "$status" -eq 0 ]
  grep -q 'createdb -p 15001 -O pixivarchive_test pixivarchive_test_' "$LOG_FILE"
  grep -q 'dropdb -p 15001 --if-exists pixivarchive_test_' "$LOG_FILE"
  ! grep -q '^su ' "$LOG_FILE"
}

@test "test-db root branch finds system runuser when PATH omits sbin" {
  rm "$BIN_DIR/runuser"
  narrow_path="$BIN_DIR:/root/.cargo/bin:/usr/local/bin:/usr/bin:/bin"

  run env -u TEST_DB_LOG ROOT_BRANCH_LOG="$ROOT_BRANCH_LOG" PATH="$narrow_path" bash scripts/test-db.sh -- bash -c 'test "$DATABASE_URL" = "$PIXIVARCHIVE_TEST_DATABASE_URL"'

  [ "$status" -eq 0 ]
  grep -q 'createdb -p 15001 -O pixivarchive_test pixivarchive_test_' "$LOG_FILE"
  grep -q 'dropdb -p 15001 --if-exists pixivarchive_test_' "$LOG_FILE"
  ! grep -q '^su ' "$LOG_FILE"
}
