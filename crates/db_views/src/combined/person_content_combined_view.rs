use crate::{
  structs::{
    CommentView,
    LocalUserView,
    PersonContentCombinedView,
    PersonContentCombinedViewInternal,
    PostView,
  },
  utils::home_instance_person_join,
};
use diesel::{
  BoolExpressionMethods,
  ExpressionMethods,
  JoinOnDsl,
  NullableExpressionMethods,
  QueryDsl,
  SelectableHelper,
};
use diesel_async::RunQueryDsl;
use i_love_jesus::PaginatedQueryBuilder;
use lemmy_db_schema::{
  self,
  aliases::{creator_community_actions, creator_local_user},
  newtypes::{PaginationCursor, PersonId},
  schema::{
    comment,
    comment_actions,
    community,
    community_actions,
    image_details,
    instance_actions,
    local_user,
    person,
    person_actions,
    person_content_combined,
    post,
    post_actions,
  },
  source::combined::person_content::{person_content_combined_keys as key, PersonContentCombined},
  traits::{InternalToCombinedView, PaginationCursorBuilder},
  utils::{get_conn, DbPool},
  PersonContentType,
};
use lemmy_utils::error::{LemmyErrorType, LemmyResult};

impl PersonContentCombinedViewInternal {
  #[diesel::dsl::auto_type(no_type_alias)]
  fn joins(my_person_id: Option<PersonId>) -> _ {
    let item_creator = person::id;

    let comment_join =
      comment::table.on(person_content_combined::comment_id.eq(comment::id.nullable()));

    let post_join = post::table.on(
      person_content_combined::post_id
        .eq(post::id.nullable())
        .or(comment::post_id.eq(post::id)),
    );

    let item_creator_join = person::table
      .on(
        comment::creator_id
          .eq(item_creator)
          // Need to filter out the post rows where the post_id given is null
          // Otherwise you'll get duped post rows
          .or(
            post::creator_id
              .eq(item_creator)
              .and(person_content_combined::post_id.is_not_null()),
          ),
      )
      .left_join(home_instance_person_join());

    let community_join = community::table.on(post::community_id.eq(community::id));

    let creator_community_actions_join = creator_community_actions.on(
      creator_community_actions
        .field(community_actions::community_id)
        .eq(post::community_id)
        .and(
          creator_community_actions
            .field(community_actions::person_id)
            .eq(item_creator),
        ),
    );
    let local_user_join = local_user::table.on(local_user::person_id.nullable().eq(my_person_id));

    let creator_local_user_join = creator_local_user.on(
      item_creator
        .eq(creator_local_user.field(local_user::person_id))
        .and(creator_local_user.field(local_user::admin).eq(true)),
    );

    let community_actions_join = community_actions::table.on(
      community_actions::community_id
        .eq(post::community_id)
        .and(community_actions::person_id.nullable().eq(my_person_id)),
    );

    let instance_actions_join = instance_actions::table.on(
      instance_actions::instance_id
        .eq(person::instance_id)
        .and(instance_actions::person_id.nullable().eq(my_person_id)),
    );

    let post_actions_join = post_actions::table.on(
      post_actions::post_id
        .eq(post::id)
        .and(post_actions::person_id.nullable().eq(my_person_id)),
    );

    let person_actions_join = person_actions::table.on(
      person_actions::target_id
        .eq(item_creator)
        .and(person_actions::person_id.nullable().eq(my_person_id)),
    );

    let comment_actions_join = comment_actions::table.on(
      comment_actions::comment_id
        .eq(comment::id)
        .and(comment_actions::person_id.nullable().eq(my_person_id)),
    );

    let image_details_join =
      image_details::table.on(post::thumbnail_url.eq(image_details::link.nullable()));

    person_content_combined::table
      .left_join(comment_join)
      .inner_join(post_join)
      .inner_join(item_creator_join)
      .inner_join(community_join)
      .left_join(creator_community_actions_join)
      .left_join(local_user_join)
      .left_join(creator_local_user_join)
      .left_join(community_actions_join)
      .left_join(instance_actions_join)
      .left_join(post_actions_join)
      .left_join(person_actions_join)
      .left_join(comment_actions_join)
      .left_join(image_details_join)
  }
}

impl PaginationCursorBuilder for PersonContentCombinedView {
  type CursorData = PersonContentCombined;

  fn to_cursor(&self) -> PaginationCursor {
    let (prefix, id) = match &self {
      PersonContentCombinedView::Comment(v) => ('C', v.comment.id.0),
      PersonContentCombinedView::Post(v) => ('P', v.post.id.0),
    };
    PaginationCursor::new(prefix, id)
  }

  async fn from_cursor(
    cursor: &PaginationCursor,
    pool: &mut DbPool<'_>,
  ) -> LemmyResult<Self::CursorData> {
    let conn = &mut get_conn(pool).await?;
    let (prefix, id) = cursor.prefix_and_id()?;

    let mut query = person_content_combined::table
      .select(Self::CursorData::as_select())
      .into_boxed();

    query = match prefix {
      'C' => query.filter(person_content_combined::comment_id.eq(id)),
      'P' => query.filter(person_content_combined::post_id.eq(id)),
      _ => return Err(LemmyErrorType::CouldntParsePaginationToken.into()),
    };
    let token = query.first(conn).await?;

    Ok(token)
  }
}

#[derive(derive_new::new)]
pub struct PersonContentCombinedQuery {
  pub creator_id: PersonId,
  #[new(default)]
  pub type_: Option<PersonContentType>,
  #[new(default)]
  pub cursor_data: Option<PersonContentCombined>,
  #[new(default)]
  pub page_back: Option<bool>,
}

impl PersonContentCombinedQuery {
  pub async fn list(
    self,
    pool: &mut DbPool<'_>,
    user: &Option<LocalUserView>,
  ) -> LemmyResult<Vec<PersonContentCombinedView>> {
    let my_person_id = user.as_ref().map(|u| u.local_user.person_id);
    let item_creator = person::id;

    let conn = &mut get_conn(pool).await?;

    // Notes: since the post_id and comment_id are optional columns,
    // many joins must use an OR condition.
    // For example, the creator must be the person table joined to either:
    // - post.creator_id
    // - comment.creator_id
    let mut query = PersonContentCombinedViewInternal::joins(my_person_id)
      // The creator id filter
      .filter(item_creator.eq(self.creator_id))
      .select(PersonContentCombinedViewInternal::as_select())
      .into_boxed();

    if let Some(type_) = self.type_ {
      query = match type_ {
        PersonContentType::All => query,
        PersonContentType::Comments => {
          query.filter(person_content_combined::comment_id.is_not_null())
        }
        PersonContentType::Posts => query.filter(person_content_combined::post_id.is_not_null()),
      }
    }

    let mut query = PaginatedQueryBuilder::new(query);

    if self.page_back.unwrap_or_default() {
      query = query.before(self.cursor_data).limit_and_offset_from_end();
    } else {
      query = query.after(self.cursor_data);
    }

    // Sorting by published
    query = query
      .then_desc(key::published)
      // Tie breaker
      .then_desc(key::id);

    let res = query
      .load::<PersonContentCombinedViewInternal>(conn)
      .await?;

    // Map the query results to the enum
    let out = res
      .into_iter()
      .filter_map(InternalToCombinedView::map_to_enum)
      .collect();

    Ok(out)
  }
}

impl InternalToCombinedView for PersonContentCombinedViewInternal {
  type CombinedView = PersonContentCombinedView;

  fn map_to_enum(self) -> Option<Self::CombinedView> {
    // Use for a short alias
    let v = self;

    if let Some(comment) = v.comment {
      Some(PersonContentCombinedView::Comment(CommentView {
        comment,
        post: v.post,
        community: v.community,
        creator: v.item_creator,
        community_actions: v.community_actions,
        comment_actions: v.comment_actions,
        person_actions: v.person_actions,
        instance_actions: v.instance_actions,
        home_instance_actions: v.home_instance_actions,
        creator_community_actions: v.creator_community_actions,
        creator_is_admin: v.item_creator_is_admin,
        can_mod: v.can_mod,
      }))
    } else {
      Some(PersonContentCombinedView::Post(PostView {
        post: v.post,
        community: v.community,
        creator: v.item_creator,
        image_details: v.image_details,
        community_actions: v.community_actions,
        post_actions: v.post_actions,
        person_actions: v.person_actions,
        instance_actions: v.instance_actions,
        home_instance_actions: v.home_instance_actions,
        creator_community_actions: v.creator_community_actions,
        creator_is_admin: v.item_creator_is_admin,
        can_mod: v.can_mod,
      }))
    }
  }
}

#[cfg(test)]
#[expect(clippy::indexing_slicing)]
mod tests {

  use crate::{
    combined::person_content_combined_view::PersonContentCombinedQuery,
    structs::PersonContentCombinedView,
  };
  use lemmy_db_schema::{
    source::{
      comment::{Comment, CommentInsertForm},
      community::{Community, CommunityInsertForm},
      instance::Instance,
      person::{Person, PersonInsertForm},
      post::{Post, PostInsertForm},
    },
    traits::Crud,
    utils::{build_db_pool_for_tests, DbPool},
  };
  use lemmy_utils::error::LemmyResult;
  use pretty_assertions::assert_eq;
  use serial_test::serial;

  struct Data {
    instance: Instance,
    timmy: Person,
    sara: Person,
    timmy_post: Post,
    timmy_post_2: Post,
    sara_post: Post,
    timmy_comment: Comment,
    sara_comment: Comment,
    sara_comment_2: Comment,
  }

  async fn init_data(pool: &mut DbPool<'_>) -> LemmyResult<Data> {
    let instance = Instance::read_or_create(pool, "my_domain.tld".to_string()).await?;

    let timmy_form = PersonInsertForm::test_form(instance.id, "timmy_pcv");
    let timmy = Person::create(pool, &timmy_form).await?;

    let sara_form = PersonInsertForm::test_form(instance.id, "sara_pcv");
    let sara = Person::create(pool, &sara_form).await?;

    let community_form = CommunityInsertForm::new(
      instance.id,
      "test community pcv".to_string(),
      "nada".to_owned(),
      "pubkey".to_string(),
    );
    let community = Community::create(pool, &community_form).await?;

    let timmy_post_form = PostInsertForm::new("timmy post prv".into(), timmy.id, community.id);
    let timmy_post = Post::create(pool, &timmy_post_form).await?;

    let timmy_post_form_2 = PostInsertForm::new("timmy post prv 2".into(), timmy.id, community.id);
    let timmy_post_2 = Post::create(pool, &timmy_post_form_2).await?;

    let sara_post_form = PostInsertForm::new("sara post prv".into(), sara.id, community.id);
    let sara_post = Post::create(pool, &sara_post_form).await?;

    let timmy_comment_form =
      CommentInsertForm::new(timmy.id, timmy_post.id, "timmy comment prv".into());
    let timmy_comment = Comment::create(pool, &timmy_comment_form, None).await?;

    let sara_comment_form =
      CommentInsertForm::new(sara.id, timmy_post.id, "sara comment prv".into());
    let sara_comment = Comment::create(pool, &sara_comment_form, None).await?;

    let sara_comment_form_2 =
      CommentInsertForm::new(sara.id, timmy_post_2.id, "sara comment prv 2".into());
    let sara_comment_2 = Comment::create(pool, &sara_comment_form_2, None).await?;

    Ok(Data {
      instance,
      timmy,
      sara,
      timmy_post,
      timmy_post_2,
      sara_post,
      timmy_comment,
      sara_comment,
      sara_comment_2,
    })
  }

  async fn cleanup(data: Data, pool: &mut DbPool<'_>) -> LemmyResult<()> {
    Instance::delete(pool, data.instance.id).await?;

    Ok(())
  }

  #[tokio::test]
  #[serial]
  async fn test_combined() -> LemmyResult<()> {
    let pool = &build_db_pool_for_tests();
    let pool = &mut pool.into();
    let data = init_data(pool).await?;

    // Do a batch read of timmy
    let timmy_content = PersonContentCombinedQuery::new(data.timmy.id)
      .list(pool, &None)
      .await?;
    assert_eq!(3, timmy_content.len());

    // Make sure the types are correct
    if let PersonContentCombinedView::Comment(v) = &timmy_content[0] {
      assert_eq!(data.timmy_comment.id, v.comment.id);
      assert_eq!(data.timmy.id, v.creator.id);
    } else {
      panic!("wrong type");
    }
    if let PersonContentCombinedView::Post(v) = &timmy_content[1] {
      assert_eq!(data.timmy_post_2.id, v.post.id);
      assert_eq!(data.timmy.id, v.post.creator_id);
    } else {
      panic!("wrong type");
    }
    if let PersonContentCombinedView::Post(v) = &timmy_content[2] {
      assert_eq!(data.timmy_post.id, v.post.id);
      assert_eq!(data.timmy.id, v.post.creator_id);
    } else {
      panic!("wrong type");
    }

    // Do a batch read of sara
    let sara_content = PersonContentCombinedQuery::new(data.sara.id)
      .list(pool, &None)
      .await?;
    assert_eq!(3, sara_content.len());

    // Make sure the report types are correct
    if let PersonContentCombinedView::Comment(v) = &sara_content[0] {
      assert_eq!(data.sara_comment_2.id, v.comment.id);
      assert_eq!(data.sara.id, v.creator.id);
      // This one was to timmy_post_2
      assert_eq!(data.timmy_post_2.id, v.post.id);
      assert_eq!(data.timmy.id, v.post.creator_id);
    } else {
      panic!("wrong type");
    }
    if let PersonContentCombinedView::Comment(v) = &sara_content[1] {
      assert_eq!(data.sara_comment.id, v.comment.id);
      assert_eq!(data.sara.id, v.creator.id);
      assert_eq!(data.timmy_post.id, v.post.id);
      assert_eq!(data.timmy.id, v.post.creator_id);
    } else {
      panic!("wrong type");
    }
    if let PersonContentCombinedView::Post(v) = &sara_content[2] {
      assert_eq!(data.sara_post.id, v.post.id);
      assert_eq!(data.sara.id, v.post.creator_id);
    } else {
      panic!("wrong type");
    }

    cleanup(data, pool).await?;

    Ok(())
  }
}
