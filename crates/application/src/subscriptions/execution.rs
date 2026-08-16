use super::*;

impl<G> SubscriptionExecutionService<G>
where
    G: PixivGateway + 'static,
{
    pub async fn execute(
        &self,
        request: SubscriptionRunRequest,
    ) -> Result<SubscriptionExecutionResult, DbError> {
        let units = self
            .repository
            .list_units_for_run(request.subscription_run_id)
            .await?;
        let mut discovered_count = 0;
        let mut ignored_count = 0;
        let mut status = DomainRunStatus::Succeeded;
        let mut error_class = None;
        let mut error_message = None;
        for unit in units {
            let result = self
                .execute_unit(SubscriptionUnitRequest {
                    context: request.context.clone(),
                    unit_id: unit.id,
                })
                .await?;
            discovered_count += result.discovered_count;
            ignored_count += result.ignored_count;
            if result.status == DomainRunStatus::Failed {
                status = DomainRunStatus::Failed;
                if error_class.is_none() {
                    error_class = result.error_class;
                    error_message = result.error_message;
                }
            }
        }
        Ok(SubscriptionExecutionResult {
            status,
            discovered_count,
            ignored_count,
            error_class,
            error_message,
        })
    }

    pub async fn execute_unit(
        &self,
        request: SubscriptionUnitRequest,
    ) -> Result<SubscriptionExecutionResult, DbError> {
        let unit_id = request.unit_id;
        let attempt = self.execute_unit_attempt(request).await?;
        if let Some(completion) = attempt.completion {
            self.repository.finish_unit(completion).await?;
        } else if let Some(error_class) = attempt.result.error_class.as_deref() {
            self.finalize_unit_failure(
                unit_id,
                error_class,
                attempt.result.error_message.as_deref(),
            )
            .await?;
        }
        Ok(attempt.result)
    }

    pub async fn execute_unit_attempt(
        &self,
        request: SubscriptionUnitRequest,
    ) -> Result<SubscriptionUnitAttemptResult, DbError> {
        self.execute_unit_attempt_with_ownership(UnitExecutionOwnership::Synchronous, request)
            .await
    }

    pub async fn execute_unit_job_attempt(
        &self,
        lease: JobLease,
        priority: JobPriority,
        request: SubscriptionUnitRequest,
    ) -> Result<SubscriptionUnitAttemptResult, DbError> {
        self.execute_unit_attempt_with_ownership(
            UnitExecutionOwnership::Job { lease, priority },
            request,
        )
        .await
    }

    async fn execute_unit_attempt_with_ownership(
        &self,
        ownership: UnitExecutionOwnership,
        request: SubscriptionUnitRequest,
    ) -> Result<SubscriptionUnitAttemptResult, DbError> {
        match ownership {
            UnitExecutionOwnership::Synchronous => {
                self.repository.mark_unit_running(request.unit_id).await?;
            }
            UnitExecutionOwnership::Job { lease, .. } => {
                self.repository
                    .mark_unit_running_job(lease, request.unit_id)
                    .await?;
            }
        }
        let unit = self.repository.load_unit(request.unit_id).await?;
        let result = match unit.kind {
            SubscriptionKind::Ranking => {
                self.execute_ranking_unit(ownership, &request.context, &unit)
                    .await
            }
            SubscriptionKind::Following => {
                self.execute_following_unit(ownership, &request.context, &unit)
                    .await
            }
            SubscriptionKind::Bookmarks => {
                self.execute_bookmarks_unit(ownership, &request.context, &unit)
                    .await
            }
        };
        match result {
            Ok(executed) => {
                let completion = FinishSubscriptionRunUnit {
                    unit_id: unit.id,
                    state: DomainRunStatus::Succeeded,
                    discovered_count: executed.discovered_count,
                    ignored_count: executed.ignored_count,
                    error_class: None,
                    error_message: None,
                    cursor_kind: unit.cursor_kind,
                    source_key: unit.source_key,
                    cursor_value: executed.cursor_value,
                };
                Ok(SubscriptionUnitAttemptResult {
                    result: SubscriptionExecutionResult {
                        status: DomainRunStatus::Succeeded,
                        discovered_count: executed.discovered_count,
                        ignored_count: executed.ignored_count,
                        error_class: None,
                        error_message: None,
                    },
                    completion: Some(completion),
                })
            }
            Err(error_class) => {
                let error_message = subscription_error_message(error_class).to_owned();
                match ownership {
                    UnitExecutionOwnership::Synchronous => {
                        self.repository
                            .record_unit_attempt_failure(
                                unit.id,
                                error_class.as_str(),
                                Some(&error_message),
                            )
                            .await?;
                    }
                    UnitExecutionOwnership::Job { lease, .. } => {
                        self.repository
                            .record_unit_attempt_failure_job(
                                lease,
                                unit.id,
                                error_class.as_str(),
                                Some(&error_message),
                            )
                            .await?;
                    }
                }
                Ok(SubscriptionUnitAttemptResult {
                    result: SubscriptionExecutionResult {
                        status: DomainRunStatus::Failed,
                        discovered_count: 0,
                        ignored_count: 0,
                        error_class: Some(error_class.as_str().to_owned()),
                        error_message: Some(error_message),
                    },
                    completion: None,
                })
            }
        }
    }

    pub async fn finalize_unit_failure(
        &self,
        unit_id: Uuid,
        error_class: &str,
        error_message: Option<&str>,
    ) -> Result<(), DbError> {
        let unit = self.repository.load_unit(unit_id).await?;
        if unit.state == DomainRunStatus::Failed {
            return Ok(());
        }
        self.repository
            .finish_unit(FinishSubscriptionRunUnit {
                unit_id: unit.id,
                state: DomainRunStatus::Failed,
                discovered_count: 0,
                ignored_count: 0,
                error_class: Some(error_class.to_owned()),
                error_message: error_message.map(str::to_owned),
                cursor_kind: unit.cursor_kind,
                source_key: unit.source_key,
                cursor_value: None,
            })
            .await?;
        Ok(())
    }
}
