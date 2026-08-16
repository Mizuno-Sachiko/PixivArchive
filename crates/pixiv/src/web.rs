use crate::{error::PixivError, mapper::validate_following_request};
use pixivarchive_domain::pixiv::{
    PixivBookmarkVisibility, PixivBookmarksMode, PixivBookmarksRequest, PixivFollowLatestMode,
    PixivFollowLatestRequest, PixivFollowLatestSource, PixivFollowingRequest,
    PixivFollowingVisibility, PixivRankingContent, PixivRankingMode, PixivRankingRequest,
};
use url::Url;

pub const ADAPTER_VERSION: &str = "pixiv-web-2026-08-10";
pub const PIXIV_REFERER: &str = "https://www.pixiv.net/";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PixivEndpoint {
    Ranking,
    WorkDetail,
    WorkPages,
    UgoiraMeta,
    FollowLatest,
    MypixivLatest,
    Bookmarks,
    PrivateBookmarks,
    Following,
    Profile,
    ProfileAll,
    ArtistFollowState,
    AddArtistFollow,
    RemoveArtistFollow,
    AddBookmark,
    DeleteBookmark,
    Csrf,
    Media,
}

impl PixivEndpoint {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ranking => "ranking",
            Self::WorkDetail => "work_detail",
            Self::WorkPages => "work_pages",
            Self::UgoiraMeta => "ugoira_meta",
            Self::FollowLatest => "follow_latest",
            Self::MypixivLatest => "mypixiv_latest",
            Self::Bookmarks => "bookmarks",
            Self::PrivateBookmarks => "private_bookmarks",
            Self::Following => "following",
            Self::Profile => "profile",
            Self::ProfileAll => "profile_all",
            Self::ArtistFollowState => "artist_follow_state",
            Self::AddArtistFollow => "add_artist_follow",
            Self::RemoveArtistFollow => "remove_artist_follow",
            Self::AddBookmark => "add_bookmark",
            Self::DeleteBookmark => "delete_bookmark",
            Self::Csrf => "csrf",
            Self::Media => "media",
        }
    }
}

pub fn ranking_url(base: &Url, request: &PixivRankingRequest) -> Result<Url, PixivError> {
    require_page(request.page, PixivEndpoint::Ranking)?;
    if !request.mode.supports_content(request.content) {
        return Err(PixivError::hidden_or_invalid(
            PixivEndpoint::Ranking,
            "Pixiv popularity rankings only support aggregate content",
        ));
    }
    let mut url = join(base, "/ranking.php", PixivEndpoint::Ranking)?;
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("mode", ranking_mode_value(request.mode));
        query.append_pair("content", ranking_content_value(request.content));
        if let Some(date) = request.date {
            query.append_pair(
                "date",
                &format!(
                    "{:04}{:02}{:02}",
                    date.year(),
                    u8::from(date.month()),
                    date.day()
                ),
            );
        }
        query.append_pair("p", &request.page.to_string());
        query.append_pair("format", "json");
    }
    Ok(url)
}

pub(crate) fn work_detail_url(base: &Url, work_id: i64) -> Result<Url, PixivError> {
    join(
        base,
        &format!("/ajax/illust/{work_id}"),
        PixivEndpoint::WorkDetail,
    )
}

pub(crate) fn work_pages_url(base: &Url, work_id: i64) -> Result<Url, PixivError> {
    join(
        base,
        &format!("/ajax/illust/{work_id}/pages"),
        PixivEndpoint::WorkPages,
    )
}

pub(crate) fn ugoira_meta_url(base: &Url, work_id: i64) -> Result<Url, PixivError> {
    join(
        base,
        &format!("/ajax/illust/{work_id}/ugoira_meta"),
        PixivEndpoint::UgoiraMeta,
    )
}

pub(crate) fn follow_latest_url(
    base: &Url,
    request: &PixivFollowLatestRequest,
) -> Result<(PixivEndpoint, Url), PixivError> {
    let (endpoint, path) = match request.source {
        PixivFollowLatestSource::Following => {
            (PixivEndpoint::FollowLatest, "/ajax/follow_latest/illust")
        }
        PixivFollowLatestSource::Mypixiv => {
            (PixivEndpoint::MypixivLatest, "/ajax/mypixiv_latest/illust")
        }
    };
    require_page(request.page, endpoint)?;
    let mut url = join(base, path, endpoint)?;
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("p", &request.page.to_string());
        if request.source == PixivFollowLatestSource::Following {
            query.append_pair("tag", request.tag.as_deref().unwrap_or(""));
            query.append_pair(
                "mode",
                match request.mode {
                    PixivFollowLatestMode::All => "all",
                    PixivFollowLatestMode::R18 => "r18",
                },
            );
        }
        query.append_pair("lang", &request.language);
    }
    Ok((endpoint, url))
}

pub(crate) fn bookmarks_url(
    base: &Url,
    request: &PixivBookmarksRequest,
    endpoint: PixivEndpoint,
) -> Result<Url, PixivError> {
    require_positive_id(request.user_id, endpoint, "Pixiv user ID")?;
    let mut url = join(
        base,
        &format!("/ajax/user/{}/illusts/bookmarks", request.user_id),
        endpoint,
    )?;
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("tag", request.tag.as_deref().unwrap_or(""));
        query.append_pair("offset", &request.offset.to_string());
        query.append_pair("limit", "48");
        query.append_pair(
            "rest",
            match request.visibility {
                PixivBookmarkVisibility::Public => "show",
                PixivBookmarkVisibility::Private => "hide",
            },
        );
        query.append_pair("order", "desc");
        query.append_pair(
            "mode",
            match request.mode {
                PixivBookmarksMode::All => "all",
                PixivBookmarksMode::Safe => "safe",
                PixivBookmarksMode::R18 => "r18",
            },
        );
        query.append_pair("work_tag", "");
        query.append_pair("lang", "zh");
        query.append_pair("bm", "");
        query.append_pair("rdm", "random");
    }
    Ok(url)
}

pub(crate) fn following_url(
    base: &Url,
    request: &PixivFollowingRequest,
) -> Result<Url, PixivError> {
    validate_following_request(request, PixivEndpoint::Following)?;
    let mut url = join(
        base,
        &format!("/ajax/user/{}/following", request.user_id),
        PixivEndpoint::Following,
    )?;
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("offset", &request.offset.to_string());
        query.append_pair("limit", &request.limit.to_string());
        query.append_pair(
            "rest",
            match request.visibility {
                PixivFollowingVisibility::Public => "show",
                PixivFollowingVisibility::Private => "hide",
            },
        );
        query.append_pair("lang", &request.language);
    }
    Ok(url)
}

pub(crate) fn profile_all_url(base: &Url, user_id: i64) -> Result<Url, PixivError> {
    join(
        base,
        &format!("/ajax/user/{user_id}/profile/all"),
        PixivEndpoint::ProfileAll,
    )
}

pub(crate) fn profile_url(base: &Url, user_id: i64) -> Result<Url, PixivError> {
    let mut url = join(
        base,
        &format!("/ajax/user/{user_id}"),
        PixivEndpoint::Profile,
    )?;
    url.query_pairs_mut().append_pair("full", "1");
    Ok(url)
}

pub(crate) fn artist_follow_state_url(base: &Url, artist_id: i64) -> Result<Url, PixivError> {
    let mut url = join(
        base,
        &format!("/ajax/user/{artist_id}"),
        PixivEndpoint::ArtistFollowState,
    )?;
    url.query_pairs_mut().append_pair("full", "1");
    Ok(url)
}

pub(crate) fn add_artist_follow_url(base: &Url) -> Result<Url, PixivError> {
    join(base, "/bookmark_add.php", PixivEndpoint::AddArtistFollow)
}

pub(crate) fn remove_artist_follow_url(base: &Url) -> Result<Url, PixivError> {
    join(
        base,
        "/rpc_group_setting.php",
        PixivEndpoint::RemoveArtistFollow,
    )
}

pub(crate) fn add_bookmark_url(base: &Url) -> Result<Url, PixivError> {
    join(
        base,
        "/ajax/illusts/bookmarks/add",
        PixivEndpoint::AddBookmark,
    )
}

pub(crate) fn delete_bookmark_url(base: &Url) -> Result<Url, PixivError> {
    join(
        base,
        "/ajax/illusts/bookmarks/delete",
        PixivEndpoint::DeleteBookmark,
    )
}

pub(crate) fn csrf_url(base: &Url) -> Result<Url, PixivError> {
    join(base, "/", PixivEndpoint::Csrf)
}

pub(crate) fn artwork_referer(work_id: i64) -> Result<Url, PixivError> {
    Url::parse(&format!("https://www.pixiv.net/artworks/{work_id}"))
        .map_err(|_| PixivError::invalid_json(PixivEndpoint::Media))
}

fn ranking_mode_value(mode: PixivRankingMode) -> &'static str {
    match mode {
        PixivRankingMode::Daily => "daily",
        PixivRankingMode::Weekly => "weekly",
        PixivRankingMode::Monthly => "monthly",
        PixivRankingMode::Rookie => "rookie",
        PixivRankingMode::Original => "original",
        PixivRankingMode::AiGenerated => "daily_ai",
        PixivRankingMode::R18 => "daily_r18",
        PixivRankingMode::R18g => "r18g",
        PixivRankingMode::Male => "male",
        PixivRankingMode::Female => "female",
    }
}

fn ranking_content_value(content: PixivRankingContent) -> &'static str {
    match content {
        PixivRankingContent::All => "all",
        PixivRankingContent::Illustration => "illust",
        PixivRankingContent::Manga => "manga",
        PixivRankingContent::Ugoira => "ugoira",
    }
}

fn join(base: &Url, path: &str, endpoint: PixivEndpoint) -> Result<Url, PixivError> {
    base.join(path)
        .map_err(|_| PixivError::invalid_json(endpoint))
}

fn require_page(page: u32, endpoint: PixivEndpoint) -> Result<(), PixivError> {
    if page == 0 {
        return Err(PixivError::hidden_or_invalid(
            endpoint,
            "Pixiv pages start at one",
        ));
    }
    Ok(())
}

fn require_positive_id(value: i64, endpoint: PixivEndpoint, name: &str) -> Result<(), PixivError> {
    if value <= 0 {
        return Err(PixivError::hidden_or_invalid(
            endpoint,
            format!("{name} must be positive"),
        ));
    }
    Ok(())
}
