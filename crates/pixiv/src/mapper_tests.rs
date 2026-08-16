use crate::mapper::{
    has_private_bookmark_evidence, map_account_profile, map_artist_work_ids, map_bookmarks,
    map_follow_latest, map_followed_artists, map_ranking_page, map_ugoira_meta, map_work_detail,
    map_work_pages,
};
use pixivarchive_domain::pixiv::{
    PixivAgeRating, PixivAiClassification, PixivBookmarkVisibility, PixivBookmarksMode,
    PixivBookmarksRequest, PixivFollowLatestMode, PixivFollowLatestRequest,
    PixivFollowLatestSource, PixivFollowingRequest, PixivFollowingVisibility, PixivImageFormat,
    PixivRankingContent, PixivRankingMode, PixivRankingRequest, PixivWorkKind,
};
use serde_json::Value;
use time::{Date, Month};

fn fixture(name: &str) -> Value {
    let path = format!("{}/../../fixtures/pixiv/{name}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn account_profile_maps_display_name_and_avatar_url() {
    let raw = serde_json::json!({
        "error": false,
        "message": "",
        "body": {
            "userId": "10001",
            "name": "Fixture Account",
            "image": "https://i.pximg.net/user-profile/img/small.png",
            "imageBig": "https://i.pximg.net/user-profile/img/large.png"
        }
    });

    let profile = map_account_profile(10_001, &raw).unwrap();

    assert_eq!(profile.display_name, "Fixture Account");
    assert_eq!(
        profile.avatar_url.as_deref(),
        Some("https://i.pximg.net/user-profile/img/large.png")
    );
}

#[test]
fn account_profile_treats_a_missing_or_invalid_avatar_as_optional() {
    let raw = serde_json::json!({
        "error": false,
        "message": "",
        "body": {
            "userId": 10001,
            "name": "Fixture Account",
            "image": "javascript:alert(1)"
        }
    });

    let profile = map_account_profile(10_001, &raw).unwrap();

    assert_eq!(profile.avatar_url, None);
}

#[test]
fn work_detail_maps_every_supported_work_kind_and_safety_flag() {
    let raw = fixture("illust.json");

    let illustration = map_work_detail(&raw["illustration"]).unwrap();
    assert_eq!(illustration.work_id, 1001);
    assert_eq!(illustration.kind, PixivWorkKind::Illustration);
    assert_eq!(illustration.age_rating, PixivAgeRating::AllAge);
    assert_eq!(
        illustration.ai_classification,
        PixivAiClassification::NotAiGenerated
    );
    assert!(illustration.is_original);
    assert_eq!(illustration.artist.pixiv_id, 501);
    assert_eq!(
        illustration.artist.account_name.as_deref(),
        Some("fixture_artist")
    );
    assert_eq!(illustration.page_count, 1);
    assert_eq!(illustration.dimensions.width, 2400);
    assert_eq!(illustration.dimensions.height, 3200);
    assert_eq!(illustration.counts.bookmarks, 1234);
    assert_eq!(illustration.counts.views, 9876);
    assert_eq!(illustration.tags[0].name, "オリジナル");
    assert_eq!(
        illustration.tags[0].translated_name.as_deref(),
        Some("original")
    );
    assert_eq!(illustration.bookmark.as_ref().unwrap().bookmark_id, 9001);
    assert_eq!(
        illustration.bookmark.as_ref().unwrap().visibility,
        PixivBookmarkVisibility::Public
    );
    assert_eq!(illustration.series.as_ref().unwrap().pixiv_id, 71);
    assert_eq!(illustration.series.as_ref().unwrap().order, Some(3));

    let manga = map_work_detail(&raw["manga"]).unwrap();
    assert_eq!(manga.kind, PixivWorkKind::Manga);
    assert_eq!(manga.age_rating, PixivAgeRating::R18);
    assert_eq!(manga.ai_classification, PixivAiClassification::AiGenerated);
    assert!(!manga.is_original);
    assert!(manga.bookmark.is_none());

    let ugoira = map_work_detail(&raw["ugoira"]).unwrap();
    assert_eq!(ugoira.kind, PixivWorkKind::Ugoira);
    assert_eq!(ugoira.age_rating, PixivAgeRating::R18g);
    assert_eq!(ugoira.ai_classification, PixivAiClassification::Unknown);
    assert_eq!(
        ugoira.bookmark.as_ref().unwrap().visibility,
        PixivBookmarkVisibility::Private
    );
}

#[test]
fn work_detail_prefers_illust_title_when_pixiv_also_returns_title() {
    let mut raw = fixture("illust.json")["illustration"].clone();
    raw["body"]["title"] = Value::from("duplicate legacy title");

    let detail = map_work_detail(&raw).unwrap();

    assert_eq!(detail.title, "Blue Illustration");
}

#[test]
fn pages_keep_source_order_dimensions_and_url_format_hints() {
    let pages = map_work_pages(1002, &fixture("pages.json")).unwrap();

    assert_eq!(pages.work_id, 1002);
    assert_eq!(pages.pages.len(), 2);
    assert_eq!(pages.pages[0].page_index, 0);
    assert_eq!(pages.pages[0].format_hint, Some(PixivImageFormat::Png));
    assert_eq!(pages.pages[1].page_index, 1);
    assert_eq!(pages.pages[1].format_hint, Some(PixivImageFormat::Jpeg));
    assert!(
        pages.pages[1]
            .original_url
            .as_str()
            .ends_with("1002_p1.jpg")
    );
}

#[test]
fn ugoira_uses_original_zip_and_preserves_frame_delays() {
    let meta = map_ugoira_meta(1003, &fixture("ugoira.json")).unwrap();

    assert_eq!(meta.work_id, 1003);
    assert!(meta.zip_url.as_str().contains("ugoira1920x1080.zip"));
    assert_eq!(meta.frame_mime_type, "image/png");
    assert_eq!(
        meta.frames
            .iter()
            .map(|frame| (frame.file.as_str(), frame.delay_ms))
            .collect::<Vec<_>>(),
        vec![("000000.png", 80), ("000001.png", 120), ("000002.png", 40)]
    );
}

#[test]
fn ranking_uses_response_next_page_and_keeps_rank_context() {
    let request = PixivRankingRequest {
        mode: PixivRankingMode::Daily,
        content: PixivRankingContent::All,
        date: None,
        page: 1,
    };
    let mut raw = fixture("ranking.json");
    raw["date"] = Value::String("20260729".to_owned());
    let page = map_ranking_page(&request, &raw).unwrap();

    assert_eq!(page.items.len(), 2);
    assert_eq!(
        page.date,
        Some(Date::from_calendar_date(2026, Month::July, 29).unwrap())
    );
    assert_eq!(page.items[0].work.work_id, 1001);
    assert_eq!(page.items[0].rank, 1);
    assert_eq!(page.items[0].previous_rank, Some(4));
    assert!(page.items[0].work.is_original);
    assert_eq!(page.items[1].work.kind, PixivWorkKind::Ugoira);
    assert_eq!(page.items[1].previous_rank, None);
    let next = page.next_cursor.unwrap();
    assert_eq!(next.page, 2);
    assert_eq!(next.mode, PixivRankingMode::Daily);
    assert_eq!(next.content, PixivRankingContent::All);
    assert_eq!(next.date, page.date);
}

#[test]
fn ranking_rejects_boolean_true_as_a_next_page() {
    let request = PixivRankingRequest {
        mode: PixivRankingMode::Daily,
        content: PixivRankingContent::All,
        date: None,
        page: 1,
    };
    let mut raw = fixture("ranking.json");
    raw["next"] = Value::Bool(true);

    assert!(map_ranking_page(&request, &raw).is_err());
}

#[test]
fn ranking_treats_zero_previous_rank_as_unranked() {
    let request = PixivRankingRequest {
        mode: PixivRankingMode::Daily,
        content: PixivRankingContent::All,
        date: None,
        page: 1,
    };
    let mut raw = fixture("ranking.json");
    raw["contents"][0]["yes_rank"] = Value::from(0);

    let page = map_ranking_page(&request, &raw).unwrap();

    assert_eq!(page.items[0].previous_rank, None);
}

#[test]
fn following_uses_page_ids_for_order_and_cursor_progress() {
    let request = PixivFollowLatestRequest {
        source: PixivFollowLatestSource::Following,
        mode: PixivFollowLatestMode::All,
        tag: None,
        language: "zh".to_owned(),
        page: 1,
    };
    let page = map_follow_latest(&request, &fixture("follow_latest.json")).unwrap();

    assert_eq!(
        page.items
            .iter()
            .map(|work| work.work_id)
            .collect::<Vec<_>>(),
        vec![1003, 1001]
    );
    assert_eq!(
        page.items[0].tags[0].translated_name.as_deref(),
        Some("ugoira")
    );
    assert_eq!(page.next_cursor.unwrap().page, 2);
}

#[test]
fn following_accepts_numeric_page_ids_from_pixiv() {
    let request = PixivFollowLatestRequest {
        source: PixivFollowLatestSource::Following,
        mode: PixivFollowLatestMode::All,
        tag: None,
        language: "zh".to_owned(),
        page: 1,
    };
    let mut raw = fixture("follow_latest.json");
    for id in raw["body"]["page"]["ids"].as_array_mut().unwrap() {
        *id = Value::from(id.as_str().unwrap().parse::<i64>().unwrap());
    }

    let page = map_follow_latest(&request, &raw).unwrap();

    assert_eq!(
        page.items
            .iter()
            .map(|work| work.work_id)
            .collect::<Vec<_>>(),
        vec![1003, 1001]
    );
}

#[test]
fn following_accepts_pixivs_thumbnail_only_page_shape() {
    let request = PixivFollowLatestRequest {
        source: PixivFollowLatestSource::Following,
        mode: PixivFollowLatestMode::All,
        tag: None,
        language: "zh".to_owned(),
        page: 1,
    };
    let raw = fixture("follow_latest_thumbnail_only.json");

    let page = map_follow_latest(&request, &raw).unwrap();

    assert_eq!(
        page.items
            .iter()
            .map(|work| work.work_id)
            .collect::<Vec<_>>(),
        vec![1001, 1003]
    );
    assert_eq!(page.next_cursor.unwrap().page, 2);
}

#[test]
fn following_distinguishes_response_structure_rejections() {
    let request = PixivFollowLatestRequest {
        source: PixivFollowLatestSource::Following,
        mode: PixivFollowLatestMode::All,
        tag: None,
        language: "zh".to_owned(),
        page: 1,
    };
    let mut raw = fixture("follow_latest.json");
    raw["body"]["page"] = Value::String("unexpected".to_owned());

    let error = map_follow_latest(&request, &raw).unwrap_err();

    assert!(
        error
            .to_string()
            .contains("follow_latest response structure rejected")
    );
}

#[test]
fn private_bookmark_evidence_accepts_pixivs_array_shape() {
    let raw = serde_json::json!({
        "body": {
            "bookmarkTags": [],
            "works": [],
            "total": 0,
        }
    });

    assert!(has_private_bookmark_evidence(&raw));
}

#[test]
fn following_skips_page_ids_without_thumbnail_data() {
    let request = PixivFollowLatestRequest {
        source: PixivFollowLatestSource::Following,
        mode: PixivFollowLatestMode::All,
        tag: None,
        language: "zh".to_owned(),
        page: 37,
    };
    let raw = fixture("follow_latest_missing_thumbnail.json");

    let page = map_follow_latest(&request, &raw).unwrap();

    assert_eq!(
        page.items
            .iter()
            .map(|work| work.work_id)
            .collect::<Vec<_>>(),
        vec![1003, 1001]
    );
    assert_eq!(page.next_cursor.unwrap().page, 38);
}

#[test]
fn work_metadata_rejects_zero_page_count_and_dimensions() {
    let mut zero_pages = fixture("illust.json")["illustration"].clone();
    zero_pages["body"]["pageCount"] = Value::from(0);
    assert!(map_work_detail(&zero_pages).is_err());

    let mut zero_width = fixture("illust.json")["illustration"].clone();
    zero_width["body"]["width"] = Value::from(0);
    assert!(map_work_detail(&zero_width).is_err());
}

#[test]
fn bookmarks_advance_by_returned_items_until_total_is_reached() {
    let request = PixivBookmarksRequest {
        user_id: 10001,
        visibility: PixivBookmarkVisibility::Private,
        mode: PixivBookmarksMode::All,
        tag: None,
        offset: 0,
    };
    let page = map_bookmarks(&request, &fixture("bookmarks.json")).unwrap();

    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0], 1002);
    assert_eq!(page.next_cursor.unwrap().offset, 1);
}

#[test]
fn bookmark_mapping_only_requires_the_work_id() {
    let request = PixivBookmarksRequest {
        user_id: 10001,
        visibility: PixivBookmarkVisibility::Private,
        mode: PixivBookmarksMode::All,
        tag: None,
        offset: 0,
    };
    let mut raw = fixture("bookmarks.json");
    raw["body"]["works"][0]["userId"] = Value::from(0);
    raw["body"]["works"][0]["pageCount"] = Value::from(0);
    raw["body"]["works"][0]["width"] = Value::from(0);

    let page = map_bookmarks(&request, &raw).unwrap();

    assert_eq!(page.items, vec![1002]);
}

#[test]
fn followed_artists_accept_string_and_numeric_ids_and_advance_by_returned_count() {
    let request = PixivFollowingRequest {
        user_id: 10001,
        visibility: PixivFollowingVisibility::Public,
        offset: 0,
        limit: 2,
        language: "zh".to_owned(),
    };
    let raw = serde_json::json!({
        "error": false,
        "message": "",
        "body": {
            "users": [
                {
                    "userId": "501",
                    "userName": "Test Artist",
                    "profileImageUrl": "https://i.pximg.net/user-profile/img/501.png",
                    "irrelevant": true
                },
                {
                    "userId": 502,
                    "userName": "Second Artist"
                }
            ],
            "total": 3
        }
    });

    let page = map_followed_artists(&request, &raw).unwrap();

    assert_eq!(page.items.len(), 2);
    assert_eq!(page.items[0].pixiv_id, 501);
    assert_eq!(page.items[0].name, "Test Artist");
    assert_eq!(
        page.items[0].profile_image_url.as_deref(),
        Some("https://i.pximg.net/user-profile/img/501.png")
    );
    assert_eq!(page.items[1].pixiv_id, 502);
    let next = page.next_cursor.unwrap();
    assert_eq!(next.user_id, 10001);
    assert_eq!(next.visibility, PixivFollowingVisibility::Public);
    assert_eq!(next.offset, 2);
    assert_eq!(next.limit, 2);
    assert_eq!(next.language, "zh");
}

#[test]
fn followed_artists_stop_when_returned_count_reaches_total() {
    let request = PixivFollowingRequest {
        user_id: 10001,
        visibility: PixivFollowingVisibility::Private,
        offset: 2,
        limit: 2,
        language: "zh".to_owned(),
    };
    let raw = serde_json::json!({
        "error": false,
        "message": "",
        "body": {
            "users": [
                {
                    "userId": "503",
                    "userName": "Last Artist",
                    "profileImageUrl": null
                }
            ],
            "total": "3"
        }
    });

    let page = map_followed_artists(&request, &raw).unwrap();

    assert_eq!(page.items[0].pixiv_id, 503);
    assert!(page.items[0].profile_image_url.is_none());
    assert!(page.next_cursor.is_none());
}

#[test]
fn artist_work_ids_merge_illustrations_and_manga_only() {
    let works = map_artist_work_ids(501, &fixture("profile_all.json")).unwrap();

    assert_eq!(works.artist_id, 501);
    assert_eq!(works.work_ids, vec![1001, 1002, 1003]);
}
