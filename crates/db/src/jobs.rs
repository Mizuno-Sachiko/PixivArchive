use crate::Db;

#[derive(Clone)]
pub struct JobRepository {
    pub(super) db: Db,
}

mod account_wait;
mod claiming;
mod enqueue;
mod listing;
mod model;
mod transitions;

pub use model::{
    ImportJobCompletion, JobAttemptRecord, JobCompletion, JobHeartbeatRecord, JobRecord, JobStats,
};

impl JobRepository {
    pub fn new(db: Db) -> Self {
        Self { db }
    }
}
