use super::*;

impl<G> SubscriptionExecutionService<G>
where
    G: PixivGateway + 'static,
{
    pub(super) async fn execute_ranking_unit(
        &self,
        ownership: UnitExecutionOwnership,
        context: &PixivRequestContext,
        unit: &pixivarchive_db::SubscriptionRunUnitRecord,
    ) -> Result<ExecutedPage, JobErrorClass> {
        let mode = enum_field::<PixivRankingMode>(&unit.params_snapshot, "mode")?;
        let content = enum_field::<PixivRankingContent>(&unit.params_snapshot, "content")?;
        let page_size = page_size(&unit.params_snapshot, 50);
        let max_rank = ranking_max_rank(&unit.params_snapshot)?;
        let max_page = max_rank.div_ceil(page_size).max(1);
        let period_count = subscription_schedule(&unit.schedule)?
            .lookback_pages
            .saturating_add(1);
        let mut requested_date = (unit.cursor_kind == "backfill")
            .then(|| ranking_cursor_date(unit.cursor_snapshot.as_ref()))
            .flatten();
        let mut latest_date = None;
        let mut oldest_date = None;
        let mut seen = HashSet::new();
        let mut discovered = 0;
        let mut ignored = 0;
        let rule_document = unit_rule_document(unit)?;
        let account_id = unit.pixiv_account_id;
        let revision_source = collection_source_context(unit);
        for period_index in 0..period_count {
            if period_index > 0 {
                let Some(previous_date) = oldest_date.and_then(Date::previous_day) else {
                    break;
                };
                requested_date = Some(previous_date);
            }
            let mut period_date = requested_date;
            for page in 1..=max_page {
                let response = self
                    .gateway
                    .ranking_page(
                        context,
                        PixivRankingRequest {
                            mode,
                            content,
                            date: period_date,
                            page,
                        },
                    )
                    .await
                    .map_err(|error| {
                        tracing::warn!(
                            source_key = %unit.source_key,
                            page,
                            error = %error,
                            "Pixiv ranking request failed"
                        );
                        pixiv_error_class(error.class())
                    })?;
                period_date = response.value.date.or(period_date);
                latest_date = latest_date.or(period_date);
                for entry in response.value.items {
                    if entry.rank > max_rank || !seen.insert(entry.work.work_id) {
                        continue;
                    }
                    let process_request = ProcessPixivWork {
                        context,
                        account_id,
                        pixiv_work_id: entry.work.work_id,
                        deletion_marker_policy: DeletionMarkerPolicy::Block,
                        forced: false,
                        rule_document: rule_document.as_ref(),
                        discovery: WorkDiscoveryContext {
                            ranking_rank: Some(entry.rank),
                            ranking_date: period_date.map(ranking_date_time),
                        },
                        revision_source: Some(revision_source.clone()),
                        download_priority: ownership.download_priority(),
                    };
                    let processed = match ownership {
                        UnitExecutionOwnership::Synchronous => {
                            self.processor.process(process_request).await
                        }
                        UnitExecutionOwnership::Job { lease, .. } => {
                            self.processor.process_for_job(lease, process_request).await
                        }
                    };
                    match processed {
                        Ok(ProcessedPixivWork::MetadataSaved { .. })
                        | Ok(ProcessedPixivWork::DownloadQueued { .. }) => {
                            let ranking_entry = pixivarchive_db::RecordRankingUnitEntry {
                                run_id: unit.subscription_run_id,
                                unit_id: unit.id,
                                source_key: unit.source_key.clone(),
                                pixiv_work_id: entry.work.work_id,
                                rank: entry.rank,
                                score: json!({
                                    "mode": mode,
                                    "content": content,
                                    "date": period_date,
                                    "page": page,
                                }),
                            };
                            let recorded = match ownership {
                                UnitExecutionOwnership::Synchronous => {
                                    self.repository
                                        .record_ranking_unit_entry(ranking_entry)
                                        .await
                                }
                                UnitExecutionOwnership::Job { lease, .. } => {
                                    self.repository
                                        .record_ranking_unit_entry_for_job(lease, ranking_entry)
                                        .await
                                }
                            }
                            .map_err(|error| database_error_class(&error))?;
                            if recorded {
                                discovered += 1;
                            }
                        }
                        Ok(ProcessedPixivWork::Ignored)
                        | Ok(ProcessedPixivWork::BlockedByDeletionMarker) => ignored += 1,
                        Err(error) if error.is_permanent_pixiv() => ignored += 1,
                        Err(error) => {
                            tracing::warn!(
                                source_key = %unit.source_key,
                                pixiv_work_id = entry.work.work_id,
                                error = ?error,
                                "Pixiv ranking work processing failed"
                            );
                            return Err(error.error_class());
                        }
                    }
                }
                if response.value.next_cursor.is_none() {
                    break;
                }
            }
            oldest_date = period_date;
        }
        let cursor_date = if unit.cursor_kind == "backfill" {
            oldest_date.and_then(Date::previous_day)
        } else {
            latest_date
        };
        let cursor_value = serde_json::to_value(PixivRankingCursor {
            mode,
            content,
            date: cursor_date,
            page: 1,
        })
        .map(Some)
        .map_err(|_| JobErrorClass::Permanent)?;
        Ok(ExecutedPage {
            discovered_count: discovered,
            ignored_count: ignored,
            cursor_value,
        })
    }
}
