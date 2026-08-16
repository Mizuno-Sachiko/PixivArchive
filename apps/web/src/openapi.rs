use utoipa::{
    Modify, OpenApi,
    openapi::{
        ObjectBuilder, Required,
        path::{Operation, ParameterBuilder, ParameterIn},
        schema::Type,
        security::{ApiKey, ApiKeyValue, SecurityScheme},
    },
};

#[derive(OpenApi)]
#[openapi(
    modifiers(&SecurityContract),
    info(
        title = "PixivArchive API",
        description = "Single-user management API for PixivArchive",
        version = "0.1.0"
    ),
    paths(
        crate::api::auth::login,
        crate::api::auth::session,
        crate::api::auth::logout,
        crate::api::bookmarks::add,
        crate::api::bookmarks::remove,
        crate::api::events::sse_events,
        crate::api::favorites::get_state,
        crate::api::favorites::update,
        crate::api::favorites::run,
        crate::api::following::get_state,
        crate::api::following::update_subscription,
        crate::api::following::update_author,
        crate::api::following::update_authors,
        crate::api::following::artist_follow_state,
        crate::api::following::update_artist_follow,
        crate::api::following::avatar::author_avatar,
        crate::api::following::run,
        crate::api::following::refresh,
        crate::api::gallery::search::search,
        crate::api::gallery::search::count,
        crate::api::gallery::search::selection_projection,
        crate::api::gallery::context::context_selection_projection,
        crate::api::gallery::overview::overview_decorations,
        crate::api::gallery::overview::shuffle_overview_decorations,
        crate::api::gallery::context::artists,
        crate::api::gallery::context::artist_detail,
        crate::api::gallery::context::tags,
        crate::api::gallery::context::tag_detail,
        crate::api::gallery::context::series,
        crate::api::gallery::context::series_detail,
        crate::api::gallery::detail::work_id_by_pixiv_id,
        crate::api::gallery::detail::work_detail,
        crate::api::gallery::detail::work_revisions,
        crate::api::gallery::download::download_work,
        crate::api::gallery::media::source_media,
        crate::api::gallery::media::derivative_media,
        crate::api::imports::list,
        crate::api::imports::queue,
        crate::api::pixiv_account::get_account,
        crate::api::pixiv_account::account_avatar,
        crate::api::pixiv_account::update_account,
        crate::api::pixiv_account::clear_credential,
        crate::api::pixiv_account::validate_account,
        crate::api::pixiv_account::update_bookmark_writeback,
        crate::api::rules::list,
        crate::api::rules::create,
        crate::api::rules::copy,
        crate::api::rules::reorder,
        crate::api::rules::get_one,
        crate::api::rules::delete_one,
        crate::api::rules::get_draft,
        crate::api::rules::save_draft,
        crate::api::rules::publish,
        crate::api::rules::validate,
        crate::api::rules::preview,
        crate::api::rules::export,
        crate::api::rules::import,
        crate::api::subscriptions::list,
        crate::api::subscriptions::create,
        crate::api::subscriptions::get_one,
        crate::api::subscriptions::list_runs,
        crate::api::subscriptions::list_cursors,
        crate::api::subscriptions::update,
        crate::api::subscriptions::set_enabled,
        crate::api::subscriptions::delete_one,
        crate::api::subscriptions::run,
        crate::api::subscriptions::stop,
        crate::api::tasks::list,
        crate::api::tasks::get_one,
        crate::api::tasks::retry,
        crate::api::tasks::cancel,
        crate::api::trash::list,
        crate::api::trash::project_selection,
        crate::api::trash::move_to_trash,
        crate::api::trash::move_gallery_to_trash,
        crate::api::trash::move_gallery_contexts_to_trash,
        crate::api::trash::restore,
        crate::api::trash::reschedule,
        crate::api::trash::purge,
        crate::api::trash::restore_many,
        crate::api::trash::reschedule_many,
        crate::api::trash::purge_many,
        crate::api::trash::purge_all,
        crate::api::system::status,
        crate::api::system::settings,
        crate::api::system::update_settings,
        crate::api::system::update_setting,
        crate::api::system::queue_maintenance,
        crate::api::system::live,
        crate::api::system::ready
    ),
    security(("session_cookie" = [])),
    components(schemas(
        pixivarchive_domain::rule::RuleCatalog,
        pixivarchive_domain::rule::RuleDefinitionV1,
        pixivarchive_domain::rule::EvaluationTrace,
        crate::api::ApiErrorBody,
        crate::api::auth::LoginBody,
        crate::api::auth::SessionDto,
        crate::api::bookmarks::AddBookmarkBody,
        crate::api::bookmarks::RemoveBookmarkBody,
        crate::api::bookmarks::BookmarkCommandDto,
        crate::api::favorites::FavoritesStateDto,
        crate::api::favorites::UpdateFavoritesBody,
        crate::api::following::FollowingStateDto,
        crate::api::following::FollowingAuthorDto,
        crate::api::following::UpdateFollowingBody,
        crate::api::following::UpdateFollowingAuthorBody,
        crate::api::following::UpdateFollowingAuthorsBody,
        crate::api::following::ArtistFollowStateDto,
        crate::api::following::UpdateArtistFollowBody,
        crate::api::gallery::GalleryTagDto,
        crate::api::gallery::GalleryWorkDto,
        crate::api::gallery::GallerySearchPageDto,
        crate::api::gallery::GalleryCountDto,
        crate::api::gallery::GallerySelectionProjectionBody,
        crate::api::gallery::GallerySelectionProjectionDto,
        crate::api::gallery::GalleryContextSelectionProjectionBody,
        crate::api::gallery::GalleryContextSelectionProjectionDto,
        crate::api::gallery::OverviewDecorationDto,
        crate::api::gallery::OverviewDecorationsDto,
        crate::api::gallery::OverviewDecorationsQuery,
        pixivarchive_domain::work::FilterMode,
        pixivarchive_domain::work::GallerySearch,
        pixivarchive_domain::work::GallerySelectionExpression,
        pixivarchive_domain::work::GalleryContextKind,
        pixivarchive_domain::work::GalleryContextSelectionExpression,
        pixivarchive_domain::work::GalleryFilterGroup,
        pixivarchive_domain::work::GalleryFilter,
        pixivarchive_domain::work::GalleryTextField,
        pixivarchive_domain::work::GalleryTextOperator,
        pixivarchive_domain::work::GalleryTagOperator,
        pixivarchive_domain::work::GalleryTagScope,
        pixivarchive_domain::work::GalleryCategoryField,
        pixivarchive_domain::work::GalleryNumberField,
        pixivarchive_domain::work::GalleryNumberComparison,
        pixivarchive_domain::work::GalleryDateField,
        pixivarchive_domain::work::GalleryDateComparison,
        pixivarchive_domain::work::GalleryBooleanField,
        pixivarchive_domain::work::GallerySortField,
        pixivarchive_domain::work::SortDirection,
        pixivarchive_domain::work::GalleryCursor,
        pixivarchive_domain::work::GalleryCursorKey,
        crate::api::gallery::GalleryArtistDetailDto,
        crate::api::gallery::GalleryArtistPageDto,
        crate::api::gallery::GalleryTagDetailDto,
        crate::api::gallery::GalleryTagPageDto,
        crate::api::gallery::GallerySeriesDetailDto,
        crate::api::gallery::GallerySeriesPageDto,
        crate::api::gallery::GalleryDerivativeDto,
        crate::api::gallery::GalleryMediaRevisionDto,
        crate::api::gallery::GalleryPageDto,
        crate::api::gallery::UgoiraFrameDto,
        crate::api::gallery::UgoiraManifestDto,
        crate::api::gallery::GalleryWorkDetailDto,
        crate::api::gallery::WorkIdResolutionDto,
        crate::api::gallery::WorkRevisionSummaryDto,
        crate::api::imports::QueueImportBody,
        crate::api::imports::ImportStrategyDto,
        crate::api::imports::ImportKindDto,
        crate::api::imports::ImportListQuery,
        crate::api::imports::ImportRunDto,
        crate::api::imports::ImportRunList,
        crate::api::pixiv_account::PixivAccountDto,
        crate::api::pixiv_account::PixivAccountStateDto,
        crate::api::pixiv_account::UpdatePixivAccountBody,
        crate::api::pixiv_account::ClearPixivCredentialBody,
        crate::api::pixiv_account::UpdateBookmarkWritebackBody,
        crate::api::rules::RuleDto,
        crate::api::rules::RuleList,
        crate::api::rules::CreateRuleBody,
        crate::api::rules::CopyRuleBody,
        crate::api::rules::ReorderRulesBody,
        crate::api::rules::ExpectedRevisionQuery,
        crate::api::rules::RuleDraftDto,
        crate::api::rules::SaveRuleDraftBody,
        crate::api::rules::PublishRuleBody,
        crate::api::rules::RuleVersionDto,
        crate::api::rules::RuleDefinitionBody,
        crate::api::rules::RuleValidationResponse,
        crate::api::rules::PreviewRuleBody,
        crate::api::rules::RulePreviewItemDto,
        crate::api::rules::RulePreviewResponse,
        crate::api::subscriptions::SubscriptionDto,
        crate::api::subscriptions::SubscriptionKindDto,
        crate::api::subscriptions::SubscriptionScheduleDto,
        crate::api::subscriptions::SubscriptionList,
        crate::api::subscriptions::SubscriptionRunListQuery,
        crate::api::subscriptions::SubscriptionRunDto,
        crate::api::subscriptions::SubscriptionRunList,
        crate::api::subscriptions::SubscriptionCursorDto,
        crate::api::subscriptions::SubscriptionCursorList,
        crate::api::subscriptions::CreateSubscriptionBody,
        crate::api::subscriptions::UpdateSubscriptionBody,
        crate::api::subscriptions::SetSubscriptionEnabledBody,
        crate::api::subscriptions::DeleteSubscriptionQuery,
        crate::api::subscriptions::RunSubscriptionBody,
        crate::api::subscriptions::SubscriptionRunAccepted,
        crate::api::tasks::TaskListQuery,
        crate::api::tasks::TaskDto,
        crate::api::tasks::TaskList,
        crate::api::tasks::TaskAttemptDto,
        crate::api::tasks::TaskDetailDto,
        crate::api::tasks::TaskCommandBody,
        crate::api::trash::MoveToTrashBody,
        crate::api::trash::MoveGalleryToTrashBody,
        crate::api::trash::MoveGalleryToTrashDto,
        crate::api::trash::MoveGalleryContextsToTrashBody,
        crate::api::trash::TrashEntryDto,
        crate::api::trash::TrashWorkSummaryDto,
        crate::api::trash::TrashListDto,
        crate::api::trash::TrashCursorDto,
        crate::api::trash::TrashCollectionSummaryDto,
        crate::api::trash::TrashSelectionBody,
        crate::api::trash::TrashSelectionDto,
        pixivarchive_domain::work::TrashFilter,
        pixivarchive_domain::work::TrashSelectionExpression,
        pixivarchive_domain::work::TrashActionBlockReason,
        pixivarchive_domain::work::TrashActionCapabilities,
        crate::api::trash::RescheduleTrashBody,
        crate::api::trash::TrashSelectionCommandBody,
        crate::api::trash::RescheduleTrashManyBody,
        crate::api::trash::PurgeAccepted,
        crate::api::trash::TrashBatchAccepted,
        crate::api::trash::PurgeAllAccepted,
        crate::api::system::ComponentStatusDto,
        crate::api::system::SystemStatusDto,
        crate::api::system::StorageStatusDto,
        crate::api::system::SystemCapabilitiesDto,
        crate::api::system::SettingsDto,
        crate::api::system::EffectiveSettingsDto,
        crate::api::system::SecuritySettingsDto,
        crate::api::system::FailureLimitDto,
        crate::api::system::StorageSettingsDto,
        crate::api::system::RetrySettingsDto,
        crate::api::system::QueueSettingsDto,
        crate::api::system::QueueQuotaWeightsDto,
        crate::api::system::JobPriorityMappingDto,
        crate::api::system::JobKindDto,
        crate::api::system::JobPriorityDto,
        crate::api::system::ProcessingSettingsDto,
        crate::api::system::RateLimitDto,
        crate::api::system::DerivativeSettingsDto,
        crate::api::system::DerivativeFormatDto,
        crate::api::system::UgoiraSettingsDto,
        crate::api::system::PixivSettingsDto,
        crate::api::system::ContentSettingsDto,
        crate::api::system::SettingPayloadDto,
        crate::api::system::BatchSettingUpdateBody,
        crate::api::system::UpdateSettingsBody,
        crate::api::system::UpdateSettingBody,
        crate::api::system::SavedSettingDto,
        crate::api::system::SavedSettingsDto,
        crate::api::system::MaintenanceBody,
        crate::api::system::MaintenanceAcceptedDto
    )),
    tags(
        (name = "Auth"),
        (name = "Bookmarks"),
        (name = "Events"),
        (name = "Favorites"),
        (name = "Following"),
        (name = "Gallery"),
        (name = "Health"),
        (name = "Imports"),
        (name = "Media"),
        (name = "Pixiv Account"),
        (name = "Rules"),
        (name = "Subscriptions"),
        (name = "System"),
        (name = "Tasks"),
        (name = "Trash")
    )
)]
pub struct ApiDoc;

struct SecurityContract;

impl Modify for SecurityContract {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        openapi
            .components
            .get_or_insert_default()
            .add_security_scheme(
                "session_cookie",
                SecurityScheme::ApiKey(ApiKey::Cookie(ApiKeyValue::with_description(
                    "pa_session",
                    "Authenticated PixivArchive administrator session",
                ))),
            );

        for (path, item) in &mut openapi.paths.paths {
            qualify_operation_id(item.get.as_mut());
            qualify_operation_id(item.put.as_mut());
            qualify_operation_id(item.post.as_mut());
            qualify_operation_id(item.delete.as_mut());
            qualify_operation_id(item.options.as_mut());
            qualify_operation_id(item.head.as_mut());
            qualify_operation_id(item.patch.as_mut());
            qualify_operation_id(item.trace.as_mut());

            if path != "/api/auth/login" {
                add_csrf_header(item.put.as_mut());
                add_csrf_header(item.post.as_mut());
                add_csrf_header(item.delete.as_mut());
                add_csrf_header(item.patch.as_mut());
            }
        }
    }
}

fn qualify_operation_id(operation: Option<&mut Operation>) {
    let Some(operation) = operation else {
        return;
    };
    let Some(operation_id) = operation.operation_id.as_deref() else {
        return;
    };
    let Some(tag) = operation.tags.as_ref().and_then(|tags| tags.first()) else {
        return;
    };
    operation.operation_id = Some(format!(
        "{}_{operation_id}",
        tag.to_ascii_lowercase().replace(' ', "_")
    ));
}

fn add_csrf_header(operation: Option<&mut Operation>) {
    let Some(operation) = operation else {
        return;
    };
    let parameter = ParameterBuilder::new()
        .name("X-CSRF-Token")
        .parameter_in(ParameterIn::Header)
        .required(Required::True)
        .description(Some(
            "Token bound to the authenticated administrator session",
        ))
        .schema(Some(ObjectBuilder::new().schema_type(Type::String).build()))
        .build();
    operation.parameters.get_or_insert_default().push(parameter);
}
