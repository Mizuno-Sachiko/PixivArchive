mod artists;
mod model;
mod overview;
mod query;
mod search;
mod selection;
mod series;
mod tags;
mod work_detail;

use crate::{Db, DbError};
pub(crate) use query::{matching_work_query, push_selection_state, selection_projection_query};
pub(crate) use selection::context_selected_work_query;

const MAX_PAGE_SIZE: u16 = 200;

#[derive(Clone)]
pub struct GalleryRepository {
    db: Db,
}

impl GalleryRepository {
    pub fn new(db: Db) -> Self {
        Self { db }
    }
}

fn validate_source_id(value: i64, name: &str) -> Result<(), DbError> {
    if value > 0 {
        Ok(())
    } else {
        Err(DbError::InvalidValue(format!("{name} must be positive")))
    }
}
