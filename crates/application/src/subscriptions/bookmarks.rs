use super::*;

impl<G> SubscriptionExecutionService<G>
where
    G: PixivGateway + 'static,
{
    pub(super) async fn execute_bookmarks_unit(
        &self,
        ownership: UnitExecutionOwnership,
        context: &PixivRequestContext,
        unit: &pixivarchive_db::SubscriptionRunUnitRecord,
    ) -> Result<ExecutedPage, JobErrorClass> {
        let account_id = unit.pixiv_account_id;
        let mode = enum_field::<PixivBookmarksMode>(&unit.params_snapshot, "mode")?;
        let now = OffsetDateTime::now_utc();
        let full_reconcile_hours = unit
            .params_snapshot
            .get("full_reconcile_hours")
            .and_then(Value::as_i64)
            .filter(|hours| *hours > 0)
            .unwrap_or(24);
        let last_full = self
            .bookmarks
            .last_full_reconciled_at(account_id)
            .await
            .map_err(|error| database_error_class(&error))?;
        let full = unit.cursor_kind == "backfill"
            || last_full.is_none_or(|completed_at| {
                now - completed_at >= Duration::hours(full_reconcile_hours)
            });
        let incremental_pages = subscription_schedule(&unit.schedule)?.lookback_pages.max(1);
        let mut discovered_count = 0;
        let mut ignored_count = 0;
        let mut all_seen_bookmarks = Vec::new();
        let mut discovery_work_ids = Vec::new();

        for visibility in [
            pixivarchive_domain::pixiv::PixivBookmarkVisibility::Public,
            pixivarchive_domain::pixiv::PixivBookmarkVisibility::Private,
        ] {
            let mut offset = 0;
            let mut pages = 0;
            loop {
                let response = self
                    .gateway
                    .bookmarks(
                        context,
                        PixivBookmarksRequest {
                            user_id: context.user_id(),
                            visibility,
                            mode,
                            tag: None,
                            offset,
                        },
                    )
                    .await
                    .map_err(|error| {
                        tracing::warn!(
                            source_key = %unit.source_key,
                            offset,
                            error = %error,
                            "Pixiv bookmarks request failed"
                        );
                        pixiv_error_class(error.class())
                    })?;
                let next_offset = response.value.next_cursor.map(|cursor| cursor.offset);
                all_seen_bookmarks.extend(response.value.items.iter().map(|work_id| {
                    PixivBookmarkSyncEntry {
                        pixiv_work_id: *work_id,
                        visibility,
                    }
                }));
                discovery_work_ids.extend(response.value.items);
                pages += 1;
                if !full && pages >= incremental_pages {
                    break;
                }
                let Some(next_offset) = next_offset else {
                    break;
                };
                offset = next_offset;
            }
        }

        let (saved, retry_pending) = self
            .save_discovery_work_ids_with_policy(
                ownership,
                context,
                unit,
                discovery_work_ids,
                true,
                true,
            )
            .await?;
        discovered_count += saved.discovered_count;
        ignored_count += saved.ignored_count;

        if full && !retry_pending {
            all_seen_bookmarks.sort_by_key(|entry| entry.pixiv_work_id);
            match ownership {
                UnitExecutionOwnership::Synchronous => {
                    self.bookmarks
                        .reconcile_full(account_id, &all_seen_bookmarks, now)
                        .await
                }
                UnitExecutionOwnership::Job { lease, .. } => {
                    self.bookmarks
                        .reconcile_full_for_job(lease, account_id, &all_seen_bookmarks, now)
                        .await
                }
            }
            .map_err(|error| database_error_class(&error))?;
        }
        Ok(ExecutedPage {
            discovered_count,
            ignored_count,
            cursor_value: Some(json!({
                "completed_at": now,
                "full_reconcile": full && !retry_pending,
                "retry_pending": retry_pending,
            })),
        })
    }
}
