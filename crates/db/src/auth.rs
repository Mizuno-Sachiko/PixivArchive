use crate::{Db, DbError};
use pixivarchive_domain::auth::{AdministratorRecord, SessionContext};
use sqlx::{Postgres, Row, Transaction};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

#[derive(Clone)]
pub struct AuthRepository {
    db: Db,
}

impl AuthRepository {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub async fn create_administrator(
        &self,
        username: &str,
        password_phc: &str,
        now: OffsetDateTime,
    ) -> Result<AdministratorRecord, DbError> {
        let id = Uuid::now_v7();
        let row = sqlx::query(
            r#"
            INSERT INTO administrator
                (id, username, password_phc, password_created_at, password_changed_at, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $4, $4, $4)
            RETURNING id, username, password_phc, password_version, revision
            "#,
        )
        .bind(id)
        .bind(username)
        .bind(password_phc)
        .bind(now)
        .fetch_one(self.db.pool())
        .await?;
        admin_from_row(&row)
    }

    pub async fn administrator(&self) -> Result<AdministratorRecord, DbError> {
        let row = sqlx::query(
            "SELECT id, username, password_phc, password_version, revision FROM administrator LIMIT 1",
        )
        .fetch_one(self.db.pool())
        .await?;
        admin_from_row(&row)
    }

    pub async fn optional_administrator(&self) -> Result<Option<AdministratorRecord>, DbError> {
        let row = sqlx::query(
            "SELECT id, username, password_phc, password_version, revision FROM administrator LIMIT 1",
        )
        .fetch_optional(self.db.pool())
        .await?;
        row.map(|row| admin_from_row(&row)).transpose()
    }

    pub async fn update_password_and_revoke_sessions(
        &self,
        input: UpdatePassword<'_>,
    ) -> Result<(), DbError> {
        let mut tx = self.db.begin().await?;
        let locked = lock_administrator(&mut tx, input.administrator_snapshot.id).await?;
        compare_administrator(&locked, input.administrator_snapshot)?;
        sqlx::query(
            r#"
            UPDATE administrator
            SET password_phc = $2,
                password_version = password_version + CASE WHEN $3 THEN 1 ELSE 0 END,
                password_changed_at = CASE WHEN $3 THEN $4 ELSE password_changed_at END,
                updated_at = $4,
                revision = revision + 1
            WHERE id = $1
            "#,
        )
        .bind(input.administrator_snapshot.id)
        .bind(input.new_phc)
        .bind(input.increment_version)
        .bind(input.now)
        .execute(&mut *tx)
        .await?;
        if input.increment_version {
            revoke_sessions_in_tx(&mut tx, input.administrator_snapshot.id, input.now).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn finalize_successful_login(
        &self,
        input: IssueSession<'_>,
        password_replacement_phc: Option<&str>,
        lease: RateLimitLease,
        attempt: LoginAttempt<'_>,
    ) -> Result<SessionContext, DbError> {
        let mut tx = self.db.begin().await?;
        let locked = lock_administrator(&mut tx, input.administrator_snapshot.id).await?;
        compare_administrator(&locked, input.administrator_snapshot)?;
        if let Some(new_phc) = password_replacement_phc {
            sqlx::query(
                r#"
                UPDATE administrator
                SET password_phc = $2, updated_at = $3, revision = revision + 1
                WHERE id = $1
                "#,
            )
            .bind(input.administrator_snapshot.id)
            .bind(new_phc)
            .bind(input.now)
            .execute(&mut *tx)
            .await?;
        }
        let session_id = Uuid::now_v7();
        let idle_expires_at = input.now + input.idle_timeout;
        let absolute_expires_at = input.now + input.absolute_timeout;
        let expires_at = idle_expires_at.min(absolute_expires_at);
        sqlx::query(
            r#"
            INSERT INTO admin_session
                (id, administrator_id, token_digest, csrf_digest, created_at, last_activity_at,
                 idle_expires_at, absolute_expires_at)
            VALUES ($1, $2, $3, $4, $5, $5, $6, $7)
            "#,
        )
        .bind(session_id)
        .bind(input.administrator_snapshot.id)
        .bind(input.token_digest)
        .bind(input.csrf_digest)
        .bind(input.now)
        .bind(idle_expires_at)
        .bind(absolute_expires_at)
        .execute(&mut *tx)
        .await?;
        for reservation in lease.reservations {
            sqlx::query(
                "UPDATE login_rate_limit SET failure_count = 0, cooldown_until = NULL, updated_at = $2 WHERE bucket_key = $1",
            )
            .bind(&reservation.bucket_key)
            .bind(input.now)
            .execute(&mut *tx)
            .await?;
            sqlx::query("DELETE FROM login_rate_limit_reservation WHERE id = $1")
                .bind(reservation.id)
                .execute(&mut *tx)
                .await?;
        }
        insert_login_attempt(&mut tx, attempt, input.now).await?;
        tx.commit().await?;
        Ok(SessionContext {
            administrator_id: input.administrator_snapshot.id,
            session_id,
            expires_at,
        })
    }

    pub async fn authenticate_session(
        &self,
        token_digest: &[u8],
        now: OffsetDateTime,
        idle_timeout: Duration,
        refresh_interval: Duration,
    ) -> Result<SessionContext, DbError> {
        let mut tx = self.db.begin().await?;
        let row = sqlx::query(
            r#"
            SELECT id, administrator_id, idle_expires_at, absolute_expires_at, last_activity_at
            FROM admin_session
            WHERE token_digest = $1
              AND revoked_at IS NULL
              AND idle_expires_at > $2
              AND absolute_expires_at > $2
            FOR UPDATE
            "#,
        )
        .bind(token_digest)
        .bind(now)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(DbError::NotFound)?;
        let session_id: Uuid = row.try_get("id")?;
        let administrator_id: Uuid = row.try_get("administrator_id")?;
        let absolute_expires_at: OffsetDateTime = row.try_get("absolute_expires_at")?;
        let idle_expires_at: OffsetDateTime = row.try_get("idle_expires_at")?;
        let last_activity_at: OffsetDateTime = row.try_get("last_activity_at")?;
        let mut expires_at = idle_expires_at.min(absolute_expires_at);
        if now - last_activity_at >= refresh_interval {
            let new_idle = (now + idle_timeout).min(absolute_expires_at);
            sqlx::query(
                "UPDATE admin_session SET last_activity_at = $2, idle_expires_at = $3 WHERE id = $1",
            )
            .bind(session_id)
            .bind(now)
            .bind(new_idle)
            .execute(&mut *tx)
            .await?;
            expires_at = new_idle;
        }
        tx.commit().await?;
        Ok(SessionContext {
            administrator_id,
            session_id,
            expires_at,
        })
    }

    pub async fn verify_csrf_digest(
        &self,
        session_id: Uuid,
        csrf_digest: &[u8],
    ) -> Result<bool, DbError> {
        let stored: Option<Vec<u8>> = sqlx::query_scalar(
            "SELECT csrf_digest FROM admin_session WHERE id = $1 AND revoked_at IS NULL",
        )
        .bind(session_id)
        .fetch_optional(self.db.pool())
        .await?;
        Ok(stored.as_deref() == Some(csrf_digest))
    }

    pub async fn revoke_session(
        &self,
        session_id: Uuid,
        now: OffsetDateTime,
    ) -> Result<(), DbError> {
        sqlx::query(
            "UPDATE admin_session SET revoked_at = $2 WHERE id = $1 AND revoked_at IS NULL",
        )
        .bind(session_id)
        .bind(now)
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    pub async fn reserve_rate_limit(
        &self,
        reservations: &[RateLimitReservation],
        now: OffsetDateTime,
    ) -> Result<RateLimitLease, DbError> {
        let mut ordered = reservations.to_vec();
        ordered.sort_by(|left, right| left.bucket_key.cmp(&right.bucket_key));
        let mut tx = self.db.begin().await?;
        for reservation in &ordered {
            reserve_one(&mut tx, reservation, now).await?;
        }
        tx.commit().await?;
        Ok(RateLimitLease {
            reservations: ordered,
        })
    }

    pub async fn release_rate_limit(&self, lease: RateLimitLease) -> Result<(), DbError> {
        let mut tx = self.db.begin().await?;
        for reservation in lease.reservations {
            sqlx::query("DELETE FROM login_rate_limit_reservation WHERE id = $1")
                .bind(reservation.id)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn record_rate_limit_failure(
        &self,
        lease: RateLimitLease,
        failed_kinds: &[RateLimitKind],
        attempt: LoginAttempt<'_>,
        now: OffsetDateTime,
    ) -> Result<(), DbError> {
        let mut tx = self.db.begin().await?;
        if let Some(administrator_id) = attempt.administrator_id {
            lock_administrator_key_share(&mut tx, administrator_id).await?;
        }
        for reservation in lease.reservations {
            if failed_kinds.contains(&reservation.kind) {
                sqlx::query(
                    r#"
                    UPDATE login_rate_limit
                    SET failure_count = failure_count + 1,
                        cooldown_until = CASE
                            WHEN failure_count + 1 >= $2 THEN $3
                            ELSE cooldown_until
                        END,
                        updated_at = $4
                    WHERE bucket_key = $1
                    "#,
                )
                .bind(&reservation.bucket_key)
                .bind(reservation.threshold)
                .bind(now + reservation.cooldown)
                .bind(now)
                .execute(&mut *tx)
                .await?;
            }
            sqlx::query("DELETE FROM login_rate_limit_reservation WHERE id = $1")
                .bind(reservation.id)
                .execute(&mut *tx)
                .await?;
        }
        insert_login_attempt(&mut tx, attempt, now).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn record_login_attempt(
        &self,
        attempt: LoginAttempt<'_>,
        now: OffsetDateTime,
    ) -> Result<(), DbError> {
        let mut tx = self.db.begin().await?;
        insert_login_attempt(&mut tx, attempt, now).await?;
        tx.commit().await?;
        Ok(())
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub enum RateLimitKind {
    Password,
    Shared,
    Entry,
}

#[derive(Clone)]
pub struct RateLimitReservation {
    pub id: Uuid,
    pub kind: RateLimitKind,
    pub bucket_key: String,
    pub threshold: i32,
    pub window: Duration,
    pub cooldown: Duration,
    pub lease: Duration,
}

#[derive(Clone)]
pub struct RateLimitLease {
    reservations: Vec<RateLimitReservation>,
}

pub struct IssueSession<'a> {
    pub administrator_snapshot: &'a AdministratorRecord,
    pub token_digest: &'a [u8],
    pub csrf_digest: &'a [u8],
    pub now: OffsetDateTime,
    pub idle_timeout: Duration,
    pub absolute_timeout: Duration,
}

pub struct UpdatePassword<'a> {
    pub administrator_snapshot: &'a AdministratorRecord,
    pub new_phc: &'a str,
    pub increment_version: bool,
    pub now: OffsetDateTime,
}

#[derive(Clone, Copy)]
pub struct LoginAttempt<'a> {
    pub administrator_id: Option<Uuid>,
    pub account_bucket: &'a str,
    pub entry_bucket: &'a str,
    pub source_bucket: &'a str,
    pub succeeded: bool,
    pub failure_reason: Option<&'a str>,
}

async fn reserve_one(
    tx: &mut Transaction<'_, Postgres>,
    reservation: &RateLimitReservation,
    now: OffsetDateTime,
) -> Result<(), DbError> {
    sqlx::query(
        r#"
        INSERT INTO login_rate_limit (bucket_key, window_started_at)
        VALUES ($1, $2)
        ON CONFLICT (bucket_key) DO NOTHING
        "#,
    )
    .bind(&reservation.bucket_key)
    .bind(now)
    .execute(&mut **tx)
    .await?;
    let row = sqlx::query(
        r#"
        SELECT failure_count, window_started_at, cooldown_until
        FROM login_rate_limit
        WHERE bucket_key = $1
        FOR UPDATE
        "#,
    )
    .bind(&reservation.bucket_key)
    .fetch_one(&mut **tx)
    .await?;
    let mut failure_count: i32 = row.try_get("failure_count")?;
    let window_started_at: OffsetDateTime = row.try_get("window_started_at")?;
    let cooldown_until: Option<OffsetDateTime> = row.try_get("cooldown_until")?;
    if cooldown_until.is_some_and(|cooldown| cooldown > now) {
        return Err(DbError::RateLimited {
            retry_after_seconds: retry_after_seconds(cooldown_until.unwrap(), now),
        });
    }
    if now - window_started_at >= reservation.window {
        failure_count = 0;
        sqlx::query(
            "UPDATE login_rate_limit SET failure_count = 0, window_started_at = $2, cooldown_until = NULL WHERE bucket_key = $1",
        )
        .bind(&reservation.bucket_key)
        .bind(now)
        .execute(&mut **tx)
        .await?;
    }
    let active: i32 = sqlx::query_scalar(
        r#"
        SELECT count(*)::int
        FROM login_rate_limit_reservation
        WHERE bucket_key = $1
          AND leased_until > $2
        "#,
    )
    .bind(&reservation.bucket_key)
    .bind(now)
    .fetch_one(&mut **tx)
    .await?;
    if failure_count + active >= reservation.threshold {
        let leased_until: Option<OffsetDateTime> = sqlx::query_scalar(
            r#"
            SELECT min(leased_until)
            FROM login_rate_limit_reservation
            WHERE bucket_key = $1
              AND leased_until > $2
            "#,
        )
        .bind(&reservation.bucket_key)
        .bind(now)
        .fetch_one(&mut **tx)
        .await?;
        let retry_at = leased_until.unwrap_or(now + reservation.cooldown);
        return Err(DbError::RateLimited {
            retry_after_seconds: retry_after_seconds(retry_at, now),
        });
    }
    sqlx::query(
        r#"
        INSERT INTO login_rate_limit_reservation (id, bucket_key, leased_until, created_at)
        VALUES ($1, $2, $3, $4)
        "#,
    )
    .bind(reservation.id)
    .bind(&reservation.bucket_key)
    .bind(now + reservation.lease)
    .bind(now)
    .execute(&mut **tx)
    .await?;
    sqlx::query("UPDATE login_rate_limit SET updated_at = $2 WHERE bucket_key = $1")
        .bind(&reservation.bucket_key)
        .bind(now)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

fn retry_after_seconds(retry_at: OffsetDateTime, now: OffsetDateTime) -> u64 {
    (retry_at - now).whole_seconds().max(1) as u64
}

async fn lock_administrator(
    tx: &mut Transaction<'_, Postgres>,
    administrator_id: Uuid,
) -> Result<AdministratorRecord, DbError> {
    let row = sqlx::query(
        r#"
        SELECT id, username, password_phc, password_version, revision
        FROM administrator
        WHERE id = $1
        FOR UPDATE
        "#,
    )
    .bind(administrator_id)
    .fetch_one(&mut **tx)
    .await?;
    admin_from_row(&row)
}

async fn lock_administrator_key_share(
    tx: &mut Transaction<'_, Postgres>,
    administrator_id: Uuid,
) -> Result<(), DbError> {
    sqlx::query(
        r#"
        SELECT id
        FROM administrator
        WHERE id = $1
        FOR KEY SHARE
        "#,
    )
    .bind(administrator_id)
    .fetch_one(&mut **tx)
    .await?;
    Ok(())
}

async fn revoke_sessions_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    administrator_id: Uuid,
    now: OffsetDateTime,
) -> Result<(), DbError> {
    sqlx::query(
        "UPDATE admin_session SET revoked_at = $2 WHERE administrator_id = $1 AND revoked_at IS NULL",
    )
    .bind(administrator_id)
    .bind(now)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn compare_administrator(
    locked: &AdministratorRecord,
    snapshot: &AdministratorRecord,
) -> Result<(), DbError> {
    if locked.id == snapshot.id
        && locked.password_version == snapshot.password_version
        && locked.password_phc == snapshot.password_phc
        && locked.revision == snapshot.revision
    {
        Ok(())
    } else {
        Err(DbError::RevisionConflict)
    }
}

async fn insert_login_attempt(
    tx: &mut Transaction<'_, Postgres>,
    attempt: LoginAttempt<'_>,
    now: OffsetDateTime,
) -> Result<(), DbError> {
    sqlx::query(
        r#"
        INSERT INTO login_attempt
            (id, administrator_id, account_bucket, entry_bucket, source_bucket,
             attempted_at, succeeded, failure_reason)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(attempt.administrator_id)
    .bind(attempt.account_bucket)
    .bind(attempt.entry_bucket)
    .bind(attempt.source_bucket)
    .bind(now)
    .bind(attempt.succeeded)
    .bind(attempt.failure_reason)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn admin_from_row(row: &sqlx::postgres::PgRow) -> Result<AdministratorRecord, DbError> {
    Ok(AdministratorRecord {
        id: row.try_get("id")?,
        username: row.try_get("username")?,
        password_phc: row.try_get("password_phc")?,
        password_version: row.try_get("password_version")?,
        revision: row.try_get("revision")?,
    })
}
