use crate::Db;

#[derive(Clone)]
pub struct WorkRepository {
    pub(super) db: Db,
}

mod deletion_markers;
mod metadata;
mod model;
mod trash;

pub use model::{SavePixivWorkMetadata, WorkRevisionSourceInput};
pub(crate) use trash::{
    load_trash_action_capabilities, trash_selection_ctes, validated_trash_batch_ids,
};

impl WorkRepository {
    pub fn new(db: Db) -> Self {
        Self { db }
    }
}
