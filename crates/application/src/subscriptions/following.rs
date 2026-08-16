use super::*;

impl<G> SubscriptionExecutionService<G>
where
    G: PixivGateway + 'static,
{
    pub(super) async fn execute_following_unit(
        &self,
        ownership: UnitExecutionOwnership,
        context: &PixivRequestContext,
        unit: &pixivarchive_db::SubscriptionRunUnitRecord,
    ) -> Result<ExecutedPage, JobErrorClass> {
        let account_id = unit.pixiv_account_id;
        let authors = match ownership {
            UnitExecutionOwnership::Synchronous => {
                self.following.refresh(account_id, context).await
            }
            UnitExecutionOwnership::Job { lease, .. } => {
                self.following
                    .refresh_for_job(lease, account_id, context)
                    .await
            }
        }
        .map_err(following_error_class)?;
        let enabled_artist_ids: HashSet<_> = authors
            .into_iter()
            .filter(|author| author.enabled)
            .map(|author| author.pixiv_artist_id)
            .collect();
        if enabled_artist_ids.is_empty() {
            return Ok(ExecutedPage {
                discovered_count: 0,
                ignored_count: 0,
                cursor_value: None,
            });
        }
        let full = unit.cursor_kind == "backfill";
        let last_page = cursor_page(unit.cursor_snapshot.as_ref(), 1)?;
        let first_page = if full {
            1
        } else {
            last_page
                .saturating_sub(subscription_schedule(&unit.schedule)?.lookback_pages)
                .max(1)
        };
        let mode = enum_field::<PixivFollowLatestMode>(&unit.params_snapshot, "mode")?;
        let mut items = Vec::new();
        let mut page = first_page;
        let next_cursor = loop {
            let response = self
                .gateway
                .follow_latest(
                    context,
                    PixivFollowLatestRequest {
                        source: PixivFollowLatestSource::Following,
                        mode,
                        tag: None,
                        language: "zh".to_owned(),
                        page,
                    },
                )
                .await
                .map_err(|error| {
                    tracing::warn!(
                        source_key = %unit.source_key,
                        page,
                        error = %error,
                        "Pixiv following request failed"
                    );
                    pixiv_error_class(error.class())
                })?;
            items.extend(response.value.items);
            let next_cursor = response.value.next_cursor;
            if !full && page >= last_page {
                break next_cursor;
            }
            let Some(cursor) = next_cursor else {
                break None;
            };
            if cursor.page <= page {
                return Err(JobErrorClass::Permanent);
            }
            page = cursor.page;
        };
        let mut saved = self
            .save_discovery_items(
                ownership,
                context,
                unit,
                items,
                Some(&enabled_artist_ids),
                true,
            )
            .await?;
        match ownership {
            UnitExecutionOwnership::Synchronous => {
                self.following
                    .mark_enabled_collected(account_id, OffsetDateTime::now_utc())
                    .await
            }
            UnitExecutionOwnership::Job { lease, .. } => {
                self.following
                    .mark_enabled_collected_for_job(lease, account_id, OffsetDateTime::now_utc())
                    .await
            }
        }
        .map_err(|error| database_error_class(&error))?;
        saved.cursor_value = next_cursor
            .map(|cursor| serde_json::to_value(cursor).map_err(|_| JobErrorClass::Permanent))
            .transpose()?;
        Ok(saved)
    }
}
