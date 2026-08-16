use super::{
    AdapterResponse, PixivGateway, PixivRequestContext, PixivWebClient, ResponseProvenance,
    response::{
        csrf_header_value, extract_csrf_token, html_headers, json_headers, read_json_response,
        read_limited, redact_write_error, require_positive_id, temporary_cache_error,
        validate_context,
    },
};
use crate::{
    error::{PixivError, PixivErrorClass, classify_http_status},
    limit::PixivRequestPermit,
    mapper::{
        has_private_bookmark_evidence, map_account_profile, map_add_artist_follow_result,
        map_artist_follow_state, map_artist_work_ids, map_bookmark_write_result, map_bookmarks,
        map_follow_latest, map_followed_artists, map_ranking_page, map_remove_artist_follow_result,
        map_ugoira_meta, map_work_detail, map_work_pages,
    },
    web::{
        ADAPTER_VERSION, PixivEndpoint, add_artist_follow_url, add_bookmark_url,
        artist_follow_state_url, bookmarks_url, csrf_url, delete_bookmark_url, follow_latest_url,
        following_url, profile_all_url, profile_url, ranking_url, remove_artist_follow_url,
        ugoira_meta_url, work_detail_url, work_pages_url,
    },
};
use async_trait::async_trait;
use pixivarchive_domain::pixiv::{
    PixivAccountValidation, PixivArtistFollowRequest, PixivArtistFollowState,
    PixivArtistFollowWriteResult, PixivArtistWorkIds, PixivBookmarkAddRequest,
    PixivBookmarkVisibility, PixivBookmarkWriteResult, PixivBookmarksCursor, PixivBookmarksMode,
    PixivBookmarksRequest, PixivDiscoveryWork, PixivFollowLatestCursor, PixivFollowLatestRequest,
    PixivFollowedArtist, PixivFollowingCursor, PixivFollowingRequest, PixivFollowingVisibility,
    PixivPage, PixivRankingPage, PixivRankingRequest, PixivUgoiraMeta, PixivWorkDetail,
    PixivWorkPages,
};
use reqwest::header::{CONTENT_TYPE, HeaderValue, RETRY_AFTER};
use secrecy::SecretString;
use serde_json::{Value, json};
use std::sync::Arc;
use url::{Url, form_urlencoded};

#[derive(Clone, Copy)]
enum ArtistFollowWrite {
    Add {
        artist_id: i64,
        visibility: PixivFollowingVisibility,
    },
    Remove {
        artist_id: i64,
    },
}

impl ArtistFollowWrite {
    fn artist_id(self) -> i64 {
        match self {
            Self::Add { artist_id, .. } | Self::Remove { artist_id } => artist_id,
        }
    }

    fn endpoint(self) -> PixivEndpoint {
        match self {
            Self::Add { .. } => PixivEndpoint::AddArtistFollow,
            Self::Remove { .. } => PixivEndpoint::RemoveArtistFollow,
        }
    }

    fn form_body(self) -> String {
        let mut body = form_urlencoded::Serializer::new(String::new());
        match self {
            Self::Add {
                artist_id,
                visibility,
            } => {
                body.append_pair("mode", "add")
                    .append_pair("type", "user")
                    .append_pair("user_id", &artist_id.to_string())
                    .append_pair("tag", "")
                    .append_pair(
                        "restrict",
                        match visibility {
                            PixivFollowingVisibility::Public => "0",
                            PixivFollowingVisibility::Private => "1",
                        },
                    )
                    .append_pair("format", "json");
            }
            Self::Remove { artist_id } => {
                body.append_pair("mode", "del")
                    .append_pair("type", "bookuser")
                    .append_pair("id", &artist_id.to_string());
            }
        }
        body.finish()
    }
}

impl PixivWebClient {
    async fn metadata_request_permit(&self) -> Option<PixivRequestPermit> {
        match &self.options.metadata_request_gate {
            Some(gate) => Some(gate.enter().await),
            None => None,
        }
    }

    async fn get_json(
        &self,
        context: &PixivRequestContext,
        endpoint: PixivEndpoint,
        url: Url,
    ) -> Result<Value, PixivError> {
        let _request_permit = self.metadata_request_permit().await;
        let headers = json_headers(context, endpoint)?;
        let response = self
            .http
            .get(url)
            .headers(headers)
            .send()
            .await
            .map_err(|_| PixivError::network(endpoint))?;
        read_json_response(response, endpoint, self.options.metadata_response_limit)
            .await
            .map_err(|error| context.redact_error(error))
    }

    async fn csrf_token(
        &self,
        context: &PixivRequestContext,
        force_refresh: bool,
    ) -> Result<Arc<SecretString>, PixivError> {
        if !force_refresh
            && let Some(token) = self
                .csrf_tokens
                .lock()
                .map_err(|_| temporary_cache_error())?
                .get(&context.user_id())
                .cloned()
        {
            return Ok(token);
        }

        let token = Arc::new(self.fetch_csrf_token(context).await?);
        self.csrf_tokens
            .lock()
            .map_err(|_| temporary_cache_error())?
            .insert(context.user_id(), Arc::clone(&token));
        Ok(token)
    }

    async fn fetch_csrf_token(
        &self,
        context: &PixivRequestContext,
    ) -> Result<SecretString, PixivError> {
        let _request_permit = self.metadata_request_permit().await;
        let endpoint = PixivEndpoint::Csrf;
        let headers = html_headers(context, endpoint)?;
        let response = self
            .http
            .get(csrf_url(&self.options.web_base_url)?)
            .headers(headers)
            .send()
            .await
            .map_err(|_| PixivError::network(endpoint))?;
        let status = response.status();
        if !status.is_success() {
            let retry_after = response
                .headers()
                .get(RETRY_AFTER)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned);
            read_limited(response, endpoint, self.options.csrf_response_limit).await?;
            return Err(classify_http_status(
                endpoint,
                status,
                None,
                retry_after.as_deref(),
            ));
        }
        let bytes = read_limited(response, endpoint, self.options.csrf_response_limit).await?;
        let html = std::str::from_utf8(&bytes)
            .map_err(|_| PixivError::new(PixivErrorClass::CsrfFailed, Some(endpoint)))?;
        extract_csrf_token(html)
            .ok_or_else(|| PixivError::new(PixivErrorClass::CsrfFailed, Some(endpoint)))
    }

    async fn send_add_bookmark(
        &self,
        context: &PixivRequestContext,
        request: &PixivBookmarkAddRequest,
        token: &SecretString,
    ) -> Result<(PixivBookmarkWriteResult, Value), PixivError> {
        let _request_permit = self.metadata_request_permit().await;
        let endpoint = PixivEndpoint::AddBookmark;
        require_positive_id(request.work_id, endpoint)?;
        let mut headers = json_headers(context, endpoint)?;
        headers.insert(
            CONTENT_TYPE,
            HeaderValue::from_static("application/json; charset=utf-8"),
        );
        headers.insert("x-csrf-token", csrf_header_value(token, endpoint)?);
        let payload = json!({
            "comment": "",
            "illust_id": request.work_id,
            "restrict": match request.visibility {
                PixivBookmarkVisibility::Public => 0,
                PixivBookmarkVisibility::Private => 1,
            },
            "tags": request.tags,
        });
        let response = self
            .http
            .post(add_bookmark_url(&self.options.web_base_url)?)
            .headers(headers)
            .body(serde_json::to_vec(&payload).map_err(|_| PixivError::invalid_json(endpoint))?)
            .send()
            .await
            .map_err(|_| PixivError::network(endpoint))?;
        let raw = read_json_response(response, endpoint, self.options.metadata_response_limit)
            .await
            .map_err(|error| redact_write_error(context, token, error))?;
        let value = map_bookmark_write_result(&raw, endpoint)
            .map_err(|error| redact_write_error(context, token, error))?;
        Ok((value, raw))
    }

    async fn send_delete_bookmark(
        &self,
        context: &PixivRequestContext,
        bookmark_id: i64,
        token: &SecretString,
    ) -> Result<(PixivBookmarkWriteResult, Value), PixivError> {
        let _request_permit = self.metadata_request_permit().await;
        let endpoint = PixivEndpoint::DeleteBookmark;
        require_positive_id(bookmark_id, endpoint)?;
        let mut headers = json_headers(context, endpoint)?;
        headers.insert(
            CONTENT_TYPE,
            HeaderValue::from_static("application/x-www-form-urlencoded; charset=utf-8"),
        );
        headers.insert("x-csrf-token", csrf_header_value(token, endpoint)?);
        let response = self
            .http
            .post(delete_bookmark_url(&self.options.web_base_url)?)
            .headers(headers)
            .body(format!("bookmark_id={bookmark_id}"))
            .send()
            .await
            .map_err(|_| PixivError::network(endpoint))?;
        let raw = read_json_response(response, endpoint, self.options.metadata_response_limit)
            .await
            .map_err(|error| redact_write_error(context, token, error))?;
        let value = map_bookmark_write_result(&raw, endpoint)
            .map_err(|error| redact_write_error(context, token, error))?;
        Ok((value, raw))
    }

    async fn send_artist_follow(
        &self,
        context: &PixivRequestContext,
        operation: ArtistFollowWrite,
        token: &SecretString,
    ) -> Result<(PixivArtistFollowWriteResult, Value), PixivError> {
        let _request_permit = self.metadata_request_permit().await;
        let endpoint = operation.endpoint();
        let artist_id = operation.artist_id();
        require_positive_id(artist_id, endpoint)?;
        let mut headers = json_headers(context, endpoint)?;
        headers.insert(
            CONTENT_TYPE,
            HeaderValue::from_static("application/x-www-form-urlencoded; charset=utf-8"),
        );
        headers.insert("x-csrf-token", csrf_header_value(token, endpoint)?);
        let url = match operation {
            ArtistFollowWrite::Add { .. } => add_artist_follow_url(&self.options.web_base_url)?,
            ArtistFollowWrite::Remove { .. } => {
                remove_artist_follow_url(&self.options.web_base_url)?
            }
        };
        let body = operation.form_body();
        let response = self
            .http
            .post(url)
            .headers(headers)
            .body(body)
            .send()
            .await
            .map_err(|_| PixivError::network(endpoint))?;
        let raw = read_json_response(response, endpoint, self.options.metadata_response_limit)
            .await
            .map_err(|error| redact_write_error(context, token, error))?;
        let value = match operation {
            ArtistFollowWrite::Add { .. } => map_add_artist_follow_result(artist_id, &raw),
            ArtistFollowWrite::Remove { .. } => map_remove_artist_follow_result(artist_id, &raw),
        }
        .map_err(|error| redact_write_error(context, token, error))?;
        Ok((value, raw))
    }

    async fn write_artist_follow(
        &self,
        context: &PixivRequestContext,
        operation: ArtistFollowWrite,
    ) -> Result<AdapterResponse<PixivArtistFollowWriteResult>, PixivError> {
        let endpoint = operation.endpoint();
        validate_context(context, endpoint)?;
        require_positive_id(operation.artist_id(), endpoint)?;
        let first_token = self.csrf_token(context, false).await?;
        match self
            .send_artist_follow(context, operation, first_token.as_ref())
            .await
        {
            Ok((value, raw)) => Ok(AdapterResponse::single(value, endpoint, raw)),
            Err(error) if error.class() == PixivErrorClass::CsrfFailed => {
                self.invalidate_csrf(context.user_id())?;
                let refreshed = self.csrf_token(context, true).await?;
                let (value, raw) = self
                    .send_artist_follow(context, operation, refreshed.as_ref())
                    .await?;
                Ok(AdapterResponse::single(value, endpoint, raw))
            }
            Err(error) => Err(error),
        }
    }

    fn invalidate_csrf(&self, user_id: i64) -> Result<(), PixivError> {
        self.csrf_tokens
            .lock()
            .map_err(|_| temporary_cache_error())?
            .remove(&user_id);
        Ok(())
    }
}

#[async_trait]
impl PixivGateway for PixivWebClient {
    async fn validate_account(
        &self,
        context: &PixivRequestContext,
    ) -> Result<AdapterResponse<PixivAccountValidation>, PixivError> {
        validate_context(context, PixivEndpoint::Profile)?;
        let identity_endpoint = PixivEndpoint::Profile;
        let identity_raw = self
            .get_json(
                context,
                identity_endpoint,
                profile_url(&self.options.web_base_url, context.user_id())?,
            )
            .await?;
        let profile = map_account_profile(context.user_id(), &identity_raw)
            .map_err(|error| context.redact_error(error))?;

        let profile_endpoint = PixivEndpoint::ProfileAll;
        let profile_raw = self
            .get_json(
                context,
                profile_endpoint,
                profile_all_url(&self.options.web_base_url, context.user_id())?,
            )
            .await?;
        map_artist_work_ids(context.user_id(), &profile_raw)
            .map_err(|error| context.redact_error(error))?;

        let bookmarks_endpoint = PixivEndpoint::PrivateBookmarks;
        let request = PixivBookmarksRequest {
            user_id: context.user_id(),
            visibility: PixivBookmarkVisibility::Private,
            mode: PixivBookmarksMode::All,
            tag: None,
            offset: 0,
        };
        let bookmarks_raw = self
            .get_json(
                context,
                bookmarks_endpoint,
                bookmarks_url(&self.options.web_base_url, &request, bookmarks_endpoint)?,
            )
            .await?;
        map_bookmarks(&request, &bookmarks_raw).map_err(|error| context.redact_error(error))?;
        if !has_private_bookmark_evidence(&bookmarks_raw) {
            return Err(PixivError::credential_invalid(
                bookmarks_endpoint,
                "private bookmark ownership was not verified",
            ));
        }

        Ok(AdapterResponse {
            value: PixivAccountValidation {
                user_id: context.user_id(),
                display_name: profile.display_name,
                avatar_url: profile.avatar_url,
                private_bookmarks_verified: true,
            },
            provenance: vec![
                ResponseProvenance {
                    adapter_version: ADAPTER_VERSION,
                    endpoint: identity_endpoint,
                    raw: identity_raw,
                },
                ResponseProvenance {
                    adapter_version: ADAPTER_VERSION,
                    endpoint: profile_endpoint,
                    raw: profile_raw,
                },
                ResponseProvenance {
                    adapter_version: ADAPTER_VERSION,
                    endpoint: bookmarks_endpoint,
                    raw: bookmarks_raw,
                },
            ],
        })
    }

    async fn ranking_page(
        &self,
        context: &PixivRequestContext,
        request: PixivRankingRequest,
    ) -> Result<AdapterResponse<PixivRankingPage>, PixivError> {
        validate_context(context, PixivEndpoint::Ranking)?;
        let raw = self
            .get_json(
                context,
                PixivEndpoint::Ranking,
                ranking_url(&self.options.web_base_url, &request)?,
            )
            .await?;
        let value =
            map_ranking_page(&request, &raw).map_err(|error| context.redact_error(error))?;
        Ok(AdapterResponse::single(value, PixivEndpoint::Ranking, raw))
    }

    async fn work_detail(
        &self,
        context: &PixivRequestContext,
        work_id: i64,
    ) -> Result<AdapterResponse<PixivWorkDetail>, PixivError> {
        let endpoint = PixivEndpoint::WorkDetail;
        validate_context(context, endpoint)?;
        require_positive_id(work_id, endpoint)?;
        let raw = self
            .get_json(
                context,
                endpoint,
                work_detail_url(&self.options.web_base_url, work_id)?,
            )
            .await?;
        let value = map_work_detail(&raw).map_err(|error| context.redact_error(error))?;
        Ok(AdapterResponse::single(value, endpoint, raw))
    }

    async fn work_pages(
        &self,
        context: &PixivRequestContext,
        work_id: i64,
    ) -> Result<AdapterResponse<PixivWorkPages>, PixivError> {
        let endpoint = PixivEndpoint::WorkPages;
        validate_context(context, endpoint)?;
        require_positive_id(work_id, endpoint)?;
        let raw = self
            .get_json(
                context,
                endpoint,
                work_pages_url(&self.options.web_base_url, work_id)?,
            )
            .await?;
        let value = map_work_pages(work_id, &raw).map_err(|error| context.redact_error(error))?;
        Ok(AdapterResponse::single(value, endpoint, raw))
    }

    async fn ugoira_meta(
        &self,
        context: &PixivRequestContext,
        work_id: i64,
    ) -> Result<AdapterResponse<PixivUgoiraMeta>, PixivError> {
        let endpoint = PixivEndpoint::UgoiraMeta;
        validate_context(context, endpoint)?;
        require_positive_id(work_id, endpoint)?;
        let raw = self
            .get_json(
                context,
                endpoint,
                ugoira_meta_url(&self.options.web_base_url, work_id)?,
            )
            .await?;
        let value = map_ugoira_meta(work_id, &raw).map_err(|error| context.redact_error(error))?;
        Ok(AdapterResponse::single(value, endpoint, raw))
    }

    async fn follow_latest(
        &self,
        context: &PixivRequestContext,
        request: PixivFollowLatestRequest,
    ) -> Result<AdapterResponse<PixivPage<PixivDiscoveryWork, PixivFollowLatestCursor>>, PixivError>
    {
        let (endpoint, url) = follow_latest_url(&self.options.web_base_url, &request)?;
        validate_context(context, endpoint)?;
        let raw = self.get_json(context, endpoint, url).await?;
        let value =
            map_follow_latest(&request, &raw).map_err(|error| context.redact_error(error))?;
        Ok(AdapterResponse::single(value, endpoint, raw))
    }

    async fn bookmarks(
        &self,
        context: &PixivRequestContext,
        request: PixivBookmarksRequest,
    ) -> Result<AdapterResponse<PixivPage<i64, PixivBookmarksCursor>>, PixivError> {
        let endpoint = PixivEndpoint::Bookmarks;
        validate_context(context, endpoint)?;
        let raw = self
            .get_json(
                context,
                endpoint,
                bookmarks_url(&self.options.web_base_url, &request, endpoint)?,
            )
            .await?;
        let value = map_bookmarks(&request, &raw).map_err(|error| context.redact_error(error))?;
        Ok(AdapterResponse::single(value, endpoint, raw))
    }

    async fn following_page(
        &self,
        context: &PixivRequestContext,
        request: PixivFollowingRequest,
    ) -> Result<AdapterResponse<PixivPage<PixivFollowedArtist, PixivFollowingCursor>>, PixivError>
    {
        let endpoint = PixivEndpoint::Following;
        validate_context(context, endpoint)?;
        let raw = self
            .get_json(
                context,
                endpoint,
                following_url(&self.options.web_base_url, &request)?,
            )
            .await?;
        let value =
            map_followed_artists(&request, &raw).map_err(|error| context.redact_error(error))?;
        Ok(AdapterResponse::single(value, endpoint, raw))
    }

    async fn artist_work_ids(
        &self,
        context: &PixivRequestContext,
        artist_id: i64,
    ) -> Result<AdapterResponse<PixivArtistWorkIds>, PixivError> {
        let endpoint = PixivEndpoint::ProfileAll;
        validate_context(context, endpoint)?;
        require_positive_id(artist_id, endpoint)?;
        let raw = self
            .get_json(
                context,
                endpoint,
                profile_all_url(&self.options.web_base_url, artist_id)?,
            )
            .await?;
        let value =
            map_artist_work_ids(artist_id, &raw).map_err(|error| context.redact_error(error))?;
        Ok(AdapterResponse::single(value, endpoint, raw))
    }

    async fn artist_follow_state(
        &self,
        context: &PixivRequestContext,
        artist_id: i64,
    ) -> Result<AdapterResponse<PixivArtistFollowState>, PixivError> {
        let endpoint = PixivEndpoint::ArtistFollowState;
        validate_context(context, endpoint)?;
        require_positive_id(artist_id, endpoint)?;
        let raw = self
            .get_json(
                context,
                endpoint,
                artist_follow_state_url(&self.options.web_base_url, artist_id)?,
            )
            .await?;
        let value = map_artist_follow_state(artist_id, &raw)
            .map_err(|error| context.redact_error(error))?;
        Ok(AdapterResponse::single(value, endpoint, raw))
    }

    async fn add_artist_follow(
        &self,
        context: &PixivRequestContext,
        request: PixivArtistFollowRequest,
    ) -> Result<AdapterResponse<PixivArtistFollowWriteResult>, PixivError> {
        self.write_artist_follow(
            context,
            ArtistFollowWrite::Add {
                artist_id: request.artist_id,
                visibility: request.visibility,
            },
        )
        .await
    }

    async fn remove_artist_follow(
        &self,
        context: &PixivRequestContext,
        artist_id: i64,
    ) -> Result<AdapterResponse<PixivArtistFollowWriteResult>, PixivError> {
        self.write_artist_follow(context, ArtistFollowWrite::Remove { artist_id })
            .await
    }

    async fn add_bookmark(
        &self,
        context: &PixivRequestContext,
        request: PixivBookmarkAddRequest,
    ) -> Result<AdapterResponse<PixivBookmarkWriteResult>, PixivError> {
        validate_context(context, PixivEndpoint::AddBookmark)?;
        let first_token = self.csrf_token(context, false).await?;
        match self
            .send_add_bookmark(context, &request, first_token.as_ref())
            .await
        {
            Ok((value, raw)) => Ok(AdapterResponse::single(
                value,
                PixivEndpoint::AddBookmark,
                raw,
            )),
            Err(error) if error.class() == PixivErrorClass::CsrfFailed => {
                self.invalidate_csrf(context.user_id())?;
                let refreshed = self.csrf_token(context, true).await?;
                let (value, raw) = self
                    .send_add_bookmark(context, &request, refreshed.as_ref())
                    .await?;
                Ok(AdapterResponse::single(
                    value,
                    PixivEndpoint::AddBookmark,
                    raw,
                ))
            }
            Err(error) => Err(error),
        }
    }

    async fn delete_bookmark(
        &self,
        context: &PixivRequestContext,
        bookmark_id: i64,
    ) -> Result<AdapterResponse<PixivBookmarkWriteResult>, PixivError> {
        validate_context(context, PixivEndpoint::DeleteBookmark)?;
        require_positive_id(bookmark_id, PixivEndpoint::DeleteBookmark)?;
        let first_token = self.csrf_token(context, false).await?;
        match self
            .send_delete_bookmark(context, bookmark_id, first_token.as_ref())
            .await
        {
            Ok((value, raw)) => Ok(AdapterResponse::single(
                value,
                PixivEndpoint::DeleteBookmark,
                raw,
            )),
            Err(error) if error.class() == PixivErrorClass::CsrfFailed => {
                self.invalidate_csrf(context.user_id())?;
                let refreshed = self.csrf_token(context, true).await?;
                let (value, raw) = self
                    .send_delete_bookmark(context, bookmark_id, refreshed.as_ref())
                    .await?;
                Ok(AdapterResponse::single(
                    value,
                    PixivEndpoint::DeleteBookmark,
                    raw,
                ))
            }
            Err(error) => Err(error),
        }
    }
}
