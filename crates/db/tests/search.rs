mod support;

use pixivarchive_db::{
    Db, DbError, DerivativeKind, GalleryRepository, JobRepository, MediaRepository, SaveDerivative,
    SavePixivWorkMetadata, SaveSourceMediaRevision, WorkRepository,
};
use pixivarchive_domain::{
    job::{JobKind, JobPriority, JobQuotaSelection, NewJob},
    media::{DerivativeFormat, MediaDimensions, MediaFormat, MediaKind},
    pixiv::{
        PixivAgeRating, PixivAiClassification, PixivArtistRef, PixivDimensions, PixivTag,
        PixivWorkCounts, PixivWorkDetail, PixivWorkKind, PixivWorkPage, PixivWorkPages,
    },
    work::{
        FilterMode, GalleryBooleanField, GalleryCategoryField, GalleryContextKind,
        GalleryContextSelectionExpression, GalleryFilter, GalleryFilterGroup, GallerySearch,
        GallerySortField, GalleryTagOperator, GalleryTagScope, GalleryTextField,
        GalleryTextOperator, SortDirection,
    },
};
use serde_json::json;
use sqlx::Row;
use std::path::PathBuf;
use time::{Date, Month, PrimitiveDateTime, Time};
use url::Url;
use uuid::Uuid;

#[tokio::test]
async fn source_and_derivative_writes_reject_expired_job_leases() {
    let locked = support::LockedDb::new().await;
    let works = WorkRepository::new(locked.db.clone());
    let work = seed_work(
        &locked.db,
        &works,
        WorkSeed {
            pixiv_id: 7_001,
            title: "lease guarded media",
            artist_id: 701,
            artist_name: "lease artist",
            tags: &[("lease", None)],
            bookmarks: 1,
            age_rating: PixivAgeRating::AllAge,
        },
    )
    .await;
    let page = sqlx::query(
        "SELECT id, current_media_revision_id, source_url FROM work_page WHERE work_id = $1",
    )
    .bind(work.id)
    .fetch_one(locked.db.pool())
    .await
    .unwrap();
    let page_id: Uuid = page.get("id");
    let current_media_id: Uuid = page.get("current_media_revision_id");
    let source_url = Url::parse(&page.get::<String, _>("source_url")).unwrap();
    let jobs = JobRepository::new(locked.db.clone());
    let media = MediaRepository::new(locked.db.clone());

    let download_job_id = jobs
        .enqueue(NewJob::for_kind(
            JobPriority::ManualImport,
            JobKind::DownloadMedia,
            json!({ "work_id": work.id, "pixiv_work_id": 7_001 }),
        ))
        .await
        .unwrap();
    let download = jobs
        .claim_next(
            Uuid::now_v7(),
            &JobQuotaSelection::new(vec![JobPriority::ManualImport]),
            time::Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(download.id, download_job_id);
    let source_path = PathBuf::from("lease/source-v2.png");
    media
        .register_artifact_intent(download.lease(), &source_path)
        .await
        .unwrap();
    expire_lease(&locked.db, download.id).await;
    let source_result = media
        .save_source_revision(SaveSourceMediaRevision {
            lease: download.lease(),
            derivative_priority: JobPriority::BackgroundMaintenance,
            work_id: work.id,
            work_page_id: page_id,
            expected_current_media_revision_id: Some(current_media_id),
            revision_number: 2,
            media_kind: MediaKind::SourceImage,
            format: MediaFormat::Png,
            source_url,
            relative_path: source_path,
            byte_size: 1,
            sha256: [1; 32],
            dimensions: Some(MediaDimensions {
                width: 1_200,
                height: 1_600,
            }),
            ugoira: None,
            complete_job: false,
        })
        .await;
    assert!(matches!(source_result, Err(DbError::LeaseConflict)));

    let derivative_job_id = jobs
        .enqueue(NewJob::for_kind(
            JobPriority::BackgroundMaintenance,
            JobKind::GenerateDerivative,
            json!({ "media_revision_id": current_media_id }),
        ))
        .await
        .unwrap();
    let derivative = jobs
        .claim_next(
            Uuid::now_v7(),
            &JobQuotaSelection::new(vec![JobPriority::BackgroundMaintenance]),
            time::Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(derivative.id, derivative_job_id);
    let derivative_path = PathBuf::from("lease/thumb.webp");
    media
        .register_artifact_intent(derivative.lease(), &derivative_path)
        .await
        .unwrap();
    expire_lease(&locked.db, derivative.id).await;
    let derivative_result = media
        .save_derivative(SaveDerivative {
            lease: derivative.lease(),
            media_revision_id: current_media_id,
            kind: DerivativeKind::WaterfallThumbnail,
            format: DerivativeFormat::Webp,
            relative_path: derivative_path,
            dimensions: MediaDimensions {
                width: 320,
                height: 480,
            },
            byte_size: 1,
            dominant_color: "#1450c8".to_owned(),
            complete_job: false,
        })
        .await;
    assert!(matches!(derivative_result, Err(DbError::LeaseConflict)));
}

#[tokio::test]
async fn structured_filters_support_groups_tags_and_ranges() {
    let locked = support::LockedDb::new().await;
    let works = WorkRepository::new(locked.db.clone());
    seed_work(
        &locked.db,
        &works,
        WorkSeed {
            pixiv_id: 8_101,
            title: "Blue Illustration",
            artist_id: 201,
            artist_name: "Alice",
            tags: &[("blue", Some("蓝色")), ("uniform", Some("制服"))],
            bookmarks: 500,
            age_rating: PixivAgeRating::AllAge,
        },
    )
    .await;
    seed_work(
        &locked.db,
        &works,
        WorkSeed {
            pixiv_id: 8_104,
            title: "Blue Variant",
            artist_id: 202,
            artist_name: "Bob",
            tags: &[(" BLUE ", Some("蓝色"))],
            bookmarks: 250,
            age_rating: PixivAgeRating::AllAge,
        },
    )
    .await;
    seed_work(
        &locked.db,
        &works,
        WorkSeed {
            pixiv_id: 8_102,
            title: "Red Moon",
            artist_id: 202,
            artist_name: "Bob",
            tags: &[("red", Some("红色"))],
            bookmarks: 1_000,
            age_rating: PixivAgeRating::R18,
        },
    )
    .await;
    seed_work(
        &locked.db,
        &works,
        WorkSeed {
            pixiv_id: 8_103,
            title: "Blue Sky",
            artist_id: 201,
            artist_name: "Alice",
            tags: &[("blue", Some("蓝色")), ("sky", Some("天空"))],
            bookmarks: 100,
            age_rating: PixivAgeRating::AllAge,
        },
    )
    .await;

    let gallery = GalleryRepository::new(locked.db.clone());
    let visible_search = GallerySearch {
        group_mode: FilterMode::All,
        groups: vec![GalleryFilterGroup {
            mode: FilterMode::All,
            filters: vec![
                GalleryFilter::Text {
                    field: GalleryTextField::Title,
                    operator: GalleryTextOperator::Contains,
                    value: "blue".to_owned(),
                },
                GalleryFilter::Tags {
                    operator: GalleryTagOperator::Any,
                    names: vec!["制服".to_owned()],
                    scope: GalleryTagScope::OriginalAndTranslation,
                },
                GalleryFilter::Number {
                    field: pixivarchive_domain::work::GalleryNumberField::BookmarkCount,
                    comparison:
                        pixivarchive_domain::work::GalleryNumberComparison::GreaterThanOrEqual(
                            500.0,
                        ),
                },
            ],
        }],
        restrict_work_ids: Vec::new(),
        sort_field: GallerySortField::PixivId,
        sort_direction: SortDirection::Descending,
        cursor: None,
        limit: 20,
    };
    let visible = gallery.search(visible_search.clone(), None).await.unwrap();
    assert_eq!(
        visible
            .items
            .iter()
            .map(|item| item.pixiv_work_id)
            .collect::<Vec<_>>(),
        vec![8_101]
    );
    assert_eq!(gallery.count(&visible_search, None).await.unwrap(), 1);
    let matching_ids = gallery.work_ids(&visible_search, &[], None).await.unwrap();
    assert_eq!(matching_ids, vec![visible.items[0].id]);
    assert!(
        gallery
            .work_ids(&visible_search, &matching_ids, None)
            .await
            .unwrap()
            .is_empty()
    );

    let first_artist_page = gallery.artists(1, None, None).await.unwrap();
    assert_eq!(first_artist_page.total, 2);
    let artist_cursor = first_artist_page.next_cursor.clone().unwrap();
    let second_artist_page = gallery
        .artists(1, Some(&artist_cursor), None)
        .await
        .unwrap();
    assert_eq!(second_artist_page.total, 2);
    assert_eq!(second_artist_page.next_cursor, None);
    assert_ne!(
        first_artist_page.items[0].id,
        second_artist_page.items[0].id
    );

    let tag_page = gallery.tags(20, None, None).await.unwrap();
    assert_eq!(tag_page.total, 4);
    assert!(matches!(
        gallery.tags(1, Some(&artist_cursor), None).await,
        Err(DbError::InvalidValue(_))
    ));
    let blue = tag_page
        .items
        .iter()
        .find(|item| item.tag.original == "blue")
        .unwrap();
    assert_eq!(blue.work_count, 3);
    assert_eq!(gallery.tag_detail(" BLUE ").await.unwrap().work_count, 3);

    let exact_tag_results = gallery
        .search(
            GallerySearch {
                group_mode: FilterMode::All,
                groups: vec![GalleryFilterGroup {
                    mode: FilterMode::All,
                    filters: vec![GalleryFilter::Tags {
                        operator: GalleryTagOperator::ExactSet,
                        names: vec!["blue".to_owned(), "sky".to_owned()],
                        scope: GalleryTagScope::Original,
                    }],
                }],
                restrict_work_ids: Vec::new(),
                sort_field: GallerySortField::PixivId,
                sort_direction: SortDirection::Descending,
                cursor: None,
                limit: 20,
            },
            None,
        )
        .await
        .unwrap();
    assert_eq!(exact_tag_results.items[0].pixiv_work_id, 8_103);
}

#[tokio::test]
async fn gallery_exposes_and_filters_ai_generated_works() {
    let locked = support::LockedDb::new().await;
    let works = WorkRepository::new(locked.db.clone());
    let ai_work = seed_work(
        &locked.db,
        &works,
        WorkSeed {
            pixiv_id: 8_104,
            title: "AI work",
            artist_id: 203,
            artist_name: "Carol",
            tags: &[],
            bookmarks: 50,
            age_rating: PixivAgeRating::AllAge,
        },
    )
    .await;
    sqlx::query(
        "UPDATE work_revision SET metadata = jsonb_set(metadata, '{ai_classification}', '\"ai_generated\"') WHERE work_id = $1",
    )
    .bind(ai_work.id)
    .execute(locked.db.pool())
    .await
    .unwrap();

    let result = GalleryRepository::new(locked.db.clone())
        .search(
            GallerySearch {
                groups: vec![GalleryFilterGroup {
                    mode: FilterMode::All,
                    filters: vec![GalleryFilter::Boolean {
                        field: pixivarchive_domain::work::GalleryBooleanField::AiGenerated,
                        value: true,
                    }],
                }],
                ..GallerySearch::default()
            },
            None,
        )
        .await
        .unwrap();

    assert_eq!(result.items.len(), 1);
    assert_eq!(result.items[0].pixiv_work_id, 8_104);
    assert!(result.items[0].ai_generated);
}

#[tokio::test]
async fn gallery_bookmarks_are_scoped_to_the_selected_pixiv_account() {
    let locked = support::LockedDb::new().await;
    let works = WorkRepository::new(locked.db.clone());
    let work_a = seed_work(
        &locked.db,
        &works,
        WorkSeed {
            pixiv_id: 8_105,
            title: "Account A bookmark",
            artist_id: 205,
            artist_name: "Alice",
            tags: &[],
            bookmarks: 10,
            age_rating: PixivAgeRating::AllAge,
        },
    )
    .await;
    let work_b = seed_work(
        &locked.db,
        &works,
        WorkSeed {
            pixiv_id: 8_106,
            title: "Account B bookmark",
            artist_id: 206,
            artist_name: "Bob",
            tags: &[],
            bookmarks: 20,
            age_rating: PixivAgeRating::AllAge,
        },
    )
    .await;
    let account_a = Uuid::now_v7();
    let account_b = Uuid::now_v7();
    for (account_id, pixiv_user_id) in [(account_a, 91_105_i64), (account_b, 91_106_i64)] {
        sqlx::query(
            r#"
            INSERT INTO pixiv_account (
                id, pixiv_user_id, display_name, state, cookie_key_id,
                cookie_nonce, cookie_ciphertext, user_agent
            )
            VALUES ($1, $2, 'search test', 'normal', 'test', $3, $4, 'test-agent')
            "#,
        )
        .bind(account_id)
        .bind(pixiv_user_id)
        .bind(vec![1_u8; 12])
        .bind(vec![2_u8; 32])
        .execute(locked.db.pool())
        .await
        .unwrap();
    }
    for (account_id, work_id, bookmark_id) in [
        (account_a, work_a.id, 81_105_i64),
        (account_b, work_b.id, 81_106_i64),
    ] {
        sqlx::query(
            r#"
            INSERT INTO pixiv_work_bookmark (
                pixiv_account_id, work_id, pixiv_bookmark_id, visibility, active
            )
            VALUES ($1, $2, $3, 'public', true)
            "#,
        )
        .bind(account_id)
        .bind(work_id)
        .bind(bookmark_id)
        .execute(locked.db.pool())
        .await
        .unwrap();
    }

    let gallery = GalleryRepository::new(locked.db.clone());
    let bookmarked_search = GallerySearch {
        groups: vec![GalleryFilterGroup {
            mode: FilterMode::All,
            filters: vec![GalleryFilter::Boolean {
                field: GalleryBooleanField::BookmarkedByCurrentAccount,
                value: true,
            }],
        }],
        ..GallerySearch::default()
    };
    let account_a_results = gallery
        .search(bookmarked_search.clone(), Some(account_a))
        .await
        .unwrap();
    let account_b_results = gallery
        .search(bookmarked_search.clone(), Some(account_b))
        .await
        .unwrap();
    let anonymous_results = gallery.search(bookmarked_search, None).await.unwrap();

    assert_eq!(account_a_results.items.len(), 1);
    assert_eq!(account_a_results.items[0].id, work_a.id);
    assert_eq!(account_a_results.items[0].bookmark_id, Some(81_105));
    assert_eq!(account_b_results.items.len(), 1);
    assert_eq!(account_b_results.items[0].id, work_b.id);
    assert_eq!(account_b_results.items[0].bookmark_id, Some(81_106));
    assert!(anonymous_results.items.is_empty());

    let detail_for_b = gallery
        .work_detail(work_a.id, Some(account_b))
        .await
        .unwrap();
    assert!(!detail_for_b.work.bookmarked_by_current_account);
    assert_eq!(detail_for_b.work.bookmark_id, None);
}

#[tokio::test]
async fn local_detail_scope_includes_trash_and_excludes_metadata_only_works() {
    let locked = support::LockedDb::new().await;
    let works = WorkRepository::new(locked.db.clone());
    let work = seed_work(
        &locked.db,
        &works,
        WorkSeed {
            pixiv_id: 8_109,
            title: "trash detail target",
            artist_id: 209,
            artist_name: "Trash Artist",
            tags: &[],
            bookmarks: 10,
            age_rating: PixivAgeRating::AllAge,
        },
    )
    .await;
    works
        .move_to_trash(
            work.id,
            time::OffsetDateTime::now_utc() + time::Duration::days(30),
        )
        .await
        .unwrap();

    let gallery = GalleryRepository::new(locked.db.clone());
    let search = GallerySearch {
        groups: vec![GalleryFilterGroup {
            mode: FilterMode::All,
            filters: vec![GalleryFilter::PixivWorkId { value: 8_109 }],
        }],
        ..GallerySearch::default()
    };
    assert!(gallery.search(search, None).await.unwrap().items.is_empty());
    assert_eq!(gallery.work_id_by_pixiv_id(8_109).await.unwrap(), work.id);

    let metadata_only = works
        .create_metadata_only(8_110, 210, "metadata-only work")
        .await
        .unwrap();
    assert!(matches!(
        gallery.work_id_by_pixiv_id(8_110).await,
        Err(DbError::NotFound)
    ));
    assert_eq!(
        gallery
            .work_detail(metadata_only.id, None)
            .await
            .unwrap()
            .work
            .id,
        metadata_only.id
    );

    works
        .mark_physically_deleted(8_109, "manual_purge")
        .await
        .unwrap();
    assert!(matches!(
        gallery.work_id_by_pixiv_id(8_109).await,
        Err(DbError::NotFound)
    ));
}

#[tokio::test]
async fn work_id_restriction_intersects_with_any_mode_search_groups() {
    let locked = support::LockedDb::new().await;
    let works = WorkRepository::new(locked.db.clone());
    let text_match = seed_work(
        &locked.db,
        &works,
        WorkSeed {
            pixiv_id: 8_111,
            title: "Blue match",
            artist_id: 211,
            artist_name: "Alice",
            tags: &[],
            bookmarks: 10,
            age_rating: PixivAgeRating::AllAge,
        },
    )
    .await;
    let unrestricted = seed_work(
        &locked.db,
        &works,
        WorkSeed {
            pixiv_id: 8_112,
            title: "Plain work",
            artist_id: 212,
            artist_name: "Bob",
            tags: &[],
            bookmarks: 20,
            age_rating: PixivAgeRating::AllAge,
        },
    )
    .await;
    let metadata_match = seed_work(
        &locked.db,
        &works,
        WorkSeed {
            pixiv_id: 8_113,
            title: "Metadata match",
            artist_id: 213,
            artist_name: "Carol",
            tags: &[],
            bookmarks: 30,
            age_rating: PixivAgeRating::AllAge,
        },
    )
    .await;
    let bookmark_match = seed_work(
        &locked.db,
        &works,
        WorkSeed {
            pixiv_id: 8_114,
            title: "Bookmark match",
            artist_id: 214,
            artist_name: "Dave",
            tags: &[],
            bookmarks: 40,
            age_rating: PixivAgeRating::AllAge,
        },
    )
    .await;

    sqlx::query("UPDATE work SET collection_state = 'metadata_only' WHERE id = $1")
        .bind(metadata_match.id)
        .execute(locked.db.pool())
        .await
        .unwrap();
    let account_id = Uuid::now_v7();
    sqlx::query(
        r#"
        INSERT INTO pixiv_account (
            id, pixiv_user_id, display_name, state, cookie_key_id,
            cookie_nonce, cookie_ciphertext, user_agent
        )
        VALUES ($1, 91114, 'search test', 'normal', 'test', $2, $3, 'test-agent')
        "#,
    )
    .bind(account_id)
    .bind(vec![1_u8; 12])
    .bind(vec![2_u8; 32])
    .execute(locked.db.pool())
    .await
    .unwrap();
    sqlx::query(
        r#"
        INSERT INTO pixiv_work_bookmark (
            pixiv_account_id, work_id, pixiv_bookmark_id, visibility, active
        )
        VALUES ($1, $2, 81114, 'public', true)
        "#,
    )
    .bind(account_id)
    .bind(bookmark_match.id)
    .execute(locked.db.pool())
    .await
    .unwrap();

    let result = GalleryRepository::new(locked.db.clone())
        .search(
            GallerySearch {
                group_mode: FilterMode::Any,
                groups: vec![
                    GalleryFilterGroup {
                        mode: FilterMode::All,
                        filters: vec![GalleryFilter::Text {
                            field: GalleryTextField::Title,
                            operator: GalleryTextOperator::Contains,
                            value: "blue".to_owned(),
                        }],
                    },
                    GalleryFilterGroup {
                        mode: FilterMode::All,
                        filters: vec![GalleryFilter::Category {
                            field: GalleryCategoryField::CollectionState,
                            include: vec!["metadata_only".to_owned()],
                            exclude: Vec::new(),
                        }],
                    },
                    GalleryFilterGroup {
                        mode: FilterMode::All,
                        filters: vec![GalleryFilter::Boolean {
                            field: GalleryBooleanField::BookmarkedByCurrentAccount,
                            value: true,
                        }],
                    },
                ],
                sort_field: GallerySortField::PixivId,
                sort_direction: SortDirection::Descending,
                cursor: None,
                limit: 20,
                restrict_work_ids: vec![text_match.id, unrestricted.id],
            },
            Some(account_id),
        )
        .await
        .unwrap();

    assert_eq!(
        result.items.iter().map(|work| work.id).collect::<Vec<_>>(),
        vec![text_match.id]
    );
}

#[tokio::test]
async fn stable_cursor_pagination_never_repeats_a_work() {
    let locked = support::LockedDb::new().await;
    let works = WorkRepository::new(locked.db.clone());
    for pixiv_id in [8_201, 8_202, 8_203] {
        seed_work(
            &locked.db,
            &works,
            WorkSeed {
                pixiv_id,
                title: "same title",
                artist_id: 210,
                artist_name: "cursor artist",
                tags: &[],
                bookmarks: 10,
                age_rating: PixivAgeRating::AllAge,
            },
        )
        .await;
    }
    let gallery = GalleryRepository::new(locked.db.clone());
    let first = gallery
        .search(
            GallerySearch {
                groups: Vec::new(),
                limit: 2,
                sort_field: GallerySortField::PixivId,
                sort_direction: SortDirection::Descending,
                ..GallerySearch::default()
            },
            None,
        )
        .await
        .unwrap();
    assert_eq!(first.items.len(), 2);
    let second = gallery
        .search(
            GallerySearch {
                groups: Vec::new(),
                limit: 2,
                sort_field: GallerySortField::PixivId,
                sort_direction: SortDirection::Descending,
                cursor: first.next_cursor,
                ..GallerySearch::default()
            },
            None,
        )
        .await
        .unwrap();
    assert_eq!(second.items.len(), 1);
    assert!(
        !first
            .items
            .iter()
            .any(|left| second.items.iter().any(|right| left.id == right.id))
    );
}

#[tokio::test]
async fn every_gallery_sort_direction_uses_stable_keyset_pagination() {
    let locked = support::LockedDb::new().await;
    let works = WorkRepository::new(locked.db.clone());
    let first = seed_work(
        &locked.db,
        &works,
        WorkSeed {
            pixiv_id: 8_221,
            title: "same title",
            artist_id: 221,
            artist_name: "sort artist",
            tags: &[],
            bookmarks: 10,
            age_rating: PixivAgeRating::AllAge,
        },
    )
    .await;
    let second = seed_work(
        &locked.db,
        &works,
        WorkSeed {
            pixiv_id: 8_222,
            title: "same title",
            artist_id: 221,
            artist_name: "sort artist",
            tags: &[],
            bookmarks: 10,
            age_rating: PixivAgeRating::AllAge,
        },
    )
    .await;
    let third = seed_work(
        &locked.db,
        &works,
        WorkSeed {
            pixiv_id: 8_223,
            title: "different title",
            artist_id: 221,
            artist_name: "sort artist",
            tags: &[],
            bookmarks: 30,
            age_rating: PixivAgeRating::AllAge,
        },
    )
    .await;
    sqlx::query("UPDATE work_revision SET pixiv_created_at = NULL WHERE work_id = $1")
        .bind(third.id)
        .execute(locked.db.pool())
        .await
        .unwrap();
    sqlx::query("UPDATE work SET bookmark_count = NULL WHERE id = $1")
        .bind(second.id)
        .execute(locked.db.pool())
        .await
        .unwrap();

    let gallery = GalleryRepository::new(locked.db.clone());
    let expected = [first.id, second.id, third.id]
        .into_iter()
        .collect::<std::collections::HashSet<_>>();
    for sort_field in [
        GallerySortField::PixivId,
        GallerySortField::LocalUpdatedAt,
        GallerySortField::PublishedAt,
        GallerySortField::BookmarkCount,
        GallerySortField::Title,
    ] {
        for sort_direction in [SortDirection::Ascending, SortDirection::Descending] {
            let mut seen = Vec::new();
            let mut cursor = None;
            loop {
                let page = gallery
                    .search(
                        GallerySearch {
                            sort_field,
                            sort_direction,
                            cursor,
                            limit: 1,
                            ..GallerySearch::default()
                        },
                        None,
                    )
                    .await
                    .unwrap();
                seen.extend(page.items.iter().map(|work| work.id));
                cursor = page.next_cursor;
                if cursor.is_none() {
                    break;
                }
            }

            let unique = seen
                .iter()
                .copied()
                .collect::<std::collections::HashSet<_>>();
            assert_eq!(seen.len(), 3, "{sort_field:?} {sort_direction:?}");
            assert_eq!(unique.len(), 3, "{sort_field:?} {sort_direction:?}");
            assert_eq!(unique, expected, "{sort_field:?} {sort_direction:?}");
        }
    }
}

#[tokio::test]
async fn nullable_gallery_sorts_keep_missing_values_last_in_both_directions() {
    let locked = support::LockedDb::new().await;
    let works = WorkRepository::new(locked.db.clone());
    let present_low = seed_work(
        &locked.db,
        &works,
        WorkSeed {
            pixiv_id: 8_231,
            title: "present low",
            artist_id: 223,
            artist_name: "nullable artist",
            tags: &[],
            bookmarks: 10,
            age_rating: PixivAgeRating::AllAge,
        },
    )
    .await;
    let present_high = seed_work(
        &locked.db,
        &works,
        WorkSeed {
            pixiv_id: 8_232,
            title: "present high",
            artist_id: 223,
            artist_name: "nullable artist",
            tags: &[],
            bookmarks: 20,
            age_rating: PixivAgeRating::AllAge,
        },
    )
    .await;
    let missing = seed_work(
        &locked.db,
        &works,
        WorkSeed {
            pixiv_id: 8_233,
            title: "missing",
            artist_id: 223,
            artist_name: "nullable artist",
            tags: &[],
            bookmarks: 30,
            age_rating: PixivAgeRating::AllAge,
        },
    )
    .await;
    sqlx::query("UPDATE work SET bookmark_count = NULL WHERE id = $1")
        .bind(missing.id)
        .execute(locked.db.pool())
        .await
        .unwrap();
    sqlx::query("UPDATE work_revision SET pixiv_created_at = NULL WHERE work_id = $1")
        .bind(missing.id)
        .execute(locked.db.pool())
        .await
        .unwrap();

    let gallery = GalleryRepository::new(locked.db.clone());
    for sort_field in [
        GallerySortField::PublishedAt,
        GallerySortField::BookmarkCount,
    ] {
        for sort_direction in [SortDirection::Ascending, SortDirection::Descending] {
            let result = gallery
                .search(
                    GallerySearch {
                        sort_field,
                        sort_direction,
                        limit: 20,
                        ..GallerySearch::default()
                    },
                    None,
                )
                .await
                .unwrap();
            assert_eq!(result.items.last().map(|work| work.id), Some(missing.id));
            let present = result.items[..2]
                .iter()
                .map(|work| work.id)
                .collect::<std::collections::HashSet<_>>();
            assert_eq!(
                present,
                [present_low.id, present_high.id]
                    .into_iter()
                    .collect::<std::collections::HashSet<_>>()
            );
        }
    }
}

#[tokio::test]
async fn gallery_cursor_rejects_a_different_sort_field_or_direction() {
    let locked = support::LockedDb::new().await;
    let works = WorkRepository::new(locked.db.clone());
    seed_work(
        &locked.db,
        &works,
        WorkSeed {
            pixiv_id: 8_241,
            title: "cursor contract",
            artist_id: 224,
            artist_name: "cursor contract artist",
            tags: &[],
            bookmarks: 10,
            age_rating: PixivAgeRating::AllAge,
        },
    )
    .await;
    seed_work(
        &locked.db,
        &works,
        WorkSeed {
            pixiv_id: 8_242,
            title: "cursor contract 2",
            artist_id: 224,
            artist_name: "cursor contract artist",
            tags: &[],
            bookmarks: 20,
            age_rating: PixivAgeRating::AllAge,
        },
    )
    .await;
    let gallery = GalleryRepository::new(locked.db.clone());
    let first = gallery
        .search(
            GallerySearch {
                sort_field: GallerySortField::PixivId,
                sort_direction: SortDirection::Descending,
                limit: 1,
                ..GallerySearch::default()
            },
            None,
        )
        .await
        .unwrap();
    let cursor = first.next_cursor.unwrap();

    for (sort_field, sort_direction) in [
        (GallerySortField::BookmarkCount, SortDirection::Descending),
        (GallerySortField::PixivId, SortDirection::Ascending),
    ] {
        let result = gallery
            .search(
                GallerySearch {
                    sort_field,
                    sort_direction,
                    cursor: Some(cursor.clone()),
                    limit: 1,
                    ..GallerySearch::default()
                },
                None,
            )
            .await;
        assert!(matches!(result, Err(DbError::InvalidValue(_))));
    }
}

#[tokio::test]
async fn directory_cursor_pagination_is_stable_for_equal_sort_values() {
    let locked = support::LockedDb::new().await;
    let works = WorkRepository::new(locked.db.clone());
    for (index, pixiv_id) in [8_211, 8_212, 8_213].into_iter().enumerate() {
        let tag = format!("cursor-tag-{index}");
        let work = seed_work(
            &locked.db,
            &works,
            WorkSeed {
                pixiv_id,
                title: "directory cursor work",
                artist_id: 220 + index as i64,
                artist_name: "same artist name",
                tags: &[(tag.as_str(), None)],
                bookmarks: 10,
                age_rating: PixivAgeRating::AllAge,
            },
        )
        .await;
        let series_id = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO series (id, pixiv_series_id, title) VALUES ($1, $2, 'same series title')",
        )
        .bind(series_id)
        .bind(8_211 + index as i64)
        .execute(locked.db.pool())
        .await
        .unwrap();
        sqlx::query("UPDATE work SET series_id = $2 WHERE id = $1")
            .bind(work.id)
            .bind(series_id)
            .execute(locked.db.pool())
            .await
            .unwrap();
    }

    let gallery = GalleryRepository::new(locked.db.clone());

    let first_artists = gallery.artists(2, None, None).await.unwrap();
    let artist_cursor = first_artists.next_cursor.clone().unwrap();
    let second_artists = gallery
        .artists(2, Some(&artist_cursor), None)
        .await
        .unwrap();
    assert_eq!(first_artists.total, 3);
    assert_eq!(second_artists.items.len(), 1);
    assert!(
        !first_artists
            .items
            .iter()
            .any(|left| { second_artists.items.iter().any(|right| left.id == right.id) })
    );

    let first_tags = gallery.tags(2, None, None).await.unwrap();
    let tag_cursor = first_tags.next_cursor.clone().unwrap();
    let second_tags = gallery.tags(2, Some(&tag_cursor), None).await.unwrap();
    assert_eq!(first_tags.total, 3);
    assert_eq!(second_tags.items.len(), 1);
    assert!(!first_tags.items.iter().any(|left| {
        second_tags
            .items
            .iter()
            .any(|right| left.tag.id == right.tag.id)
    }));

    let first_series = gallery.series(2, None, None).await.unwrap();
    let series_cursor = first_series.next_cursor.clone().unwrap();
    let second_series = gallery.series(2, Some(&series_cursor), None).await.unwrap();
    assert_eq!(first_series.total, 3);
    assert_eq!(second_series.items.len(), 1);
    assert!(
        !first_series
            .items
            .iter()
            .any(|left| { second_series.items.iter().any(|right| left.id == right.id) })
    );
}

#[tokio::test]
async fn context_selection_projects_fixed_query_and_deduplicated_works() {
    let locked = support::LockedDb::new().await;
    let works = WorkRepository::new(locked.db.clone());
    let first = seed_work(
        &locked.db,
        &works,
        WorkSeed {
            pixiv_id: 8_221,
            title: "shared context first",
            artist_id: 221,
            artist_name: "shared artist",
            tags: &[("shared", None), ("night", None)],
            bookmarks: 10,
            age_rating: PixivAgeRating::AllAge,
        },
    )
    .await;
    let second = seed_work(
        &locked.db,
        &works,
        WorkSeed {
            pixiv_id: 8_222,
            title: "shared context second",
            artist_id: 222,
            artist_name: "other artist",
            tags: &[("shared", None)],
            bookmarks: 10,
            age_rating: PixivAgeRating::AllAge,
        },
    )
    .await;
    let third = seed_work(
        &locked.db,
        &works,
        WorkSeed {
            pixiv_id: 8_223,
            title: "night context third",
            artist_id: 223,
            artist_name: "third artist",
            tags: &[("night", None)],
            bookmarks: 10,
            age_rating: PixivAgeRating::AllAge,
        },
    )
    .await;
    let gallery = GalleryRepository::new(locked.db.clone());
    let tags = gallery.tags(10, None, None).await.unwrap();
    let shared_id = tags
        .items
        .iter()
        .find(|item| item.tag.original == "shared")
        .unwrap()
        .tag
        .id;
    let expression = GalleryContextSelectionExpression {
        kind: GalleryContextKind::Tag,
        query: String::new(),
        base_selected: true,
        exception_context_ids: Vec::new(),
    };

    let projection = gallery
        .context_selection_projection(&expression, &[shared_id])
        .await
        .unwrap();

    assert_eq!(projection.selected_context_count, 2);
    assert_eq!(projection.selected_work_count, 3);
    assert_eq!(projection.selected_visible_context_ids, vec![shared_id]);
    assert_eq!(
        sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM work WHERE id = ANY($1) ORDER BY pixiv_work_id"
        )
        .bind([first.id, second.id, third.id])
        .fetch_all(locked.db.pool())
        .await
        .unwrap(),
        [first.id, second.id, third.id]
    );
}

#[tokio::test]
async fn context_selection_inverts_the_complete_unloaded_query() {
    let locked = support::LockedDb::new().await;
    let works = WorkRepository::new(locked.db.clone());
    for (index, tag) in ["batch-one", "batch-two", "outside"]
        .into_iter()
        .enumerate()
    {
        seed_work(
            &locked.db,
            &works,
            WorkSeed {
                pixiv_id: 8_231 + index as i64,
                title: "context invert",
                artist_id: 231 + index as i64,
                artist_name: "context invert artist",
                tags: &[(tag, None)],
                bookmarks: 10,
                age_rating: PixivAgeRating::AllAge,
            },
        )
        .await;
    }
    let gallery = GalleryRepository::new(locked.db.clone());
    let visible = gallery.tags(1, None, Some("batch")).await.unwrap().items[0]
        .tag
        .id;
    let expression = GalleryContextSelectionExpression {
        kind: GalleryContextKind::Tag,
        query: "batch".to_owned(),
        base_selected: true,
        exception_context_ids: Vec::new(),
    };

    let projection = gallery
        .context_selection_projection(&expression, &[visible])
        .await
        .unwrap();

    assert_eq!(projection.selected_context_count, 2);
    assert_eq!(projection.selected_work_count, 2);
    assert_eq!(projection.selected_visible_context_ids, vec![visible]);
}

#[tokio::test]
async fn gallery_context_reads_saved_derivative_color() {
    let locked = support::LockedDb::new().await;
    let works = WorkRepository::new(locked.db.clone());
    let first = seed_work(
        &locked.db,
        &works,
        WorkSeed {
            pixiv_id: 8_301,
            title: "first",
            artist_id: 230,
            artist_name: "context artist",
            tags: &[("shared", Some("共有"))],
            bookmarks: 40,
            age_rating: PixivAgeRating::AllAge,
        },
    )
    .await;
    let series_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO series (id, pixiv_series_id, title) VALUES ($1, 8301, 'context series')",
    )
    .bind(series_id)
    .execute(locked.db.pool())
    .await
    .unwrap();
    sqlx::query("UPDATE work SET series_id = $2 WHERE id = $1")
        .bind(first.id)
        .bind(series_id)
        .execute(locked.db.pool())
        .await
        .unwrap();

    let (first_media, first_derivative_id) =
        seed_waterfall_derivative(&locked.db, first.id, "first/thumb.webp").await;

    let gallery = GalleryRepository::new(locked.db.clone());
    let detail = gallery.work_detail(first.id, None).await.unwrap();
    assert_eq!(detail.pages.len(), 1);
    let current = detail.pages[0].current_media.as_ref().unwrap();
    assert_eq!(current.id, first_media);
    assert_eq!(current.derivatives[0].id, first_derivative_id);
    assert_eq!(current.derivatives[0].dominant_color, "#1450c8");

    let artist = gallery
        .artist_detail(detail.work.pixiv_artist_id)
        .await
        .unwrap();
    assert_eq!(artist.work_count, 1);
    assert_eq!(artist.cover_age_rating, Some(PixivAgeRating::AllAge));
    let tag = gallery
        .tag_detail(&detail.work.tags[0].original)
        .await
        .unwrap();
    assert_eq!(tag.work_count, 1);
    assert_eq!(tag.cover_age_rating, Some(PixivAgeRating::AllAge));
    let series = gallery.series_detail(8_301).await.unwrap();
    assert_eq!(series.work_count, 1);
    assert_eq!(series.cover_age_rating, Some(PixivAgeRating::AllAge));
    let series_page = gallery.series(1, None, None).await.unwrap();
    assert_eq!(series_page.total, 1);
    assert_eq!(series_page.next_cursor, None);
    assert_eq!(
        series_page.items[0].cover_age_rating,
        Some(PixivAgeRating::AllAge)
    );
    assert_eq!(gallery.revisions(first.id).await.unwrap().len(), 1);

    for filter in [
        GalleryFilter::TagId { value: tag.tag.id },
        GalleryFilter::SeriesId { value: series_id },
    ] {
        let context = gallery
            .search(
                GallerySearch {
                    groups: vec![GalleryFilterGroup {
                        mode: FilterMode::All,
                        filters: vec![filter],
                    }],
                    ..GallerySearch::default()
                },
                None,
            )
            .await
            .unwrap();
        assert_eq!(
            context
                .items
                .iter()
                .map(|item| item.pixiv_work_id)
                .collect::<Vec<_>>(),
            vec![8_301]
        );
    }
}

#[tokio::test]
async fn overview_decorations_respect_rating_and_collection_state() {
    let locked = support::LockedDb::new().await;
    let works = WorkRepository::new(locked.db.clone());
    let all_age = seed_work(
        &locked.db,
        &works,
        WorkSeed {
            pixiv_id: 8_401,
            title: "all age decoration",
            artist_id: 241,
            artist_name: "all age artist",
            tags: &[("all-age-decoration", None)],
            bookmarks: 10,
            age_rating: PixivAgeRating::AllAge,
        },
    )
    .await;
    let r18 = seed_work(
        &locked.db,
        &works,
        WorkSeed {
            pixiv_id: 8_402,
            title: "r18 decoration",
            artist_id: 242,
            artist_name: "r18 artist",
            tags: &[("r18-decoration", None)],
            bookmarks: 20,
            age_rating: PixivAgeRating::R18,
        },
    )
    .await;
    let unknown = seed_work(
        &locked.db,
        &works,
        WorkSeed {
            pixiv_id: 8_403,
            title: "unknown decoration",
            artist_id: 243,
            artist_name: "unknown artist",
            tags: &[("unknown-decoration", None)],
            bookmarks: 30,
            age_rating: PixivAgeRating::Unknown,
        },
    )
    .await;
    let trashed = seed_work(
        &locked.db,
        &works,
        WorkSeed {
            pixiv_id: 8_404,
            title: "trashed decoration",
            artist_id: 244,
            artist_name: "trashed artist",
            tags: &[("trashed-decoration", None)],
            bookmarks: 40,
            age_rating: PixivAgeRating::AllAge,
        },
    )
    .await;

    let second_all_age = seed_work(
        &locked.db,
        &works,
        WorkSeed {
            pixiv_id: 8_405,
            title: "second all age decoration",
            artist_id: 245,
            artist_name: "second all age artist",
            tags: &[("second-all-age-decoration", None)],
            bookmarks: 50,
            age_rating: PixivAgeRating::AllAge,
        },
    )
    .await;

    for (work_id, path) in [
        (all_age.id, "overview/all-age.webp"),
        (r18.id, "overview/r18.webp"),
        (unknown.id, "overview/unknown.webp"),
        (trashed.id, "overview/trashed.webp"),
    ] {
        seed_waterfall_derivative(&locked.db, work_id, path).await;
    }
    sqlx::query("UPDATE work SET collection_state = 'trash' WHERE id = $1")
        .bind(trashed.id)
        .execute(locked.db.pool())
        .await
        .unwrap();

    let gallery = GalleryRepository::new(locked.db.clone());
    let first_day = Date::from_calendar_date(2026, Month::August, 12).unwrap();
    let second_day = Date::from_calendar_date(2026, Month::August, 13).unwrap();
    let third_day = Date::from_calendar_date(2026, Month::August, 14).unwrap();
    let empty_day = Date::from_calendar_date(2026, Month::August, 15).unwrap();

    let first_selection = gallery
        .overview_decorations(first_day, false)
        .await
        .unwrap();
    assert_eq!(
        decoration_ids(&first_selection),
        vec![Some(8_401), Some(8_401), Some(8_401)]
    );

    seed_waterfall_derivative(
        &locked.db,
        second_all_age.id,
        "overview/second-all-age.webp",
    )
    .await;

    assert_eq!(
        gallery
            .overview_decorations(first_day, false)
            .await
            .unwrap(),
        first_selection
    );

    let next_selection = gallery
        .overview_decorations(second_day, false)
        .await
        .unwrap();
    let next_ids = decoration_ids(&next_selection);
    assert_eq!(next_ids.len(), 3);
    assert_eq!(next_ids[0], next_ids[2]);
    assert_ne!(next_ids[0], next_ids[1]);
    let mut next_unique = next_ids.into_iter().flatten().collect::<Vec<_>>();
    next_unique.sort_unstable();
    next_unique.dedup();
    assert_eq!(next_unique, vec![8_401, 8_405]);

    let permitted = gallery.overview_decorations(third_day, true).await.unwrap();
    let mut permitted = permitted
        .iter()
        .map(|item| {
            let item = item.as_ref().unwrap();
            (item.pixiv_work_id, item.age_rating)
        })
        .collect::<Vec<_>>();
    permitted.sort_by_key(|(pixiv_work_id, _)| *pixiv_work_id);
    assert_eq!(
        permitted,
        vec![
            (8_401, PixivAgeRating::AllAge),
            (8_402, PixivAgeRating::R18),
            (8_405, PixivAgeRating::AllAge),
        ]
    );

    let replacement = gallery
        .shuffle_overview_decorations(first_day, false)
        .await
        .unwrap();
    assert_ne!(replacement, first_selection);
    let replacement_ids = decoration_ids(&replacement);
    let unavailable_pixiv_id = replacement_ids[0].unwrap();
    sqlx::query(
        r#"
        DELETE FROM derivative
        WHERE media_revision_id = (
            SELECT work_page.current_media_revision_id
            FROM work_page
            JOIN work ON work.id = work_page.work_id
            WHERE work.pixiv_work_id = $1
              AND work_page.page_index = 0
        )
        "#,
    )
    .bind(unavailable_pixiv_id)
    .execute(locked.db.pool())
    .await
    .unwrap();
    let unavailable = gallery
        .overview_decorations(first_day, false)
        .await
        .unwrap();
    for (before, after) in replacement_ids.iter().zip(unavailable.iter()) {
        if *before == Some(unavailable_pixiv_id) {
            assert!(after.is_none());
        } else {
            assert_eq!(after.as_ref().map(|item| item.pixiv_work_id), *before);
        }
    }

    sqlx::query("UPDATE work SET collection_state = 'trash'")
        .execute(locked.db.pool())
        .await
        .unwrap();
    let empty = gallery
        .overview_decorations(empty_day, false)
        .await
        .unwrap();
    assert_eq!(empty.len(), 3);
    assert!(empty.iter().all(Option::is_none));
    let stored_empty_positions: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM overview_decoration_selection WHERE selection_date = $1",
    )
    .bind(empty_day)
    .fetch_one(locked.db.pool())
    .await
    .unwrap();
    assert_eq!(stored_empty_positions, 3);
}

fn decoration_ids(
    decorations: &[Option<pixivarchive_domain::work::GalleryOverviewDecoration>],
) -> Vec<Option<i64>> {
    decorations
        .iter()
        .map(|item| item.as_ref().map(|item| item.pixiv_work_id))
        .collect()
}

async fn expire_lease(db: &Db, job_id: Uuid) {
    sqlx::query("UPDATE job SET lease_expires_at = now() - interval '1 second' WHERE id = $1")
        .bind(job_id)
        .execute(db.pool())
        .await
        .unwrap();
}

async fn seed_waterfall_derivative(db: &Db, work_id: Uuid, path: &str) -> (Uuid, Uuid) {
    let media_revision_id: Uuid = sqlx::query_scalar(
        "SELECT current_media_revision_id FROM work_page WHERE work_id = $1 AND page_index = 0",
    )
    .bind(work_id)
    .fetch_one(db.pool())
    .await
    .unwrap();
    let jobs = JobRepository::new(db.clone());
    let generation_job_id = jobs
        .enqueue(NewJob::for_kind(
            JobPriority::BackgroundMaintenance,
            JobKind::GenerateDerivative,
            json!({ "media_revision_id": media_revision_id }),
        ))
        .await
        .unwrap();
    let claimed = jobs
        .claim_next(
            Uuid::now_v7(),
            &JobQuotaSelection::new(vec![JobPriority::BackgroundMaintenance]),
            time::Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(claimed.id, generation_job_id);
    let derivative_path = PathBuf::from(path);
    let media = MediaRepository::new(db.clone());
    media
        .register_artifact_intent(claimed.lease(), &derivative_path)
        .await
        .unwrap();
    let derivative_id = media
        .save_derivative(SaveDerivative {
            lease: claimed.lease(),
            media_revision_id,
            kind: DerivativeKind::WaterfallThumbnail,
            format: DerivativeFormat::Webp,
            relative_path: derivative_path,
            dimensions: MediaDimensions {
                width: 320,
                height: 480,
            },
            byte_size: 64,
            dominant_color: "#1450c8".to_owned(),
            complete_job: false,
        })
        .await
        .unwrap();
    (media_revision_id, derivative_id)
}

struct WorkSeed<'a> {
    pixiv_id: i64,
    title: &'a str,
    artist_id: i64,
    artist_name: &'a str,
    tags: &'a [(&'a str, Option<&'a str>)],
    bookmarks: u64,
    age_rating: PixivAgeRating,
}

async fn seed_work(
    db: &Db,
    works: &WorkRepository,
    seed: WorkSeed<'_>,
) -> pixivarchive_domain::job::WorkSummary {
    let published_at = PrimitiveDateTime::new(
        Date::from_calendar_date(2026, Month::January, 2).unwrap(),
        Time::MIDNIGHT,
    )
    .assume_utc();
    let saved = works
        .save_pixiv_metadata(SavePixivWorkMetadata {
            account_id: None,
            detail: PixivWorkDetail {
                work_id: seed.pixiv_id,
                title: seed.title.to_owned(),
                description: format!("{} description", seed.title),
                kind: PixivWorkKind::Illustration,
                age_rating: seed.age_rating,
                ai_classification: PixivAiClassification::NotAiGenerated,
                is_original: true,
                artist: PixivArtistRef {
                    pixiv_id: seed.artist_id,
                    name: seed.artist_name.to_owned(),
                    account_name: None,
                },
                published_at: Some(published_at),
                updated_at: Some(published_at),
                tags: seed
                    .tags
                    .iter()
                    .map(|(name, translated)| PixivTag {
                        name: (*name).to_owned(),
                        translated_name: translated.map(str::to_owned),
                    })
                    .collect(),
                page_count: 1,
                dimensions: PixivDimensions {
                    width: 1_200,
                    height: 1_600,
                },
                counts: PixivWorkCounts {
                    bookmarks: seed.bookmarks,
                    likes: 10,
                    comments: 1,
                    views: 2_000,
                },
                bookmarked_by_current_account: Some(false),
                bookmark: None,
                series: None,
            },
            pages: PixivWorkPages {
                work_id: seed.pixiv_id,
                pages: vec![PixivWorkPage {
                    page_index: 0,
                    original_url: Url::parse(&format!(
                        "https://i.pximg.net/{}/original.png",
                        seed.pixiv_id
                    ))
                    .unwrap(),
                    dimensions: PixivDimensions {
                        width: 1_200,
                        height: 1_600,
                    },
                    format_hint: Some(pixivarchive_domain::pixiv::PixivImageFormat::Png),
                }],
            },
            ugoira: None,
            provenance: json!({"test": true}),
            revision_source: None,
        })
        .await
        .unwrap();
    let page_id: Uuid =
        sqlx::query_scalar("SELECT id FROM work_page WHERE work_id = $1 AND page_index = 0")
            .bind(saved.id)
            .fetch_one(db.pool())
            .await
            .unwrap();
    let media_id = Uuid::now_v7();
    sqlx::query(
        r#"
        INSERT INTO media_revision (
            id, work_page_id, revision_number, media_kind, format,
            source_path, byte_size, sha256
        )
        VALUES ($1, $2, 1, 'source_image', 'png', $3, 1, $4)
        "#,
    )
    .bind(media_id)
    .bind(page_id)
    .bind(format!("seed/{}/source.png", seed.pixiv_id))
    .bind(vec![0_u8; 32])
    .execute(db.pool())
    .await
    .unwrap();
    sqlx::query("UPDATE work_page SET current_media_revision_id = $2 WHERE id = $1")
        .bind(page_id)
        .bind(media_id)
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query("UPDATE work SET collection_state = 'collected' WHERE id = $1")
        .bind(saved.id)
        .execute(db.pool())
        .await
        .unwrap();
    saved
}
