use pixivarchive_domain::work::{
    GalleryContextKind, GalleryContextSelectionExpression, GallerySearch,
    GallerySelectionExpression, TrashFilter, TrashSelectionExpression,
};
use serde_json::json;
use uuid::Uuid;

#[test]
fn gallery_selection_expression_preserves_fixed_query_and_exceptions() {
    let work_id = Uuid::now_v7();
    let expression = GallerySelectionExpression {
        search: GallerySearch::default(),
        base_selected: true,
        exception_work_ids: vec![work_id],
    };

    let value = serde_json::to_value(&expression).unwrap();
    assert_eq!(value["search"], json!(GallerySearch::default()));
    assert_eq!(value["base_selected"], json!(true));
    assert_eq!(value["exception_work_ids"], json!([work_id]));
}

#[test]
fn gallery_context_selection_expression_preserves_fixed_query_and_exceptions() {
    let context_id = Uuid::now_v7();
    let expression = GalleryContextSelectionExpression {
        kind: GalleryContextKind::Tag,
        query: "夜空".to_owned(),
        base_selected: false,
        exception_context_ids: vec![context_id],
    };

    let value = serde_json::to_value(&expression).unwrap();
    assert_eq!(value["kind"], json!("tag"));
    assert_eq!(value["query"], json!("夜空"));
    assert_eq!(value["base_selected"], json!(false));
    assert_eq!(value["exception_context_ids"], json!([context_id]));
}

#[test]
fn trash_selection_expression_preserves_fixed_filter_and_exceptions() {
    let work_id = Uuid::now_v7();
    let expression = TrashSelectionExpression {
        filter: TrashFilter {
            query: Some("夜空".to_owned()),
            purge_states: vec!["pending".to_owned()],
        },
        base_selected: true,
        exception_work_ids: vec![work_id],
    };

    let value = serde_json::to_value(&expression).unwrap();
    assert_eq!(value["filter"]["query"], json!("夜空"));
    assert_eq!(value["filter"]["purge_states"], json!(["pending"]));
    assert_eq!(value["base_selected"], json!(true));
    assert_eq!(value["exception_work_ids"], json!([work_id]));
}
