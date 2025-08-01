use crate::{
  newtypes::{CommunityId, DbUrl, LanguageId, LocalUserId, PersonId},
  source::{
    actor_language::LocalUserLanguage,
    local_user::{LocalUser, LocalUserInsertForm, LocalUserUpdateForm},
    site::Site,
  },
  utils::{
    functions::{coalesce, lower},
    get_conn,
    now,
    DbPool,
  },
};
use bcrypt::{hash, DEFAULT_COST};
use diesel::{
  dsl::{insert_into, not, IntervalDsl},
  result::Error,
  CombineDsl,
  ExpressionMethods,
  JoinOnDsl,
  QueryDsl,
};
use diesel_async::RunQueryDsl;
use lemmy_db_schema_file::{
  enums::CommunityVisibility,
  schema::{community, community_actions, local_user, person, registration_application},
};
use lemmy_utils::error::{LemmyErrorExt, LemmyErrorType, LemmyResult};

impl LocalUser {
  pub async fn create(
    pool: &mut DbPool<'_>,
    form: &LocalUserInsertForm,
    languages: Vec<LanguageId>,
  ) -> LemmyResult<LocalUser> {
    let conn = &mut get_conn(pool).await?;
    let mut form_with_encrypted_password = form.clone();

    if let Some(password_encrypted) = &form.password_encrypted {
      let password_hash = hash(password_encrypted, DEFAULT_COST)?;
      form_with_encrypted_password.password_encrypted = Some(password_hash);
    }

    let local_user_ = insert_into(local_user::table)
      .values(form_with_encrypted_password)
      .get_result::<Self>(conn)
      .await?;

    LocalUserLanguage::update(pool, languages, local_user_.id).await?;

    Ok(local_user_)
  }

  pub async fn update(
    pool: &mut DbPool<'_>,
    local_user_id: LocalUserId,
    form: &LocalUserUpdateForm,
  ) -> LemmyResult<usize> {
    let conn = &mut get_conn(pool).await?;
    let res = diesel::update(local_user::table.find(local_user_id))
      .set(form)
      .execute(conn)
      .await;
    // Diesel will throw an error if the query is all Nones (not updating anything), ignore this.
    match res {
      Err(Error::QueryBuilderError(_)) => Ok(0),
      other => other,
    }
    .with_lemmy_type(LemmyErrorType::CouldntUpdate)
  }

  pub async fn delete(pool: &mut DbPool<'_>, id: LocalUserId) -> LemmyResult<usize> {
    let conn = &mut *get_conn(pool).await?;
    diesel::delete(local_user::table.find(id))
      .execute(conn)
      .await
      .with_lemmy_type(LemmyErrorType::Deleted)
  }

  pub async fn update_password(
    pool: &mut DbPool<'_>,
    local_user_id: LocalUserId,
    new_password: &str,
  ) -> LemmyResult<Self> {
    let conn = &mut get_conn(pool).await?;
    let password_hash = hash(new_password, DEFAULT_COST)?;

    diesel::update(local_user::table.find(local_user_id))
      .set((local_user::password_encrypted.eq(password_hash),))
      .get_result::<Self>(conn)
      .await
      .with_lemmy_type(LemmyErrorType::CouldntUpdate)
  }

  pub async fn set_all_users_email_verified(pool: &mut DbPool<'_>) -> LemmyResult<Vec<Self>> {
    let conn = &mut get_conn(pool).await?;
    diesel::update(local_user::table)
      .set(local_user::email_verified.eq(true))
      .get_results::<Self>(conn)
      .await
      .with_lemmy_type(LemmyErrorType::CouldntUpdate)
  }

  pub async fn set_all_users_registration_applications_accepted(
    pool: &mut DbPool<'_>,
  ) -> LemmyResult<Vec<Self>> {
    let conn = &mut get_conn(pool).await?;
    diesel::update(local_user::table)
      .set(local_user::accepted_application.eq(true))
      .get_results::<Self>(conn)
      .await
      .with_lemmy_type(LemmyErrorType::CouldntUpdate)
  }

  pub async fn delete_old_denied_local_users(pool: &mut DbPool<'_>) -> LemmyResult<usize> {
    let conn = &mut get_conn(pool).await?;

    // Make sure:
    // - An admin has interacted with the application
    // - The app is older than a week
    // - The accepted_application is false
    let old_denied_registrations = registration_application::table
      .filter(registration_application::admin_id.is_not_null())
      .filter(registration_application::published_at.lt(now() - 1.week()))
      .select(registration_application::local_user_id);

    // Delete based on join logic is here:
    // https://stackoverflow.com/questions/60836040/how-do-i-perform-a-delete-with-sub-query-in-diesel-against-a-postgres-database
    let local_users = local_user::table
      .filter(local_user::id.eq_any(old_denied_registrations))
      .filter(not(local_user::accepted_application))
      .select(local_user::person_id);

    // Delete the person rows, which should automatically clear the local_user ones
    let persons = person::table.filter(person::id.eq_any(local_users));

    diesel::delete(persons)
      .execute(conn)
      .await
      .with_lemmy_type(LemmyErrorType::Deleted)
  }

  pub async fn check_is_email_taken(pool: &mut DbPool<'_>, email: &str) -> LemmyResult<()> {
    use diesel::dsl::{exists, select};
    let conn = &mut get_conn(pool).await?;
    select(not(exists(local_user::table.filter(
      lower(coalesce(local_user::email, "")).eq(email.to_lowercase()),
    ))))
    .get_result::<bool>(conn)
    .await?
    .then_some(())
    .ok_or(LemmyErrorType::EmailAlreadyTaken.into())
  }

  // TODO: maybe move this and pass in LocalUserView
  pub async fn export_backup(
    pool: &mut DbPool<'_>,
    person_id_: PersonId,
  ) -> LemmyResult<UserBackupLists> {
    use lemmy_db_schema_file::schema::{
      comment,
      comment_actions,
      community,
      community_actions,
      instance,
      instance_actions,
      person_actions,
      post,
      post_actions,
    };
    let conn = &mut get_conn(pool).await?;

    let followed_communities = community_actions::table
      .filter(community_actions::followed_at.is_not_null())
      .filter(community_actions::person_id.eq(person_id_))
      .inner_join(community::table)
      .select(community::ap_id)
      .get_results(conn)
      .await?;

    let saved_posts = post_actions::table
      .filter(post_actions::saved_at.is_not_null())
      .filter(post_actions::person_id.eq(person_id_))
      .inner_join(post::table)
      .select(post::ap_id)
      .get_results(conn)
      .await?;

    let saved_comments = comment_actions::table
      .filter(comment_actions::saved_at.is_not_null())
      .filter(comment_actions::person_id.eq(person_id_))
      .inner_join(comment::table)
      .select(comment::ap_id)
      .get_results(conn)
      .await?;

    let blocked_communities = community_actions::table
      .filter(community_actions::blocked_at.is_not_null())
      .filter(community_actions::person_id.eq(person_id_))
      .inner_join(community::table)
      .select(community::ap_id)
      .get_results(conn)
      .await?;

    let blocked_users = person_actions::table
      .filter(person_actions::blocked_at.is_not_null())
      .filter(person_actions::person_id.eq(person_id_))
      .inner_join(person::table.on(person_actions::target_id.eq(person::id)))
      .select(person::ap_id)
      .get_results(conn)
      .await?;

    let blocked_instances_communities = instance_actions::table
      .filter(instance_actions::blocked_communities_at.is_not_null())
      .filter(instance_actions::person_id.eq(person_id_))
      .inner_join(instance::table)
      .select(instance::domain)
      .get_results(conn)
      .await?;

    let blocked_instances_persons = instance_actions::table
      .filter(instance_actions::blocked_persons_at.is_not_null())
      .filter(instance_actions::person_id.eq(person_id_))
      .inner_join(instance::table)
      .select(instance::domain)
      .get_results(conn)
      .await?;

    // TODO: use join for parallel queries?

    Ok(UserBackupLists {
      followed_communities,
      saved_posts,
      saved_comments,
      blocked_communities,
      blocked_users,
      blocked_instances_communities,
      blocked_instances_persons,
    })
  }

  /// Checks to make sure the acting admin is higher than the target admin
  pub async fn is_higher_admin_check(
    pool: &mut DbPool<'_>,
    admin_person_id: PersonId,
    target_person_ids: Vec<PersonId>,
  ) -> LemmyResult<()> {
    let conn = &mut get_conn(pool).await?;

    // Build the list of persons
    let mut persons = target_person_ids;
    persons.push(admin_person_id);
    persons.dedup();

    let res = local_user::table
      .filter(local_user::admin.eq(true))
      .filter(local_user::person_id.eq_any(persons))
      .order_by(local_user::id)
      // This does a limit 1 select first
      .first::<LocalUser>(conn)
      .await?;

    // If the first result sorted by published is the acting admin
    if res.person_id == admin_person_id {
      Ok(())
    } else {
      Err(LemmyErrorType::NotHigherAdmin)?
    }
  }

  /// Checks to make sure the acting moderator is higher than the target moderator
  pub async fn is_higher_mod_or_admin_check(
    pool: &mut DbPool<'_>,
    for_community_id: CommunityId,
    admin_person_id: PersonId,
    target_person_ids: Vec<PersonId>,
  ) -> LemmyResult<()> {
    let conn = &mut get_conn(pool).await?;

    // Build the list of persons
    let mut persons = target_person_ids;
    persons.push(admin_person_id);
    persons.dedup();

    let admins = local_user::table
      .filter(local_user::admin.eq(true))
      .filter(local_user::person_id.eq_any(&persons))
      .order_by(local_user::id)
      .select(local_user::person_id);

    let mods = community_actions::table
      .filter(community_actions::became_moderator_at.is_not_null())
      .filter(community_actions::community_id.eq(for_community_id))
      .filter(community_actions::person_id.eq_any(&persons))
      .order_by(community_actions::became_moderator_at)
      .select(community_actions::person_id);

    let res = admins.union_all(mods).get_results::<PersonId>(conn).await?;
    let first_person = res.as_slice().first().ok_or(LemmyErrorType::NotHigherMod)?;

    // If the first result sorted by published is the acting mod
    if *first_person == admin_person_id {
      Ok(())
    } else {
      Err(LemmyErrorType::NotHigherMod)?
    }
  }
}

/// Adds some helper functions for an optional LocalUser
pub trait LocalUserOptionHelper {
  fn person_id(&self) -> Option<PersonId>;
  fn local_user_id(&self) -> Option<LocalUserId>;
  fn show_bot_accounts(&self) -> bool;
  fn show_read_posts(&self) -> bool;
  fn is_admin(&self) -> bool;
  fn show_nsfw(&self, site: &Site) -> bool;
  fn hide_media(&self) -> bool;
  fn visible_communities_only<Q>(&self, query: Q) -> Q
  where
    Q: diesel::query_dsl::methods::FilterDsl<
      diesel::dsl::Eq<community::visibility, CommunityVisibility>,
      Output = Q,
    >;
}

impl LocalUserOptionHelper for Option<&LocalUser> {
  fn person_id(&self) -> Option<PersonId> {
    self.map(|l| l.person_id)
  }

  fn local_user_id(&self) -> Option<LocalUserId> {
    self.map(|l| l.id)
  }

  fn show_bot_accounts(&self) -> bool {
    self.map(|l| l.show_bot_accounts).unwrap_or(true)
  }

  fn show_read_posts(&self) -> bool {
    self.map(|l| l.show_read_posts).unwrap_or(true)
  }

  fn is_admin(&self) -> bool {
    self.map(|l| l.admin).unwrap_or(false)
  }

  fn show_nsfw(&self, site: &Site) -> bool {
    self
      .map(|l| l.show_nsfw)
      .unwrap_or(site.content_warning.is_some())
  }

  fn hide_media(&self) -> bool {
    self.map(|l| l.hide_media).unwrap_or(false)
  }

  // TODO: use this function for private community checks, but the generics get extremely confusing
  fn visible_communities_only<Q>(&self, query: Q) -> Q
  where
    Q: diesel::query_dsl::methods::FilterDsl<
      diesel::dsl::Eq<community::visibility, CommunityVisibility>,
      Output = Q,
    >,
  {
    if self.is_none() {
      query.filter(community::visibility.eq(CommunityVisibility::Public))
    } else {
      query
    }
  }
}

impl LocalUserInsertForm {
  pub fn test_form(person_id: PersonId) -> Self {
    Self::new(person_id, Some(String::new()))
  }

  pub fn test_form_admin(person_id: PersonId) -> Self {
    LocalUserInsertForm {
      admin: Some(true),
      ..Self::test_form(person_id)
    }
  }
}

pub struct UserBackupLists {
  pub followed_communities: Vec<DbUrl>,
  pub saved_posts: Vec<DbUrl>,
  pub saved_comments: Vec<DbUrl>,
  pub blocked_communities: Vec<DbUrl>,
  pub blocked_users: Vec<DbUrl>,
  pub blocked_instances_communities: Vec<String>,
  pub blocked_instances_persons: Vec<String>,
}

#[cfg(test)]
mod tests {
  use crate::{
    source::{
      instance::Instance,
      local_user::{LocalUser, LocalUserInsertForm},
      person::{Person, PersonInsertForm},
    },
    traits::Crud,
    utils::build_db_pool_for_tests,
  };
  use lemmy_utils::error::LemmyResult;
  use serial_test::serial;

  #[tokio::test]
  #[serial]
  async fn test_admin_higher_check() -> LemmyResult<()> {
    let pool = &build_db_pool_for_tests();
    let pool = &mut pool.into();

    let inserted_instance = Instance::read_or_create(pool, "my_domain.tld".to_string()).await?;

    let fiona_person = PersonInsertForm::test_form(inserted_instance.id, "fiona");
    let inserted_fiona_person = Person::create(pool, &fiona_person).await?;

    let fiona_local_user_form = LocalUserInsertForm::test_form_admin(inserted_fiona_person.id);
    let _inserted_fiona_local_user =
      LocalUser::create(pool, &fiona_local_user_form, vec![]).await?;

    let delores_person = PersonInsertForm::test_form(inserted_instance.id, "delores");
    let inserted_delores_person = Person::create(pool, &delores_person).await?;
    let delores_local_user_form = LocalUserInsertForm::test_form_admin(inserted_delores_person.id);
    let _inserted_delores_local_user =
      LocalUser::create(pool, &delores_local_user_form, vec![]).await?;

    let admin_person_ids = vec![inserted_fiona_person.id, inserted_delores_person.id];

    // Make sure fiona is marked as a higher admin than delores, and vice versa
    let fiona_higher_check =
      LocalUser::is_higher_admin_check(pool, inserted_fiona_person.id, admin_person_ids.clone())
        .await;
    assert!(fiona_higher_check.is_ok());

    // This should throw an error, since delores was added later
    let delores_higher_check =
      LocalUser::is_higher_admin_check(pool, inserted_delores_person.id, admin_person_ids).await;
    assert!(delores_higher_check.is_err());

    Instance::delete(pool, inserted_instance.id).await?;

    Ok(())
  }

  #[tokio::test]
  #[serial]
  async fn test_email_taken() -> LemmyResult<()> {
    let pool = &build_db_pool_for_tests();
    let pool = &mut pool.into();

    let darwin_email = "charles.darwin@gmail.com";

    let inserted_instance = Instance::read_or_create(pool, "my_domain.tld".to_string()).await?;

    let darwin_person = PersonInsertForm::test_form(inserted_instance.id, "darwin");
    let inserted_darwin_person = Person::create(pool, &darwin_person).await?;

    let mut darwin_local_user_form =
      LocalUserInsertForm::test_form_admin(inserted_darwin_person.id);
    darwin_local_user_form.email = Some(darwin_email.into());
    let _inserted_darwin_local_user =
      LocalUser::create(pool, &darwin_local_user_form, vec![]).await?;

    let check = LocalUser::check_is_email_taken(pool, darwin_email).await;
    assert!(check.is_err());

    let passed_check = LocalUser::check_is_email_taken(pool, "not_charles@gmail.com").await;
    assert!(passed_check.is_ok());

    Ok(())
  }
}
