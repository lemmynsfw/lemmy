use crate::{
  diesel::OptionalExtension,
  newtypes::{PersonId, PostId},
  schema::post_actions,
  source::post_actions::{PostActions, PostActionsForm},
  utils::{get_conn, now, DbPool},
};
use diesel::{
  expression::SelectableHelper,
  insert_into,
  result::Error,
  ExpressionMethods,
  NullableExpressionMethods,
  QueryDsl,
};
use diesel_async::RunQueryDsl;

impl PostActions {
  pub async fn upsert(pool: &mut DbPool<'_>, form: &PostActionsForm) -> Result<Self, Error> {
    let conn = &mut get_conn(pool).await?;
    let form = (form, post_actions::read_comments.eq(now().nullable()));
    insert_into(post_actions::table)
      .values(form)
      .on_conflict((post_actions::person_id, post_actions::post_id))
      .do_update()
      .set(form)
      .returning(Self::as_select())
      .get_result::<Self>(conn)
      .await
  }
  pub async fn read(
    pool: &mut DbPool<'_>,
    person_id_: PersonId,
    post_id_: PostId,
  ) -> Result<Option<Self>, Error> {
    let conn = &mut get_conn(pool).await?;
    post_actions::table
      .find((person_id_, post_id_))
      .filter(post_actions::read_comments.is_not_null())
      .select(Self::as_select())
      .first(conn)
      .await
      .optional()
  }
}
