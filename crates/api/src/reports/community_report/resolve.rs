use actix_web::web::{Data, Json};
use lemmy_api_common::{
  context::LemmyContext,
  reports::community::{CommunityReportResponse, ResolveCommunityReport},
  utils::is_admin,
};
use lemmy_db_schema::{source::community_report::CommunityReport, traits::Reportable};
use lemmy_db_views_local_user::LocalUserView;
use lemmy_db_views_reports::CommunityReportView;
use lemmy_utils::error::LemmyResult;

pub async fn resolve_community_report(
  data: Json<ResolveCommunityReport>,
  context: Data<LemmyContext>,
  local_user_view: LocalUserView,
) -> LemmyResult<Json<CommunityReportResponse>> {
  is_admin(&local_user_view)?;

  let report_id = data.report_id;
  let person_id = local_user_view.person.id;
  if data.resolved {
    CommunityReport::resolve(&mut context.pool(), report_id, person_id).await?;
  } else {
    CommunityReport::unresolve(&mut context.pool(), report_id, person_id).await?;
  }

  let community_report_view =
    CommunityReportView::read(&mut context.pool(), report_id, person_id).await?;

  Ok(Json(CommunityReportResponse {
    community_report_view,
  }))
}
