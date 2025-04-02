use super::convert_published_time;
use crate::community_use_pending;
use activitypub_federation::config::Data;
use actix_web::web::Json;
use lemmy_api_common::{
  build_response::{build_post_response, send_local_notifs},
  context::LemmyContext,
  plugins::{plugin_hook_after, plugin_hook_before},
  post::{CreatePost, PostResponse},
  request::generate_post_link_metadata,
  send_activity::SendActivityData,
  utils::{
    check_community_user_action,
    check_nsfw_allowed,
    get_url_blocklist,
    honeypot_check,
    process_markdown_opt,
    send_webmention,
    slur_regex,
  },
};
use lemmy_db_schema::{
  impls::actor_language::validate_post_language,
  newtypes::PostOrCommentId,
  source::{
    community::Community,
    post::{Post, PostActions, PostInsertForm, PostLikeForm, PostReadForm},
  },
  traits::{Crud, Likeable, Readable},
  utils::diesel_url_create,
};
use lemmy_db_views::structs::{CommunityModeratorView, LocalUserView, SiteView};
use lemmy_utils::{
  error::{LemmyErrorExt, LemmyErrorType, LemmyResult},
  utils::{
    mention::scrape_text_for_mentions,
    slurs::check_slurs,
    validation::{
      is_url_blocked,
      is_valid_alt_text_field,
      is_valid_body_field,
      is_valid_post_title,
      is_valid_url,
    },
  },
};

pub async fn create_post(
  data: Json<CreatePost>,
  context: Data<LemmyContext>,
  local_user_view: LocalUserView,
) -> LemmyResult<Json<PostResponse>> {
  honeypot_check(&data.honeypot)?;
  let local_site = SiteView::read_local(&mut context.pool()).await?.local_site;

  let slur_regex = slur_regex(&context).await?;
  check_slurs(&data.name, &slur_regex)?;
  let url_blocklist = get_url_blocklist(&context).await?;

  let body = process_markdown_opt(&data.body, &slur_regex, &url_blocklist, &context).await?;
  let url = diesel_url_create(data.url.as_deref())?;
  let custom_thumbnail = diesel_url_create(data.custom_thumbnail.as_deref())?;
  check_nsfw_allowed(data.nsfw, Some(&local_site))?;

  is_valid_post_title(&data.name)?;

  if let Some(url) = &url {
    is_url_blocked(url, &url_blocklist)?;
    is_valid_url(url)?;
  }

  if let Some(custom_thumbnail) = &custom_thumbnail {
    is_valid_url(custom_thumbnail)?;
  }

  if let Some(alt_text) = &data.alt_text {
    is_valid_alt_text_field(alt_text)?;
  }

  if let Some(body) = &body {
    is_valid_body_field(body, true)?;
  }

  let community = Community::read(&mut context.pool(), data.community_id).await?;
  check_community_user_action(&local_user_view, &community, &mut context.pool()).await?;

  // If its an NSFW community, then use that as a default
  let nsfw = Some(true);

  if community.posting_restricted_to_mods {
    let community_id = data.community_id;
    CommunityModeratorView::check_is_community_moderator(
      &mut context.pool(),
      community_id,
      local_user_view.local_user.person_id,
    )
    .await?;
  }

  let language_id = validate_post_language(
    &mut context.pool(),
    data.language_id,
    data.community_id,
    local_user_view.local_user.id,
  )
  .await?;

  let scheduled_publish_time =
    convert_published_time(data.scheduled_publish_time, &local_user_view, &context).await?;
  let mut post_form = PostInsertForm {
    url,
    body,
    alt_text: data.alt_text.clone(),
    nsfw,
    language_id: Some(language_id),
    federation_pending: Some(community_use_pending(&community, &context).await),
    scheduled_publish_time,
    ..PostInsertForm::new(
      data.name.trim().to_string(),
      local_user_view.person.id,
      data.community_id,
    )
  };

  post_form = plugin_hook_before("before_create_local_post", post_form).await?;

  let inserted_post = Post::create(&mut context.pool(), &post_form)
    .await
    .with_lemmy_type(LemmyErrorType::CouldntCreatePost)?;
  plugin_hook_after("after_create_local_post", &inserted_post)?;

  let community_id = community.id;
  let federate_post = if scheduled_publish_time.is_none() {
    send_webmention(inserted_post.clone(), community);
    |post| Some(SendActivityData::CreatePost(post))
  } else {
    |_| None
  };
  generate_post_link_metadata(
    inserted_post.clone(),
    custom_thumbnail.map(Into::into),
    federate_post,
    context.reset_request_count(),
  )
  .await?;

  // They like their own post by default
  let person_id = local_user_view.person.id;
  let post_id = inserted_post.id;
  let like_form = PostLikeForm::new(post_id, person_id, 1);

  PostActions::like(&mut context.pool(), &like_form).await?;

  // Scan the post body for user mentions, add those rows
  let mentions = scrape_text_for_mentions(&inserted_post.body.clone().unwrap_or_default());
  send_local_notifs(
    mentions,
    PostOrCommentId::Post(inserted_post.id),
    &local_user_view.person,
    true,
    &context,
    Some(&local_user_view),
  )
  .await?;

  let read_form = PostReadForm::new(post_id, person_id);
  PostActions::mark_as_read(&mut context.pool(), &read_form).await?;

  build_post_response(&context, community_id, local_user_view, post_id).await
}
