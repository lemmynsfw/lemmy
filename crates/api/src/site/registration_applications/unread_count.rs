use activitypub_federation::config::Data;
use actix_web::web::Json;
use lemmy_api_common::{
  context::LemmyContext,
  site::GetUnreadRegistrationApplicationCountResponse,
  utils::is_admin,
};
use lemmy_db_views::structs::{LocalUserView, RegistrationApplicationView, SiteView};
use lemmy_utils::error::LemmyResult;

pub async fn get_unread_registration_application_count(
  context: Data<LemmyContext>,
  local_user_view: LocalUserView,
) -> LemmyResult<Json<GetUnreadRegistrationApplicationCountResponse>> {
  let local_site = SiteView::read_local(&mut context.pool()).await?.local_site;

  // Only let admins do this
  is_admin(&local_user_view)?;

  let verified_email_only = local_site.require_email_verification;

  let registration_applications =
    RegistrationApplicationView::get_unread_count(&mut context.pool(), verified_email_only).await?;

  Ok(Json(GetUnreadRegistrationApplicationCountResponse {
    registration_applications,
  }))
}
