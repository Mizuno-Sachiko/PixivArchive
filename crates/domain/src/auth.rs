use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionContext {
    pub administrator_id: Uuid,
    pub session_id: Uuid,
    pub expires_at: OffsetDateTime,
}

#[derive(Clone, Eq, PartialEq)]
pub struct AdministratorRecord {
    pub id: Uuid,
    pub username: String,
    pub password_phc: String,
    pub password_version: i64,
    pub revision: i64,
}
