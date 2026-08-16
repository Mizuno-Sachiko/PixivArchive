use pixivarchive_db::Db;

pub struct LockedDb {
    pub db: Db,
    _locked: pixivarchive_test_support::LockedDb,
}

impl LockedDb {
    pub async fn new() -> Self {
        let locked =
            pixivarchive_test_support::LockedDb::new(pixivarchive_test_support::WORKER_LOCK_ID)
                .await;
        Self {
            db: locked.db.clone(),
            _locked: locked,
        }
    }
}
