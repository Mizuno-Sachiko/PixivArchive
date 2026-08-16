use pixivarchive_application::{
    jobs::{QueueQuotaError, QueueQuotaWeights, RetryDecision, RetryPolicy, RetryPolicyError},
    settings::{
        EffectiveSettings, QueueQuotaWeights as SettingsQueueQuotaWeights, QueueSettings,
        RetrySettings,
    },
};
use pixivarchive_domain::job::{
    ClaimedJob, JobErrorClass, JobKind, JobPriority, JobPriorityMapping, JobPriorityPolicy,
    JobPriorityPolicyError, JobQuotaSelection,
};
use serde_json::json;
use std::num::NonZeroU16;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

#[test]
fn job_kind_uses_strict_stable_strings() {
    assert_eq!(
        JobKind::parse("scheduled_collection").unwrap(),
        JobKind::ScheduledCollection
    );
    assert!(JobKind::parse(" scheduled_collection").is_err());
    assert!(JobKind::parse("unknown_job").is_err());

    let encoded = serde_json::to_string(&JobKind::ImportWork).unwrap();
    assert_eq!(encoded, "\"import_work\"");
    assert_eq!(
        serde_json::from_str::<JobKind>("\"import_artist\"").unwrap(),
        JobKind::ImportArtist
    );
    assert!(serde_json::from_str::<JobKind>("\"import-work\"").is_err());
}

#[test]
fn job_priority_policy_requires_one_mapping_for_every_job_kind() {
    let mut mappings = JobPriorityPolicy::default().mappings();
    mappings
        .iter_mut()
        .find(|mapping| mapping.job_kind == JobKind::ImportWork)
        .unwrap()
        .priority = JobPriority::Immediate;
    let policy = JobPriorityPolicy::from_mappings(&mappings).unwrap();
    assert_eq!(
        policy.priority_for(JobKind::ImportWork),
        JobPriority::Immediate
    );

    let missing = &mappings[..mappings.len() - 1];
    assert_eq!(
        JobPriorityPolicy::from_mappings(missing).unwrap_err(),
        JobPriorityPolicyError::Missing(JobKind::PurgeTrash)
    );

    mappings.push(JobPriorityMapping {
        job_kind: JobKind::ImportWork,
        priority: JobPriority::ManualImport,
    });
    assert_eq!(
        JobPriorityPolicy::from_mappings(&mappings).unwrap_err(),
        JobPriorityPolicyError::Duplicate(JobKind::ImportWork)
    );
}

#[test]
fn retry_policy_uses_default_backoff_sequence_then_terminal_state() {
    let policy = RetryPolicy::default();

    assert_eq!(
        policy.next_attempt(JobErrorClass::Network, 1, None),
        RetryDecision::RetryAfter(Duration::seconds(60))
    );
    assert_eq!(
        policy.next_attempt(JobErrorClass::Server, 2, None),
        RetryDecision::RetryAfter(Duration::seconds(300))
    );
    assert_eq!(
        policy.next_attempt(JobErrorClass::Network, 3, None),
        RetryDecision::RetryAfter(Duration::seconds(1_200))
    );
    assert_eq!(
        policy.next_attempt(JobErrorClass::Server, 4, None),
        RetryDecision::RetryAfter(Duration::seconds(3_600))
    );
    assert_eq!(
        policy.next_attempt(JobErrorClass::Network, 5, None),
        RetryDecision::DoNotRetry
    );
}

#[test]
fn retry_policy_rejects_empty_or_non_positive_backoff_sequences() {
    assert_eq!(
        RetryPolicy::new(vec![]).unwrap_err(),
        RetryPolicyError::InvalidBackoff
    );
    assert_eq!(
        RetryPolicy::new(vec![Duration::seconds(0)]).unwrap_err(),
        RetryPolicyError::InvalidBackoff
    );
}

#[test]
fn retry_policy_respects_rate_limit_retry_after() {
    let policy = RetryPolicy::default();

    assert_eq!(
        policy.next_attempt(JobErrorClass::RateLimit, 1, Some(Duration::seconds(0))),
        RetryDecision::RetryAfter(Duration::seconds(0))
    );
    assert_eq!(
        policy.next_attempt(JobErrorClass::RateLimit, 1, Some(Duration::seconds(42))),
        RetryDecision::RetryAfter(Duration::seconds(42))
    );
    assert_eq!(
        policy.next_attempt(JobErrorClass::RateLimit, 1, Some(Duration::seconds(-1))),
        RetryDecision::RetryAfter(Duration::seconds(60))
    );
}

#[test]
fn retry_policy_marks_account_and_terminal_errors_without_ordinary_retry() {
    let policy = RetryPolicy::default();

    assert_eq!(
        policy.next_attempt(JobErrorClass::CredentialInvalid, 1, None),
        RetryDecision::BlockAccount
    );
    assert_eq!(
        policy.next_attempt(JobErrorClass::Permanent, 1, None),
        RetryDecision::DoNotRetry
    );
}

#[test]
fn quota_weights_reject_zero_weight() {
    assert_eq!(
        QueueQuotaWeights::new(1, 0, 1, 1).unwrap_err(),
        QueueQuotaError::ZeroWeight
    );
}

#[test]
fn queue_quota_default_matches_the_persisted_settings_default() {
    let settings = SettingsQueueQuotaWeights::default();

    assert_eq!(
        QueueQuotaWeights::default(),
        QueueQuotaWeights::from(&settings)
    );
}

#[test]
fn quota_weights_rotate_all_four_priority_classes_by_weight() {
    let mut rotation = QueueQuotaWeights::new(2, 1, 1, 1).unwrap().rotation();

    assert_eq!(
        rotation.next_selection(),
        JobQuotaSelection::with_fallback(JobPriority::ManualImport)
    );
    assert_eq!(
        rotation.next_selection(),
        JobQuotaSelection::with_fallback(JobPriority::Immediate)
    );
    assert_eq!(
        rotation.next_selection(),
        JobQuotaSelection::with_fallback(JobPriority::Immediate)
    );
    assert_eq!(
        rotation.next_selection(),
        JobQuotaSelection::with_fallback(JobPriority::ScheduledCollection)
    );
    assert_eq!(
        rotation.next_selection(),
        JobQuotaSelection::with_fallback(JobPriority::BackgroundMaintenance)
    );
    assert_eq!(
        rotation.next_selection(),
        JobQuotaSelection::with_fallback(JobPriority::ManualImport)
    );
}

#[test]
fn retry_policy_from_effective_settings_changes_terminal_retry_count_and_delays() {
    let settings = EffectiveSettings {
        retry: RetrySettings {
            network_backoff_seconds: vec![7],
        },
        ..EffectiveSettings::default()
    };
    let policy = RetryPolicy::from_effective_settings(&settings).unwrap();

    assert_eq!(
        policy.next_attempt(JobErrorClass::Network, 1, None),
        RetryDecision::RetryAfter(Duration::seconds(7))
    );
    assert_eq!(
        policy.next_attempt(JobErrorClass::Network, 2, None),
        RetryDecision::DoNotRetry
    );
}

#[test]
fn queue_quota_weights_from_effective_settings_change_claim_order() {
    let settings = EffectiveSettings {
        queue: QueueSettings {
            quota_weights: SettingsQueueQuotaWeights {
                immediate: NonZeroU16::new(1).unwrap(),
                manual_import: NonZeroU16::new(1).unwrap(),
                scheduled_collection: NonZeroU16::new(3).unwrap(),
                background_maintenance: NonZeroU16::new(1).unwrap(),
            },
            ..QueueSettings::default()
        },
        ..EffectiveSettings::default()
    };
    let mut rotation = QueueQuotaWeights::from(&settings.queue.quota_weights).rotation();

    assert_eq!(
        rotation.next_selection().priority_values()[0],
        "manual_import"
    );
    assert_eq!(rotation.next_selection().priority_values()[0], "immediate");
    assert_eq!(
        rotation.next_selection().priority_values()[0],
        "scheduled_collection"
    );
    assert_eq!(
        rotation.next_selection().priority_values()[0],
        "scheduled_collection"
    );
    assert_eq!(
        rotation.next_selection().priority_values()[0],
        "scheduled_collection"
    );
    assert_eq!(
        rotation.next_selection().priority_values()[0],
        "background_maintenance"
    );
}

#[test]
fn claimed_job_carries_attempt_and_lease_identity() {
    let lease_owner = Uuid::now_v7();
    let lease_expires_at = OffsetDateTime::now_utc() + Duration::minutes(5);

    let claimed = ClaimedJob {
        id: Uuid::now_v7(),
        priority: JobPriority::Immediate,
        kind: "scheduled_collection".to_owned(),
        payload: json!({}),
        state: pixivarchive_domain::job::JobState::Running,
        resource_revision: 3,
        attempt_number: 2,
        lease_owner,
        lease_expires_at,
    };

    assert_eq!(claimed.attempt_number, 2);
    assert_eq!(claimed.lease_owner, lease_owner);
    assert_eq!(claimed.lease_expires_at, lease_expires_at);
}
