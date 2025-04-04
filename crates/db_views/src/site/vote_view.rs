use crate::structs::VoteView;
use diesel::{
  result::Error,
  BoolExpressionMethods,
  ExpressionMethods,
  JoinOnDsl,
  NullableExpressionMethods,
  QueryDsl,
};
use diesel_async::RunQueryDsl;
use lemmy_db_schema::{
  aliases::creator_community_actions,
  newtypes::{CommentId, PostId},
  utils::{get_conn, limit_and_offset, DbPool},
};
use lemmy_db_schema_file::schema::{
  comment,
  comment_actions,
  community_actions,
  person,
  post,
  post_actions,
};

impl VoteView {
  pub async fn list_for_post(
    pool: &mut DbPool<'_>,
    post_id: PostId,
    page: Option<i64>,
    limit: Option<i64>,
  ) -> Result<Vec<Self>, Error> {
    let conn = &mut get_conn(pool).await?;
    let (limit, offset) = limit_and_offset(page, limit)?;

    let creator_community_actions_join = creator_community_actions.on(
      creator_community_actions
        .field(community_actions::community_id)
        .eq(post::community_id)
        .and(
          creator_community_actions
            .field(community_actions::person_id)
            .eq(post_actions::person_id),
        ),
    );

    post_actions::table
      .filter(post_actions::like_score.is_not_null())
      .inner_join(person::table)
      .inner_join(post::table)
      .left_join(creator_community_actions_join)
      .filter(post_actions::post_id.eq(post_id))
      .select((
        person::all_columns,
        creator_community_actions
          .field(community_actions::received_ban)
          .nullable()
          .is_not_null(),
        post_actions::like_score.assume_not_null(),
      ))
      .order_by(post_actions::like_score)
      .limit(limit)
      .offset(offset)
      .load::<Self>(conn)
      .await
  }

  pub async fn list_for_comment(
    pool: &mut DbPool<'_>,
    comment_id: CommentId,
    page: Option<i64>,
    limit: Option<i64>,
  ) -> Result<Vec<Self>, Error> {
    let conn = &mut get_conn(pool).await?;
    let (limit, offset) = limit_and_offset(page, limit)?;

    let creator_community_actions_join = creator_community_actions.on(
      creator_community_actions
        .field(community_actions::community_id)
        .eq(post::community_id)
        .and(
          creator_community_actions
            .field(community_actions::person_id)
            .eq(comment_actions::person_id),
        ),
    );

    comment_actions::table
      .filter(comment_actions::like_score.is_not_null())
      .inner_join(person::table)
      .inner_join(comment::table.inner_join(post::table))
      .left_join(creator_community_actions_join)
      .filter(comment_actions::comment_id.eq(comment_id))
      .select((
        person::all_columns,
        creator_community_actions
          .field(community_actions::received_ban)
          .nullable()
          .is_not_null(),
        comment_actions::like_score.assume_not_null(),
      ))
      .order_by(comment_actions::like_score)
      .limit(limit)
      .offset(offset)
      .load::<Self>(conn)
      .await
  }
}

#[cfg(test)]
mod tests {
  use crate::structs::VoteView;
  use lemmy_db_schema::{
    source::{
      comment::{Comment, CommentActions, CommentInsertForm, CommentLikeForm},
      community::{Community, CommunityActions, CommunityInsertForm, CommunityPersonBanForm},
      instance::Instance,
      person::{Person, PersonInsertForm},
      post::{Post, PostActions, PostInsertForm, PostLikeForm},
    },
    traits::{Bannable, Crud, Likeable},
    utils::build_db_pool_for_tests,
  };
  use lemmy_utils::error::LemmyResult;
  use pretty_assertions::assert_eq;
  use serial_test::serial;

  #[tokio::test]
  #[serial]
  async fn post_and_comment_vote_views() -> LemmyResult<()> {
    let pool = &build_db_pool_for_tests();
    let pool = &mut pool.into();

    let inserted_instance = Instance::read_or_create(pool, "my_domain.tld".to_string()).await?;

    let new_person = PersonInsertForm::test_form(inserted_instance.id, "timmy_vv");

    let inserted_timmy = Person::create(pool, &new_person).await?;

    let new_person_2 = PersonInsertForm::test_form(inserted_instance.id, "sara_vv");

    let inserted_sara = Person::create(pool, &new_person_2).await?;

    let new_community = CommunityInsertForm::new(
      inserted_instance.id,
      "test community vv".to_string(),
      "nada".to_owned(),
      "pubkey".to_string(),
    );
    let inserted_community = Community::create(pool, &new_community).await?;

    let new_post = PostInsertForm::new(
      "A test post vv".into(),
      inserted_timmy.id,
      inserted_community.id,
    );
    let inserted_post = Post::create(pool, &new_post).await?;

    let comment_form = CommentInsertForm::new(
      inserted_timmy.id,
      inserted_post.id,
      "A test comment vv".into(),
    );
    let inserted_comment = Comment::create(pool, &comment_form, None).await?;

    // Timmy upvotes his own post
    let timmy_post_vote_form = PostLikeForm::new(inserted_post.id, inserted_timmy.id, 1);
    PostActions::like(pool, &timmy_post_vote_form).await?;

    // Sara downvotes timmy's post
    let sara_post_vote_form = PostLikeForm::new(inserted_post.id, inserted_sara.id, -1);
    PostActions::like(pool, &sara_post_vote_form).await?;

    let mut expected_post_vote_views = [
      VoteView {
        creator: inserted_sara.clone(),
        creator_banned_from_community: false,
        score: -1,
      },
      VoteView {
        creator: inserted_timmy.clone(),
        creator_banned_from_community: false,
        score: 1,
      },
    ];
    expected_post_vote_views[1].creator.post_count = 1;
    expected_post_vote_views[1].creator.comment_count = 1;

    let read_post_vote_views = VoteView::list_for_post(pool, inserted_post.id, None, None).await?;
    assert_eq!(read_post_vote_views, expected_post_vote_views);

    // Timothy votes down his own comment
    let timmy_comment_vote_form = CommentLikeForm::new(inserted_timmy.id, inserted_comment.id, -1);
    CommentActions::like(pool, &timmy_comment_vote_form).await?;

    // Sara upvotes timmy's comment
    let sara_comment_vote_form = CommentLikeForm::new(inserted_sara.id, inserted_comment.id, 1);
    CommentActions::like(pool, &sara_comment_vote_form).await?;

    let mut expected_comment_vote_views = [
      VoteView {
        creator: inserted_timmy.clone(),
        creator_banned_from_community: false,
        score: -1,
      },
      VoteView {
        creator: inserted_sara.clone(),
        creator_banned_from_community: false,
        score: 1,
      },
    ];
    expected_comment_vote_views[0].creator.post_count = 1;
    expected_comment_vote_views[0].creator.comment_count = 1;

    let read_comment_vote_views =
      VoteView::list_for_comment(pool, inserted_comment.id, None, None).await?;
    assert_eq!(read_comment_vote_views, expected_comment_vote_views);

    // Ban timmy from that community
    let ban_timmy_form = CommunityPersonBanForm::new(inserted_community.id, inserted_timmy.id);
    CommunityActions::ban(pool, &ban_timmy_form).await?;

    // Make sure creator_banned_from_community is true
    let read_comment_vote_views_after_ban =
      VoteView::list_for_comment(pool, inserted_comment.id, None, None).await?;

    assert!(read_comment_vote_views_after_ban
      .first()
      .is_some_and(|c| c.creator_banned_from_community));

    let read_post_vote_views_after_ban =
      VoteView::list_for_post(pool, inserted_post.id, None, None).await?;

    assert!(read_post_vote_views_after_ban
      .get(1)
      .is_some_and(|p| p.creator_banned_from_community));

    // Cleanup
    Instance::delete(pool, inserted_instance.id).await?;

    Ok(())
  }
}
