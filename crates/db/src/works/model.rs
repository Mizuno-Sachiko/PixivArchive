use pixivarchive_domain::pixiv::{PixivUgoiraMeta, PixivWorkDetail, PixivWorkPages};
use serde_json::Value;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct SavePixivWorkMetadata {
    pub account_id: Option<Uuid>,
    pub detail: PixivWorkDetail,
    pub pages: PixivWorkPages,
    pub ugoira: Option<PixivUgoiraMeta>,
    pub provenance: Value,
    pub revision_source: Option<WorkRevisionSourceInput>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WorkRevisionSourceInput {
    pub subscription_id: Uuid,
    pub subscription_run_id: Uuid,
    pub subscription_name: String,
    pub pixiv_user_id: i64,
}
