use pixivarchive_application::rules::{
    PublishRuleVersionRequest, RuleService, RuleServiceError, SaveRuleDraftRequest,
};
use pixivarchive_domain::rule::{RuleAction, RuleDefinitionV1};
use pixivarchive_test_support::LockedDb;
use uuid::Uuid;

const RULE_VERSIONS_DB_LOCK_ID: i64 = 709020004;

#[tokio::test]
async fn create_rule_starts_with_one_editable_draft() {
    let locked = LockedDb::new(RULE_VERSIONS_DB_LOCK_ID).await;
    let service = RuleService::new(locked.db.clone());

    let rule = service
        .create_rule("main", RuleAction::Download)
        .await
        .unwrap();
    let draft = service.load_draft(rule.id).await.unwrap().unwrap();

    assert!(rule.current_version.is_none());
    assert!(rule.has_draft);
    assert_eq!(draft.base_version, None);
    assert_eq!(
        RuleDefinitionV1::parse(draft.definition).unwrap().id,
        rule.id
    );
}

#[tokio::test]
async fn draft_updates_require_the_current_revision() {
    let locked = LockedDb::new(RULE_VERSIONS_DB_LOCK_ID).await;
    let service = RuleService::new(locked.db.clone());
    let rule = service
        .create_rule("main", RuleAction::Download)
        .await
        .unwrap();
    let draft = service.load_draft(rule.id).await.unwrap().unwrap();
    let definition = definition(rule.id, "main", RuleAction::Ignore);

    let conflict = service
        .save_draft(SaveRuleDraftRequest {
            rule_id: rule.id,
            expected_revision: Some(draft.revision + 1),
            base_version: None,
            definition: serde_json::to_value(&definition).unwrap(),
        })
        .await
        .unwrap_err();
    assert!(matches!(conflict, RuleServiceError::RevisionConflict));

    let saved = service
        .save_draft(SaveRuleDraftRequest {
            rule_id: rule.id,
            expected_revision: Some(draft.revision),
            base_version: None,
            definition: serde_json::to_value(definition).unwrap(),
        })
        .await
        .unwrap();
    assert_eq!(saved.revision, draft.revision + 1);
}

#[tokio::test]
async fn publishing_creates_an_immutable_version_and_clears_the_draft() {
    let locked = LockedDb::new(RULE_VERSIONS_DB_LOCK_ID).await;
    let service = RuleService::new(locked.db.clone());
    let rule = service
        .create_rule("main", RuleAction::Download)
        .await
        .unwrap();
    let draft = service.load_draft(rule.id).await.unwrap().unwrap();

    let published = service
        .publish_version(PublishRuleVersionRequest {
            rule_id: rule.id,
            base_version: None,
            expected_draft_revision: draft.revision,
            created_by: None,
        })
        .await
        .unwrap();

    assert_eq!(published.version, 1);
    assert!(service.load_draft(rule.id).await.unwrap().is_none());
    assert_eq!(
        service.rule(rule.id).await.unwrap().current_version,
        Some(1)
    );
    let mutation = sqlx::query("UPDATE rule_version SET version = 2 WHERE id = $1")
        .bind(published.id)
        .execute(locked.db.pool())
        .await;
    assert!(mutation.is_err());
}

#[tokio::test]
async fn a_published_rule_can_begin_and_publish_a_new_revision() {
    let locked = LockedDb::new(RULE_VERSIONS_DB_LOCK_ID).await;
    let service = RuleService::new(locked.db.clone());
    let rule = service
        .create_rule("main", RuleAction::Download)
        .await
        .unwrap();
    let first_draft = service.load_draft(rule.id).await.unwrap().unwrap();
    service
        .publish_version(PublishRuleVersionRequest {
            rule_id: rule.id,
            base_version: None,
            expected_draft_revision: first_draft.revision,
            created_by: None,
        })
        .await
        .unwrap();

    let second_draft = service
        .save_draft(SaveRuleDraftRequest {
            rule_id: rule.id,
            expected_revision: None,
            base_version: Some(1),
            definition: serde_json::to_value(definition(
                rule.id,
                "main updated",
                RuleAction::MetadataOnly,
            ))
            .unwrap(),
        })
        .await
        .unwrap();
    let second = service
        .publish_version(PublishRuleVersionRequest {
            rule_id: rule.id,
            base_version: Some(1),
            expected_draft_revision: second_draft.revision,
            created_by: None,
        })
        .await
        .unwrap();

    assert_eq!(second.version, 2);
    let current = service.rule(rule.id).await.unwrap();
    assert_eq!(current.name, "main updated");
    assert_eq!(current.match_action, RuleAction::MetadataOnly);
}

#[tokio::test]
async fn deletion_uses_optimistic_revision_control() {
    let locked = LockedDb::new(RULE_VERSIONS_DB_LOCK_ID).await;
    let service = RuleService::new(locked.db.clone());
    let rule = service
        .create_rule("main", RuleAction::Download)
        .await
        .unwrap();

    let conflict = service
        .delete_rule(rule.id, rule.revision + 1)
        .await
        .unwrap_err();
    assert!(matches!(conflict, RuleServiceError::RevisionConflict));

    let draft = service.load_draft(rule.id).await.unwrap().unwrap();
    service
        .publish_version(PublishRuleVersionRequest {
            rule_id: rule.id,
            base_version: None,
            expected_draft_revision: draft.revision,
            created_by: None,
        })
        .await
        .unwrap();
    let published = service.rule(rule.id).await.unwrap();

    service
        .delete_rule(rule.id, published.revision)
        .await
        .unwrap();
    assert!(matches!(
        service.rule(rule.id).await,
        Err(RuleServiceError::NotFound)
    ));
}

#[tokio::test]
async fn copying_a_rule_uses_its_latest_draft_with_a_new_identity() {
    let locked = LockedDb::new(RULE_VERSIONS_DB_LOCK_ID).await;
    let service = RuleService::new(locked.db.clone());
    let source = service
        .create_rule("source", RuleAction::Download)
        .await
        .unwrap();
    let source_draft = service.load_draft(source.id).await.unwrap().unwrap();
    service
        .save_draft(SaveRuleDraftRequest {
            rule_id: source.id,
            expected_revision: Some(source_draft.revision),
            base_version: None,
            definition: serde_json::to_value(definition(
                source.id,
                "edited source",
                RuleAction::MetadataOnly,
            ))
            .unwrap(),
        })
        .await
        .unwrap();

    let copied = service.copy_rule(source.id, "source copy").await.unwrap();
    let copied_definition = RuleDefinitionV1::parse(
        service
            .load_draft(copied.id)
            .await
            .unwrap()
            .unwrap()
            .definition,
    )
    .unwrap();

    assert_ne!(copied.id, source.id);
    assert_eq!(copied.name, "source copy");
    assert_eq!(copied_definition.id, copied.id);
    assert_eq!(copied_definition.name, "source copy");
    assert_eq!(copied_definition.action, RuleAction::MetadataOnly);
    assert!(copied.sort_order > source.sort_order);
}

#[tokio::test]
async fn copying_a_saved_rule_uses_its_current_immutable_version() {
    let locked = LockedDb::new(RULE_VERSIONS_DB_LOCK_ID).await;
    let service = RuleService::new(locked.db.clone());
    let source = service
        .create_rule("source", RuleAction::Download)
        .await
        .unwrap();
    let draft = service.load_draft(source.id).await.unwrap().unwrap();
    service
        .publish_version(PublishRuleVersionRequest {
            rule_id: source.id,
            base_version: None,
            expected_draft_revision: draft.revision,
            created_by: None,
        })
        .await
        .unwrap();

    let copied = service.copy_rule(source.id, "saved copy").await.unwrap();
    let copied_definition = RuleDefinitionV1::parse(
        service
            .load_draft(copied.id)
            .await
            .unwrap()
            .unwrap()
            .definition,
    )
    .unwrap();

    assert_eq!(copied_definition.id, copied.id);
    assert_eq!(copied_definition.name, "saved copy");
    assert_eq!(copied_definition.action, RuleAction::Download);
}

#[tokio::test]
async fn rule_order_requires_the_complete_catalog_and_persists() {
    let locked = LockedDb::new(RULE_VERSIONS_DB_LOCK_ID).await;
    let service = RuleService::new(locked.db.clone());
    let first = service
        .create_rule("first", RuleAction::Download)
        .await
        .unwrap();
    let second = service
        .create_rule("second", RuleAction::Download)
        .await
        .unwrap();
    let third = service
        .create_rule("third", RuleAction::Download)
        .await
        .unwrap();

    let incomplete = service
        .reorder_rules(&[third.id, first.id])
        .await
        .unwrap_err();
    assert!(matches!(incomplete, RuleServiceError::RevisionConflict));

    let duplicate = service
        .reorder_rules(&[third.id, first.id, first.id])
        .await
        .unwrap_err();
    assert!(matches!(duplicate, RuleServiceError::InvalidRequest(_)));

    let reordered = service
        .reorder_rules(&[third.id, first.id, second.id])
        .await
        .unwrap();
    assert_eq!(
        reordered.iter().map(|rule| rule.id).collect::<Vec<_>>(),
        [third.id, first.id, second.id]
    );
    assert_eq!(
        service
            .list_rules()
            .await
            .unwrap()
            .iter()
            .map(|rule| (rule.id, rule.sort_order))
            .collect::<Vec<_>>(),
        [(third.id, 1), (first.id, 2), (second.id, 3)]
    );
}

fn definition(rule_id: Uuid, name: &str, action: RuleAction) -> RuleDefinitionV1 {
    RuleDefinitionV1::match_all(rule_id, name, action, RuleAction::Download)
}
