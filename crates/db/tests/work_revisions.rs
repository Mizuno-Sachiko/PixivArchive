mod support;

use pixivarchive_db::{
    GalleryRepository, SavePixivWorkMetadata, WorkRepository, WorkRevisionSourceInput,
};
use pixivarchive_domain::pixiv::{
    PixivAgeRating, PixivAiClassification, PixivArtistRef, PixivDimensions, PixivImageFormat,
    PixivTag, PixivWorkCounts, PixivWorkDetail, PixivWorkKind, PixivWorkPage, PixivWorkPages,
};
use serde_json::json;
use url::Url;
use uuid::Uuid;

#[tokio::test]
async fn revisions_keep_subscription_snapshots_and_leave_unchanged_basis_alone() {
    let locked = support::LockedDb::new().await;
    let (subscription_a, run_a) = create_source(&locked.db, 810_001, "首个来源").await;
    let (subscription_b, run_b) = create_source(&locked.db, 810_002, "第二个来源").await;
    let (subscription_c, run_c) = create_source(&locked.db, 810_003, "附加来源").await;
    let works = WorkRepository::new(locked.db.clone());
    let gallery = GalleryRepository::new(locked.db.clone());
    let work_id = 81_001;

    let saved = works
        .save_pixiv_metadata(metadata(
            work_id,
            "初始标题",
            Some(WorkRevisionSourceInput {
                subscription_id: subscription_a,
                subscription_run_id: run_a,
                subscription_name: "首个来源".to_owned(),
                pixiv_user_id: 810_001,
            }),
        ))
        .await
        .unwrap();
    works
        .save_pixiv_metadata(metadata(
            work_id,
            "第二个标题",
            Some(WorkRevisionSourceInput {
                subscription_id: subscription_b,
                subscription_run_id: run_b,
                subscription_name: "改名后的第二个来源".to_owned(),
                pixiv_user_id: 810_002,
            }),
        ))
        .await
        .unwrap();

    let unchanged = works
        .save_pixiv_metadata(metadata(
            work_id,
            "第二个标题",
            Some(WorkRevisionSourceInput {
                subscription_id: subscription_a,
                subscription_run_id: run_a,
                subscription_name: "重复观察".to_owned(),
                pixiv_user_id: 810_001,
            }),
        ))
        .await
        .unwrap();
    assert_eq!(unchanged.pixiv_id, work_id);

    sqlx::query("UPDATE subscription SET name = '当前订阅名称' WHERE id = $1")
        .bind(subscription_a)
        .execute(locked.db.pool())
        .await
        .unwrap();
    let first_revision_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM work_revision WHERE work_id = $1 AND title = '初始标题'",
    )
    .bind(saved.id)
    .fetch_one(locked.db.pool())
    .await
    .unwrap();
    sqlx::query(
        r#"
        INSERT INTO work_revision_source (
            id,
            work_revision_id,
            subscription_id,
            subscription_run_id,
            subscription_name,
            pixiv_user_id
        )
        VALUES ($1, $2, $3, $4, '附加来源快照', 810003)
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(first_revision_id)
    .bind(subscription_c)
    .bind(run_c)
    .execute(locked.db.pool())
    .await
    .unwrap();

    let revisions = gallery.revisions(unchanged.id).await.unwrap();
    assert_eq!(revisions.len(), 2);
    let first = revisions
        .iter()
        .find(|revision| revision.title == "初始标题")
        .unwrap();
    assert_eq!(first.sources.len(), 2);
    assert!(first.sources.iter().any(|source| {
        source.subscription_name == "首个来源" && source.pixiv_user_id == 810_001
    }));
    assert!(first.sources.iter().any(|source| {
        source.subscription_name == "附加来源快照" && source.pixiv_user_id == 810_003
    }));
    let second = revisions
        .iter()
        .find(|revision| revision.title == "第二个标题")
        .unwrap();
    assert_eq!(second.sources.len(), 1);
    assert_eq!(second.sources[0].subscription_name, "改名后的第二个来源");
    assert_eq!(second.sources[0].pixiv_user_id, 810_002);

    sqlx::query("DELETE FROM subscription WHERE id = $1")
        .bind(subscription_b)
        .execute(locked.db.pool())
        .await
        .unwrap();
    let after_subscription_delete = gallery.revisions(unchanged.id).await.unwrap();
    let retained = after_subscription_delete
        .iter()
        .find(|revision| revision.title == "第二个标题")
        .unwrap();
    assert_eq!(retained.sources[0].subscription_name, "改名后的第二个来源");
    assert_eq!(retained.sources[0].pixiv_user_id, 810_002);
}

#[tokio::test]
async fn revisions_saved_without_a_subscription_have_no_sources() {
    let locked = support::LockedDb::new().await;
    let saved = WorkRepository::new(locked.db.clone())
        .save_pixiv_metadata(metadata(81_002, "手动导入", None))
        .await
        .unwrap();

    let revisions = GalleryRepository::new(locked.db.clone())
        .revisions(saved.id)
        .await
        .unwrap();

    assert_eq!(revisions.len(), 1);
    assert!(revisions[0].sources.is_empty());
}

async fn create_source(db: &pixivarchive_db::Db, pixiv_user_id: i64, name: &str) -> (Uuid, Uuid) {
    let account_id = Uuid::now_v7();
    let subscription_id = Uuid::now_v7();
    let run_id = Uuid::now_v7();
    sqlx::query(
        r#"
        INSERT INTO pixiv_account (id, pixiv_user_id, display_name, state)
        VALUES ($1, $2, $3, 'unconfigured')
        "#,
    )
    .bind(account_id)
    .bind(pixiv_user_id)
    .bind(name)
    .execute(db.pool())
    .await
    .unwrap();
    sqlx::query(
        r#"
        INSERT INTO subscription (id, pixiv_account_id, name, kind, schedule, params)
        VALUES ($1, $2, $3, 'ranking', '{}'::jsonb, '{}'::jsonb)
        "#,
    )
    .bind(subscription_id)
    .bind(account_id)
    .bind(name)
    .execute(db.pool())
    .await
    .unwrap();
    sqlx::query(
        r#"
        INSERT INTO subscription_run (id, subscription_id, trigger_kind, state)
        VALUES ($1, $2, 'manual', 'succeeded')
        "#,
    )
    .bind(run_id)
    .bind(subscription_id)
    .execute(db.pool())
    .await
    .unwrap();
    (subscription_id, run_id)
}

fn metadata(
    work_id: i64,
    title: &str,
    revision_source: Option<WorkRevisionSourceInput>,
) -> SavePixivWorkMetadata {
    let timestamp = time::OffsetDateTime::from_unix_timestamp(1_750_000_000).unwrap();
    SavePixivWorkMetadata {
        account_id: None,
        detail: PixivWorkDetail {
            work_id,
            title: title.to_owned(),
            description: "测试描述".to_owned(),
            kind: PixivWorkKind::Illustration,
            age_rating: PixivAgeRating::AllAge,
            ai_classification: PixivAiClassification::NotAiGenerated,
            is_original: true,
            artist: PixivArtistRef {
                pixiv_id: 81_010,
                name: "测试作者".to_owned(),
                account_name: None,
            },
            published_at: Some(timestamp),
            updated_at: Some(timestamp),
            tags: vec![PixivTag {
                name: "测试".to_owned(),
                translated_name: None,
            }],
            page_count: 1,
            dimensions: PixivDimensions {
                width: 800,
                height: 1_200,
            },
            counts: PixivWorkCounts {
                bookmarks: 10,
                views: 100,
                likes: 8,
                comments: 1,
            },
            bookmarked_by_current_account: None,
            bookmark: None,
            series: None,
        },
        pages: PixivWorkPages {
            work_id,
            pages: vec![PixivWorkPage {
                page_index: 0,
                original_url: Url::parse("https://i.pximg.net/test.png").unwrap(),
                dimensions: PixivDimensions {
                    width: 800,
                    height: 1_200,
                },
                format_hint: Some(PixivImageFormat::Png),
            }],
        },
        ugoira: None,
        provenance: json!({"test": true}),
        revision_source,
    }
}
