use super::*;

impl<G> SubscriptionExecutionService<G>
where
    G: PixivGateway + 'static,
{
    pub(super) async fn save_discovery_items(
        &self,
        ownership: UnitExecutionOwnership,
        context: &PixivRequestContext,
        unit: &pixivarchive_db::SubscriptionRunUnitRecord,
        items: Vec<pixivarchive_domain::pixiv::PixivDiscoveryWork>,
        enabled_artist_ids: Option<&HashSet<i64>>,
        forced: bool,
    ) -> Result<ExecutedPage, JobErrorClass> {
        let mut seen = HashSet::new();
        let mut work_ids = Vec::new();
        let mut filtered_count = 0;
        for item in items {
            if !seen.insert(item.work_id) {
                continue;
            }
            if enabled_artist_ids
                .is_some_and(|artist_ids| !artist_ids.contains(&item.artist.pixiv_id))
            {
                filtered_count += 1;
                continue;
            }
            work_ids.push(item.work_id);
        }
        let mut saved = self
            .save_discovery_work_ids(ownership, context, unit, work_ids, forced)
            .await?;
        saved.ignored_count += filtered_count;
        Ok(saved)
    }

    pub(super) async fn save_discovery_work_ids(
        &self,
        ownership: UnitExecutionOwnership,
        context: &PixivRequestContext,
        unit: &pixivarchive_db::SubscriptionRunUnitRecord,
        work_ids: Vec<i64>,
        forced: bool,
    ) -> Result<ExecutedPage, JobErrorClass> {
        self.save_discovery_work_ids_with_policy(ownership, context, unit, work_ids, forced, false)
            .await
            .map(|(page, _)| page)
    }

    pub(super) async fn save_discovery_work_ids_with_policy(
        &self,
        ownership: UnitExecutionOwnership,
        context: &PixivRequestContext,
        unit: &pixivarchive_db::SubscriptionRunUnitRecord,
        work_ids: Vec<i64>,
        forced: bool,
        continue_after_transient_failure: bool,
    ) -> Result<(ExecutedPage, bool), JobErrorClass> {
        let mut seen = HashSet::new();
        let mut discovered = 0;
        let mut ignored = 0;
        let mut retry_pending = false;
        let rule_document = if forced {
            None
        } else {
            unit_rule_document(unit)?
        };
        let account_id = unit.pixiv_account_id;
        for work_id in work_ids {
            if !seen.insert(work_id) {
                continue;
            }
            let request = ProcessPixivWork {
                context,
                account_id,
                pixiv_work_id: work_id,
                deletion_marker_policy: DeletionMarkerPolicy::Block,
                forced,
                rule_document: rule_document.as_ref(),
                discovery: WorkDiscoveryContext::default(),
                download_priority: ownership.download_priority(),
            };
            let processed = match ownership {
                UnitExecutionOwnership::Synchronous => self.processor.process(request).await,
                UnitExecutionOwnership::Job { lease, .. } => {
                    self.processor.process_for_job(lease, request).await
                }
            };
            match processed {
                Ok(ProcessedPixivWork::MetadataSaved { .. })
                | Ok(ProcessedPixivWork::DownloadQueued { .. }) => discovered += 1,
                Ok(ProcessedPixivWork::Ignored)
                | Ok(ProcessedPixivWork::BlockedByDeletionMarker) => ignored += 1,
                Err(error) if error.is_permanent_pixiv() => ignored += 1,
                Err(error)
                    if continue_after_transient_failure
                        && matches!(
                            error.error_class(),
                            JobErrorClass::Network
                                | JobErrorClass::Server
                                | JobErrorClass::RateLimit
                        ) =>
                {
                    tracing::warn!(
                        source_key = %unit.source_key,
                        pixiv_work_id = work_id,
                        error_class = ?error.error_class(),
                        "Pixiv bookmark work will be retried by the next full synchronization"
                    );
                    ignored += 1;
                    retry_pending = true;
                }
                Err(error) => return Err(error.error_class()),
            }
        }
        Ok((
            ExecutedPage {
                discovered_count: discovered,
                ignored_count: ignored,
                cursor_value: None,
            },
            retry_pending,
        ))
    }
}
