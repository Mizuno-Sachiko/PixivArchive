use pixivarchive_application::pixiv_accounts::{
    PixivAccountContextError, PixivAccountContextFactory, PixivCookieCipher,
    PixivCookieCipherError, PixivCookieKeyConfig, PixivCookieKeyringConfig,
};
use pixivarchive_db::DbError;
use pixivarchive_worker::executors::subscription::PixivContextProvider;
use secrecy::SecretString;
use uuid::Uuid;

mod support;

use support::LockedDb;

#[tokio::test]
async fn worker_context_provider_loads_accounts_by_id() {
    let locked = LockedDb::new().await;
    let cipher = PixivCookieCipher::new(PixivCookieKeyringConfig::new(PixivCookieKeyConfig::new(
        "primary", [5; 32],
    )))
    .unwrap();
    let factory = PixivAccountContextFactory::new(locked.db.clone(), cipher.clone());

    let missing = PixivContextProvider::context_for_account(&factory, Uuid::now_v7())
        .await
        .unwrap_err();
    assert!(matches!(
        missing,
        PixivAccountContextError::Storage(DbError::NotFound)
    ));

    let account_id = Uuid::now_v7();
    sqlx::query(
        r#"
        INSERT INTO pixiv_account (
            id, pixiv_user_id, display_name, state, cookie_key_id,
            cookie_nonce, cookie_ciphertext, user_agent
        )
        VALUES ($1, 10001, 'Test Artist', 'normal', 'primary', $2, $3, $4)
        "#,
    )
    .bind(account_id)
    .bind(vec![0_u8; 12])
    .bind(vec![1_u8])
    .bind("PixivArchiveWorkerTest/1.0")
    .execute(locked.db.pool())
    .await
    .unwrap();

    let invalid = PixivContextProvider::context_for_account(&factory, account_id)
        .await
        .unwrap_err();
    assert!(matches!(
        invalid,
        PixivAccountContextError::Cipher(PixivCookieCipherError::InvalidCredential)
    ));

    let encrypted = cipher
        .encrypt(
            10_001,
            &SecretString::from("PHPSESSID=10001_worker-session"),
        )
        .unwrap();
    sqlx::query(
        r#"
        UPDATE pixiv_account
        SET cookie_nonce = $2,
            cookie_ciphertext = $3
        WHERE id = $1
        "#,
    )
    .bind(account_id)
    .bind(encrypted.nonce.to_vec())
    .bind(encrypted.ciphertext)
    .execute(locked.db.pool())
    .await
    .unwrap();

    let context = PixivContextProvider::context_for_account(&factory, account_id)
        .await
        .unwrap();
    assert_eq!(context.user_id(), 10_001);
    assert_eq!(context.user_agent(), "PixivArchiveWorkerTest/1.0");
    assert_eq!(
        context.cookie_header_value().unwrap(),
        "PHPSESSID=10001_worker-session"
    );
}
