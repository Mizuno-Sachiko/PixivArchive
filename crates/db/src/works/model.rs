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
}
