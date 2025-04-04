use crate::objects::{
  comment::ApubComment,
  community::ApubCommunity,
  instance::ApubSite,
  person::ApubPerson,
  post::ApubPost,
};
use activitypub_federation::{
  config::Data,
  fetch::webfinger::webfinger_resolve_actor,
  traits::{Actor, Object},
};
use diesel::NotFound;
use either::Either;
use itertools::Itertools;
use lemmy_api_common::{context::LemmyContext, LemmyErrorType};
use lemmy_db_schema::{newtypes::InstanceId, traits::ApubActor};
use lemmy_db_views::structs::LocalUserView;
use lemmy_utils::error::{LemmyError, LemmyResult};

pub(crate) mod markdown_links;
pub mod search;

pub(crate) type PostOrComment = Either<ApubPost, ApubComment>;

pub type SiteOrCommunityOrUser = Either<ApubSite, UserOrCommunity>;

pub type UserOrCommunity = Either<ApubPerson, ApubCommunity>;

/// Resolve actor identifier like `!news@example.com` to user or community object.
///
/// In case the requesting user is logged in and the object was not found locally, it is attempted
/// to fetch via webfinger from the original instance.
pub async fn resolve_ap_identifier<ActorType, DbActor>(
  identifier: &str,
  context: &Data<LemmyContext>,
  local_user_view: &Option<LocalUserView>,
  include_deleted: bool,
) -> LemmyResult<ActorType>
where
  ActorType: Object<DataType = LemmyContext, Error = LemmyError>
    + Object
    + Actor
    + From<DbActor>
    + Send
    + 'static,
  for<'de2> <ActorType as Object>::Kind: serde::Deserialize<'de2>,
  DbActor: ApubActor + Send + 'static,
{
  // remote actor
  if identifier.contains('@') {
    let (name, domain) = identifier
      .splitn(2, '@')
      .collect_tuple()
      .ok_or(LemmyErrorType::InvalidUrl)?;
    let actor = DbActor::read_from_name_and_domain(&mut context.pool(), name, domain)
      .await
      .ok()
      .flatten();
    if let Some(actor) = actor {
      Ok(actor.into())
    } else if local_user_view.is_some() {
      // Fetch the actor from its home instance using webfinger
      let actor: ActorType = webfinger_resolve_actor(&identifier.to_lowercase(), context).await?;
      Ok(actor)
    } else {
      Err(NotFound.into())
    }
  }
  // local actor
  else {
    let identifier = identifier.to_string();
    Ok(
      DbActor::read_from_name(&mut context.pool(), &identifier, include_deleted)
        .await?
        .ok_or(NotFound)?
        .into(),
    )
  }
}

pub(crate) fn get_instance_id(s: &SiteOrCommunityOrUser) -> InstanceId {
  match s {
    SiteOrCommunityOrUser::Left(s) => s.instance_id,
    SiteOrCommunityOrUser::Right(UserOrCommunity::Left(u)) => u.instance_id,
    SiteOrCommunityOrUser::Right(UserOrCommunity::Right(c)) => c.instance_id,
  }
}
