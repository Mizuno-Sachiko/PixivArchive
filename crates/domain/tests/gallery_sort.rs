use pixivarchive_domain::work::{
    GalleryCursor, GalleryCursorKey, GallerySearch, GallerySortField, SortDirection,
};
use serde_json::json;
use uuid::Uuid;

#[test]
fn gallery_sort_field_and_direction_have_independent_json_contracts() {
    let search = GallerySearch {
        sort_field: GallerySortField::PublishedAt,
        sort_direction: SortDirection::Ascending,
        ..GallerySearch::default()
    };

    let value = serde_json::to_value(&search).unwrap();

    assert_eq!(value["sort_field"], json!("published_at"));
    assert_eq!(value["sort_direction"], json!("ascending"));
    assert!(value.get("sort").is_none());
}

#[test]
fn gallery_cursor_records_the_sort_contract_that_created_it() {
    let cursor = GalleryCursor {
        sort_field: GallerySortField::BookmarkCount,
        sort_direction: SortDirection::Descending,
        key: GalleryCursorKey::Null,
        work_id: Uuid::from_u128(7),
    };

    let value = serde_json::to_value(&cursor).unwrap();

    assert_eq!(value["sort_field"], json!("bookmark_count"));
    assert_eq!(value["sort_direction"], json!("descending"));
    assert_eq!(value["key"], json!({ "type": "null" }));
}

#[test]
fn gallery_sort_defaults_to_newest_pixiv_id() {
    let search = GallerySearch::default();

    assert_eq!(search.sort_field, GallerySortField::PixivId);
    assert_eq!(search.sort_direction, SortDirection::Descending);
}
