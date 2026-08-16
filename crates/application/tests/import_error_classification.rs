use pixivarchive_application::imports::ImportServiceError;
use pixivarchive_db::DbError;
use pixivarchive_domain::job::JobErrorClass;

#[test]
fn import_storage_errors_keep_retryable_database_failures_distinct() {
    assert_eq!(
        ImportServiceError::Storage(DbError::RevisionConflict).error_class(),
        JobErrorClass::Server
    );
    assert_eq!(
        ImportServiceError::Storage(DbError::InvalidValue("bad payload".to_owned())).error_class(),
        JobErrorClass::Permanent
    );
}
