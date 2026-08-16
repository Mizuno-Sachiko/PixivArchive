use crate::Db;

mod model;
mod queue;
mod runs;

pub use model::{CreateImportRun, ImportRunRecord, ImportRunSummaryRecord, QueueImportRequest};

#[derive(Clone)]
pub struct ImportRepository {
    db: Db,
}

impl ImportRepository {
    pub fn new(db: Db) -> Self {
        Self { db }
    }
}
