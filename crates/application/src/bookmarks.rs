use crate::pixiv_accounts::{PixivAccountContextError, PixivAccountContextFactory};
use async_trait::async_trait;
use pixivarchive_db::{
    BookmarkRepository, Db, DbError, PixivAccountRepository, RecordBookmarkWriteback,
};
use pixivarchive_domain::pixiv::{PixivBookmarkAddRequest, PixivBookmarkVisibility};
use pixivarchive_pixiv::{PixivErrorClass, PixivGateway, PixivRequestContext};
use serde_json::json;
use std::sync::Arc;
use thiserror::Error;
use uuid::Uuid;

#[async_trait]
pub trait BookmarkCommandPort: Send + Sync {
    async fn add(
        &self,
        request: BookmarkCommandRequest,
    ) -> Result<BookmarkWritebackResult, BookmarkWritebackError>;

    async fn remove(
        &self,
        request: BookmarkCommandRequest,
    ) -> Result<BookmarkWritebackResult, BookmarkWritebackError>;
}

#[derive(Clone, Default)]
pub struct DisabledBookmarkCommandPort;

#[async_trait]
impl BookmarkCommandPort for DisabledBookmarkCommandPort {
    async fn add(
        &self,
        _request: BookmarkCommandRequest,
    ) -> Result<BookmarkWritebackResult, BookmarkWritebackError> {
        Ok(BookmarkWritebackResult::disabled())
    }

    async fn remove(
        &self,
        _request: BookmarkCommandRequest,
    ) -> Result<BookmarkWritebackResult, BookmarkWritebackError> {
        Ok(BookmarkWritebackResult::disabled())
    }
}

#[derive(Clone)]
pub struct BookmarkWritebackService<G> {
    accounts: PixivAccountRepository,
    bookmarks: BookmarkRepository,
    gateway: Arc<G>,
}

#[derive(Clone)]
pub struct LiveBookmarkCommandPort<G> {
    accounts: PixivAccountRepository,
    service: BookmarkWritebackService<G>,
    context_factory: PixivAccountContextFactory,
}

impl<G> LiveBookmarkCommandPort<G>
where
    G: PixivGateway + Clone + 'static,
{
    pub fn new(db: Db, gateway: G, context_factory: PixivAccountContextFactory) -> Self {
        Self {
            accounts: PixivAccountRepository::new(db.clone()),
            service: BookmarkWritebackService::new(db, gateway),
            context_factory,
        }
    }

    async fn execute(
        &self,
        request: BookmarkCommandRequest,
        operation: BookmarkWritebackOperation,
    ) -> Result<BookmarkWritebackResult, BookmarkWritebackError> {
        let account = self.accounts.require_current(request.account_id).await?;
        if !account.bookmark_writeback_enabled {
            return self.service.record_disabled(request, operation).await;
        }
        let context = self.context_factory.context_for_record(&account)?;
        self.service
            .execute_enabled(
                BookmarkWritebackRequest {
                    account_id: request.account_id,
                    context,
                    target_pixiv_id: request.target_pixiv_id,
                    visibility: request.visibility,
                    tags: request.tags,
                },
                operation,
            )
            .await
    }
}

#[async_trait]
impl<G> BookmarkCommandPort for LiveBookmarkCommandPort<G>
where
    G: PixivGateway + Clone + 'static,
{
    async fn add(
        &self,
        request: BookmarkCommandRequest,
    ) -> Result<BookmarkWritebackResult, BookmarkWritebackError> {
        self.execute(request, BookmarkWritebackOperation::Add).await
    }

    async fn remove(
        &self,
        request: BookmarkCommandRequest,
    ) -> Result<BookmarkWritebackResult, BookmarkWritebackError> {
        self.execute(request, BookmarkWritebackOperation::Remove)
            .await
    }
}

impl<G> BookmarkWritebackService<G>
where
    G: PixivGateway + 'static,
{
    pub fn new(db: Db, gateway: G) -> Self {
        Self {
            accounts: PixivAccountRepository::new(db.clone()),
            bookmarks: BookmarkRepository::new(db),
            gateway: Arc::new(gateway),
        }
    }

    pub async fn add(
        &self,
        request: BookmarkWritebackRequest,
    ) -> Result<BookmarkWritebackResult, BookmarkWritebackError> {
        self.execute(request, BookmarkWritebackOperation::Add).await
    }

    pub async fn remove(
        &self,
        request: BookmarkWritebackRequest,
    ) -> Result<BookmarkWritebackResult, BookmarkWritebackError> {
        self.execute(request, BookmarkWritebackOperation::Remove)
            .await
    }

    async fn execute(
        &self,
        request: BookmarkWritebackRequest,
        operation: BookmarkWritebackOperation,
    ) -> Result<BookmarkWritebackResult, BookmarkWritebackError> {
        let account = self.accounts.get(request.account_id).await?;
        if !account.bookmark_writeback_enabled {
            return self
                .record_disabled(
                    BookmarkCommandRequest {
                        account_id: request.account_id,
                        target_pixiv_id: request.target_pixiv_id,
                        visibility: request.visibility,
                        tags: request.tags,
                    },
                    operation,
                )
                .await;
        }
        if request.context.user_id() != account.pixiv_user_id {
            return Err(BookmarkWritebackError::AccountMismatch);
        }
        self.execute_enabled(request, operation).await
    }

    async fn record_disabled(
        &self,
        request: BookmarkCommandRequest,
        operation: BookmarkWritebackOperation,
    ) -> Result<BookmarkWritebackResult, BookmarkWritebackError> {
        self.accounts
            .record_bookmark_writeback(RecordBookmarkWriteback {
                account_id: request.account_id,
                operation: operation.as_str(),
                target_pixiv_id: request.target_pixiv_id,
                status: "disabled",
                error_class: None,
                result: json!({}),
            })
            .await?;
        Ok(BookmarkWritebackResult {
            status: BookmarkWritebackStatus::Disabled,
            bookmark_id: None,
            error_class: None,
        })
    }

    async fn execute_enabled(
        &self,
        request: BookmarkWritebackRequest,
        operation: BookmarkWritebackOperation,
    ) -> Result<BookmarkWritebackResult, BookmarkWritebackError> {
        let local_bookmark_id = if operation == BookmarkWritebackOperation::Remove {
            self.bookmarks
                .active_bookmark_id(request.account_id, request.target_pixiv_id)
                .await?
        } else {
            None
        };
        let result = match operation {
            BookmarkWritebackOperation::Add => self
                .gateway
                .add_bookmark(
                    &request.context,
                    PixivBookmarkAddRequest {
                        work_id: request.target_pixiv_id,
                        visibility: request.visibility,
                        tags: request.tags,
                    },
                )
                .await
                .map(|response| response.value.bookmark_id),
            BookmarkWritebackOperation::Remove => {
                let resolved_bookmark_id = match local_bookmark_id {
                    Some(bookmark_id) => Ok(Some(bookmark_id)),
                    None => self
                        .gateway
                        .work_detail(&request.context, request.target_pixiv_id)
                        .await
                        .map(|response| {
                            response.value.bookmark.map(|bookmark| bookmark.bookmark_id)
                        }),
                };
                match resolved_bookmark_id {
                    Ok(Some(bookmark_id)) => match self
                        .gateway
                        .delete_bookmark(&request.context, bookmark_id)
                        .await
                    {
                        Ok(response) => Ok(response.value.bookmark_id),
                        Err(error) if error.class() == PixivErrorClass::HiddenOrNotFound => {
                            Ok(None)
                        }
                        Err(error) => Err(error),
                    },
                    Ok(None) => Ok(None),
                    Err(error) => Err(error),
                }
            }
        };

        match result {
            Ok(bookmark_id) => {
                match operation {
                    BookmarkWritebackOperation::Add => {
                        self.bookmarks
                            .mark_added(
                                request.account_id,
                                request.target_pixiv_id,
                                bookmark_id,
                                request.visibility,
                            )
                            .await?;
                    }
                    BookmarkWritebackOperation::Remove => {
                        self.bookmarks
                            .mark_removed_by_work(request.account_id, request.target_pixiv_id)
                            .await?;
                    }
                }
                self.accounts
                    .record_bookmark_writeback(RecordBookmarkWriteback {
                        account_id: request.account_id,
                        operation: operation.as_str(),
                        target_pixiv_id: request.target_pixiv_id,
                        status: "succeeded",
                        error_class: None,
                        result: json!({ "bookmark_id": bookmark_id }),
                    })
                    .await?;
                Ok(BookmarkWritebackResult {
                    status: BookmarkWritebackStatus::Succeeded,
                    bookmark_id,
                    error_class: None,
                })
            }
            Err(error) => {
                let error_class = error.class().as_str().to_owned();
                self.accounts
                    .record_bookmark_writeback(RecordBookmarkWriteback {
                        account_id: request.account_id,
                        operation: operation.as_str(),
                        target_pixiv_id: request.target_pixiv_id,
                        status: "failed",
                        error_class: Some(error_class.clone()),
                        result: json!({}),
                    })
                    .await?;
                Ok(BookmarkWritebackResult {
                    status: BookmarkWritebackStatus::Failed,
                    bookmark_id: None,
                    error_class: Some(error_class),
                })
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct BookmarkCommandRequest {
    pub account_id: Uuid,
    pub target_pixiv_id: i64,
    pub visibility: PixivBookmarkVisibility,
    pub tags: Vec<String>,
}

pub struct BookmarkWritebackRequest {
    pub account_id: Uuid,
    pub context: PixivRequestContext,
    pub target_pixiv_id: i64,
    pub visibility: PixivBookmarkVisibility,
    pub tags: Vec<String>,
}

impl BookmarkWritebackRequest {
    pub fn add(
        account_id: Uuid,
        context: PixivRequestContext,
        work_id: i64,
        visibility: PixivBookmarkVisibility,
        tags: Vec<String>,
    ) -> Self {
        Self {
            account_id,
            context,
            target_pixiv_id: work_id,
            visibility,
            tags,
        }
    }

    pub fn remove(account_id: Uuid, context: PixivRequestContext, work_id: i64) -> Self {
        Self {
            account_id,
            context,
            target_pixiv_id: work_id,
            visibility: PixivBookmarkVisibility::Private,
            tags: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BookmarkWritebackResult {
    pub status: BookmarkWritebackStatus,
    pub bookmark_id: Option<i64>,
    pub error_class: Option<String>,
}

impl BookmarkWritebackResult {
    fn disabled() -> Self {
        Self {
            status: BookmarkWritebackStatus::Disabled,
            bookmark_id: None,
            error_class: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BookmarkWritebackStatus {
    Disabled,
    Succeeded,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BookmarkWritebackOperation {
    Add,
    Remove,
}

impl BookmarkWritebackOperation {
    fn as_str(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Remove => "remove",
        }
    }
}

#[derive(Debug, Error)]
pub enum BookmarkWritebackError {
    #[error("bookmark writeback storage failed")]
    Storage(#[from] DbError),
    #[error("Pixiv account request context is unavailable")]
    Context(#[from] PixivAccountContextError),
    #[error("bookmark writeback context does not match the target Pixiv account")]
    AccountMismatch,
}
