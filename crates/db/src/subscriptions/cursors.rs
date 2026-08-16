use super::*;

impl SubscriptionRepository {
    pub async fn list_cursors(
        &self,
        subscription_id: Uuid,
    ) -> Result<Vec<SubscriptionCursorRecord>, DbError> {
        let rows = sqlx::query(
            r#"
            SELECT cursor_kind, source_key, cursor_value
            FROM subscription_cursor
            WHERE subscription_id = $1
            ORDER BY cursor_kind, source_key
            "#,
        )
        .bind(subscription_id)
        .fetch_all(self.db.pool())
        .await?;
        Ok(rows
            .iter()
            .map(|row| SubscriptionCursorRecord {
                cursor_kind: row.get("cursor_kind"),
                source_key: row.get("source_key"),
                value: row.get::<Json<Value>, _>("cursor_value").0,
            })
            .collect())
    }

    pub async fn cursor(
        &self,
        subscription_id: Uuid,
        cursor_kind: &str,
    ) -> Result<Option<Value>, DbError> {
        let row = sqlx::query(
            "SELECT cursor_value FROM subscription_cursor WHERE subscription_id = $1 AND cursor_kind = $2 AND source_key = 'default'",
        )
        .bind(subscription_id)
        .bind(cursor_kind)
        .fetch_optional(self.db.pool())
        .await?;
        Ok(row.map(|row| row.get::<Json<Value>, _>("cursor_value").0))
    }

    pub async fn source_cursor(
        &self,
        subscription_id: Uuid,
        cursor_kind: &str,
        source_key: &str,
    ) -> Result<Option<Value>, DbError> {
        let row = sqlx::query(
            "SELECT cursor_value FROM subscription_cursor WHERE subscription_id = $1 AND cursor_kind = $2 AND source_key = $3",
        )
        .bind(subscription_id)
        .bind(cursor_kind)
        .bind(source_key)
        .fetch_optional(self.db.pool())
        .await?;
        Ok(row.map(|row| row.get::<Json<Value>, _>("cursor_value").0))
    }

    pub async fn save_cursor(
        &self,
        subscription_id: Uuid,
        cursor_kind: &str,
        cursor_value: Value,
    ) -> Result<(), DbError> {
        sqlx::query(
            r#"
            INSERT INTO subscription_cursor (id, subscription_id, cursor_kind, source_key, cursor_value)
            VALUES ($1, $2, $3, 'default', $4)
            ON CONFLICT (subscription_id, cursor_kind, source_key)
            DO UPDATE SET cursor_value = excluded.cursor_value,
                          updated_at = now()
            "#,
        )
        .bind(Uuid::now_v7())
        .bind(subscription_id)
        .bind(cursor_kind)
        .bind(Json(cursor_value))
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    pub async fn save_source_cursor(
        &self,
        subscription_id: Uuid,
        cursor_kind: &str,
        source_key: &str,
        cursor_value: Value,
    ) -> Result<(), DbError> {
        sqlx::query(
            r#"
            INSERT INTO subscription_cursor (id, subscription_id, cursor_kind, source_key, cursor_value)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (subscription_id, cursor_kind, source_key)
            DO UPDATE SET cursor_value = excluded.cursor_value,
                          updated_at = now()
            "#,
        )
        .bind(Uuid::now_v7())
        .bind(subscription_id)
        .bind(cursor_kind)
        .bind(source_key)
        .bind(Json(cursor_value))
        .execute(self.db.pool())
        .await?;
        Ok(())
    }
}
