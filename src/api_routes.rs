use actix_web::{guard, web::*};
use lemmy_api::{
  comment::{
    distinguish::distinguish_comment,
    like::like_comment,
    list_comment_likes::list_comment_likes,
    save::save_comment,
  },
  community::{
    add_mod::add_mod_to_community,
    ban::ban_from_community,
    block::user_block_community,
    follow::follow_community,
    multi_community_follow::follow_multi_community,
    pending_follows::{
      approve::post_pending_follows_approve,
      count::get_pending_follows_count,
      list::get_pending_follows_list,
    },
    random::get_random_community,
    tag::{create_community_tag, delete_community_tag, update_community_tag},
    transfer::transfer_community,
    update_notifications::update_community_notifications,
  },
  local_user::{
    add_admin::add_admin,
    ban_person::ban_from_site,
    block::user_block_person,
    change_password::change_password,
    change_password_after_reset::change_password_after_reset,
    donation_dialog_shown::donation_dialog_shown,
    export_data::export_data,
    generate_totp_secret::generate_totp_secret,
    get_captcha::get_captcha,
    list_hidden::list_person_hidden,
    list_liked::list_person_liked,
    list_logins::list_logins,
    list_media::list_media,
    list_read::list_person_read,
    list_saved::list_person_saved,
    login::login,
    logout::logout,
    note_person::user_note_person,
    notifications::{
      list::list_notifications,
      mark_all_read::mark_all_notifications_read,
      mark_notification_read::mark_notification_as_read,
      unread_count::unread_count,
    },
    report_count::report_count,
    resend_verification_email::resend_verification_email,
    reset_password::reset_password,
    save_settings::save_user_settings,
    update_totp::update_totp,
    user_block_instance::{user_block_instance_communities, user_block_instance_persons},
    validate_auth::validate_auth,
    verify_email::verify_email,
  },
  post::{
    feature::feature_post,
    get_link_metadata::get_link_metadata,
    hide::hide_post,
    like::like_post,
    list_post_likes::list_post_likes,
    lock::lock_post,
    mark_many_read::mark_posts_as_read,
    mark_read::mark_post_as_read,
    mod_update::mod_update_post,
    save::save_post,
    update_notifications::update_post_notifications,
  },
  reports::{
    comment_report::{create::create_comment_report, resolve::resolve_comment_report},
    community_report::{create::create_community_report, resolve::resolve_community_report},
    post_report::{create::create_post_report, resolve::resolve_post_report},
    private_message_report::{create::create_pm_report, resolve::resolve_pm_report},
    report_combined::list::list_reports,
  },
  site::{
    admin_allow_instance::admin_allow_instance,
    admin_block_instance::admin_block_instance,
    admin_list_users::admin_list_users,
    federated_instances::get_federated_instances,
    leave_admin::leave_admin,
    list_all_media::list_all_media,
    mod_log::get_mod_log,
    purge::{
      comment::purge_comment,
      community::purge_community,
      person::purge_person,
      post::purge_post,
    },
    registration_applications::{
      approve::approve_registration_application,
      get::get_registration_application,
      list::list_registration_applications,
      unread_count::get_unread_registration_application_count,
    },
  },
};
use lemmy_api_crud::{
  comment::{
    create::create_comment,
    delete::delete_comment,
    read::get_comment,
    remove::remove_comment,
    update::update_comment,
  },
  community::{
    create::create_community,
    delete::delete_community,
    list::list_communities,
    remove::remove_community,
    update::update_community,
  },
  custom_emoji::{
    create::create_custom_emoji,
    delete::delete_custom_emoji,
    list::list_custom_emojis,
    update::update_custom_emoji,
  },
  multi_community::{
    create::create_multi_community,
    create_entry::create_multi_community_entry,
    delete_entry::delete_multi_community_entry,
    get::get_multi_community,
    list::list_multi_communities,
    update::update_multi_community,
  },
  oauth_provider::{
    create::create_oauth_provider,
    delete::delete_oauth_provider,
    update::update_oauth_provider,
  },
  post::{
    create::create_post,
    delete::delete_post,
    read::get_post,
    remove::remove_post,
    update::update_post,
  },
  private_message::{
    create::create_private_message,
    delete::delete_private_message,
    update::update_private_message,
  },
  site::{create::create_site, read::get_site, update::update_site},
  tagline::{
    create::create_tagline,
    delete::delete_tagline,
    list::list_taglines,
    update::update_tagline,
  },
  user::{
    create::{authenticate_with_oauth, register},
    delete::delete_account,
    my_user::get_my_user,
  },
};
use lemmy_apub::api::{
  list_comments::{list_comments, list_comments_slim},
  list_person_content::list_person_content,
  list_posts::list_posts,
  read_community::get_community,
  read_person::read_person,
  resolve_object::resolve_object,
  search::search,
  user_settings_backup::{export_settings, import_settings},
};
use lemmy_routes::images::{
  delete::{
    delete_community_banner,
    delete_community_icon,
    delete_image,
    delete_image_admin,
    delete_site_banner,
    delete_site_icon,
    delete_user_avatar,
    delete_user_banner,
  },
  download::{get_image, image_proxy},
  pictrs_health,
  upload::{
    upload_community_banner,
    upload_community_icon,
    upload_image,
    upload_site_banner,
    upload_site_icon,
    upload_user_avatar,
    upload_user_banner,
  },
};
use lemmy_utils::rate_limit::RateLimit;

pub fn config(cfg: &mut ServiceConfig, rate_limit: &RateLimit) {
  cfg.service(
    scope("/api/v4")
      .wrap(rate_limit.message())
      // Site
      .service(
        scope("/site")
          .route("", get().to(get_site))
          .route("", post().to(create_site))
          .route("", put().to(update_site))
          .route("/icon", post().to(upload_site_icon))
          .route("/icon", delete().to(delete_site_icon))
          .route("/banner", post().to(upload_site_banner))
          .route("/banner", delete().to(delete_site_banner)),
      )
      .route("/modlog", get().to(get_mod_log))
      .service(
        resource("/search")
          .wrap(rate_limit.search())
          .route(get().to(search)),
      )
      .service(
        resource("/resolve_object")
          .wrap(rate_limit.search())
          .route(get().to(resolve_object)),
      )
      // Community
      .service(
        resource("/community")
          .guard(guard::Post())
          .wrap(rate_limit.register())
          .route(post().to(create_community)),
      )
      .service(
        scope("/community")
          .route("", get().to(get_community))
          .route("", put().to(update_community))
          .route("/random", get().to(get_random_community))
          .route("/list", get().to(list_communities))
          .route("/follow", post().to(follow_community))
          .route("/report", post().to(create_community_report))
          .route("/report/resolve", put().to(resolve_community_report))
          .route("/delete", post().to(delete_community))
          // Mod Actions
          .route("/remove", post().to(remove_community))
          .route("/transfer", post().to(transfer_community))
          .route("/ban_user", post().to(ban_from_community))
          .route("/mod", post().to(add_mod_to_community))
          .route("/icon", post().to(upload_community_icon))
          .route("/icon", delete().to(delete_community_icon))
          .route("/banner", post().to(upload_community_banner))
          .route("/banner", delete().to(delete_community_banner))
          .route("/tag", post().to(create_community_tag))
          .route("/tag", put().to(update_community_tag))
          .route("/tag", delete().to(delete_community_tag))
          .route("/notifications", post().to(update_community_notifications))
          .service(
            scope("/pending_follows")
              .route("/count", get().to(get_pending_follows_count))
              .route("/list", get().to(get_pending_follows_list))
              .route("/approve", post().to(post_pending_follows_approve)),
          ),
      )
      .service(
        scope("/multi_community")
          .route("", post().to(create_multi_community))
          .route("", put().to(update_multi_community))
          .route("", get().to(get_multi_community))
          .route("/entry", post().to(create_multi_community_entry))
          .route("/entry", delete().to(delete_multi_community_entry))
          .route("/list", get().to(list_multi_communities))
          .route("/follow", post().to(follow_multi_community)),
      )
      .route("/federated_instances", get().to(get_federated_instances))
      // Post
      .service(
        resource("/post")
          // Handle POST to /post separately to add the post() rate limitter
          .guard(guard::Post())
          .wrap(rate_limit.post())
          .route(post().to(create_post)),
      )
      .service(
        resource("/post/site_metadata")
          .wrap(rate_limit.search())
          .route(get().to(get_link_metadata)),
      )
      .service(
        scope("/post")
          .route("", get().to(get_post))
          .route("", put().to(update_post))
          .route("/delete", post().to(delete_post))
          .route("/remove", post().to(remove_post))
          .route("/mark_as_read", post().to(mark_post_as_read))
          .route("/mark_as_read/many", post().to(mark_posts_as_read))
          .route("/hide", post().to(hide_post))
          .route("/lock", post().to(lock_post))
          .route("/feature", post().to(feature_post))
          .route("/list", get().to(list_posts))
          .route("/like", post().to(like_post))
          .route("/like/list", get().to(list_post_likes))
          .route("/save", put().to(save_post))
          .route("/report", post().to(create_post_report))
          .route("/report/resolve", put().to(resolve_post_report))
          .route("/notifications", post().to(update_post_notifications))
          .route("/mod_update", put().to(mod_update_post)),
      )
      // Comment
      .service(
        // Handle POST to /comment separately to add the comment() rate limitter
        resource("/comment")
          .guard(guard::Post())
          .wrap(rate_limit.comment())
          .route(post().to(create_comment)),
      )
      .service(
        scope("/comment")
          .route("", get().to(get_comment))
          .route("", put().to(update_comment))
          .route("/delete", post().to(delete_comment))
          .route("/remove", post().to(remove_comment))
          .route("/distinguish", post().to(distinguish_comment))
          .route("/like", post().to(like_comment))
          .route("/like/list", get().to(list_comment_likes))
          .route("/save", put().to(save_comment))
          .route("/list", get().to(list_comments))
          .route("/list/slim", get().to(list_comments_slim))
          .route("/report", post().to(create_comment_report))
          .route("/report/resolve", put().to(resolve_comment_report)),
      )
      // Private Message
      .service(
        scope("/private_message")
          .route("", post().to(create_private_message))
          .route("", put().to(update_private_message))
          .route("/delete", post().to(delete_private_message))
          .route("/report", post().to(create_pm_report))
          .route("/report/resolve", put().to(resolve_pm_report)),
      )
      // Reports
      .service(
        scope("/report")
          .wrap(rate_limit.message())
          .route("/list", get().to(list_reports)),
      )
      // User
      .service(
        scope("/account/auth")
          .guard(guard::Post())
          .wrap(rate_limit.register())
          .route("/register", post().to(register))
          .route("/login", post().to(login))
          .route("/logout", post().to(logout))
          .route("/password_reset", post().to(reset_password))
          .route("/get_captcha", get().to(get_captcha))
          .route("/password_change", post().to(change_password_after_reset))
          .route("/change_password", put().to(change_password))
          .route("/totp/generate", post().to(generate_totp_secret))
          .route("/totp/update", post().to(update_totp))
          .route("/verify_email", post().to(verify_email))
          .route(
            "/resend_verification_email",
            post().to(resend_verification_email),
          ),
      )
      .service(
        scope("/account")
          .route("", get().to(get_my_user))
          .service(
            scope("/media")
              .route("", delete().to(delete_image))
              .route("/list", get().to(list_media)),
          )
          .route("/notifications", get().to(list_notifications))
          .route("/delete", post().to(delete_account))
          .route("/mark_as_read/all", post().to(mark_all_notifications_read))
          .route("/mark_as_read", post().to(mark_notification_as_read))
          .route("/report_count", get().to(report_count))
          .route("/unread_count", get().to(unread_count))
          .route("/list_logins", get().to(list_logins))
          .route("/validate_auth", get().to(validate_auth))
          .route("/donation_dialog_shown", post().to(donation_dialog_shown))
          .route("/avatar", post().to(upload_user_avatar))
          .route("/avatar", delete().to(delete_user_avatar))
          .route("/banner", post().to(upload_user_banner))
          .route("/banner", delete().to(delete_user_banner))
          .service(
            scope("/block")
              .route("/person", post().to(user_block_person))
              .route("/community", post().to(user_block_community))
              .route(
                "/instance/communities",
                post().to(user_block_instance_communities),
              )
              .route("/instance/persons", post().to(user_block_instance_persons)),
          )
          .route("/saved", get().to(list_person_saved))
          .route("/read", get().to(list_person_read))
          .route("/hidden", get().to(list_person_hidden))
          .route("/liked", get().to(list_person_liked))
          .route("/settings/save", put().to(save_user_settings))
          // Account settings import / export have a strict rate limit
          .service(
            scope("/settings")
              .wrap(rate_limit.import_user_settings())
              .route("/export", get().to(export_settings))
              .route("/import", post().to(import_settings)),
          )
          .service(
            resource("/data/export")
              .wrap(rate_limit.import_user_settings())
              .route(get().to(export_data)),
          ),
      )
      // User actions
      .service(
        scope("/person")
          .route("", get().to(read_person))
          .route("/content", get().to(list_person_content))
          .route("/note", post().to(user_note_person)),
      )
      // Admin Actions
      .service(
        scope("/admin")
          .route("/add", post().to(add_admin))
          .route(
            "/registration_application/count",
            get().to(get_unread_registration_application_count),
          )
          .route(
            "/registration_application/list",
            get().to(list_registration_applications),
          )
          .route(
            "/registration_application/approve",
            put().to(approve_registration_application),
          )
          .route(
            "/registration_application",
            get().to(get_registration_application),
          )
          .service(
            scope("/purge")
              .route("/person", post().to(purge_person))
              .route("/community", post().to(purge_community))
              .route("/post", post().to(purge_post))
              .route("/comment", post().to(purge_comment)),
          )
          .service(
            scope("/tagline")
              .route("", post().to(create_tagline))
              .route("", put().to(update_tagline))
              .route("/delete", post().to(delete_tagline))
              .route("/list", get().to(list_taglines)),
          )
          .route("/ban", post().to(ban_from_site))
          .route("/users", get().to(admin_list_users))
          .route("/leave", post().to(leave_admin))
          .service(
            scope("/instance")
              .route("/block", post().to(admin_block_instance))
              .route("/allow", post().to(admin_allow_instance)),
          ),
      )
      .service(
        scope("/custom_emoji")
          .route("", post().to(create_custom_emoji))
          .route("", put().to(update_custom_emoji))
          .route("/delete", post().to(delete_custom_emoji))
          .route("/list", get().to(list_custom_emojis)),
      )
      .service(
        scope("/oauth_provider")
          .route("", post().to(create_oauth_provider))
          .route("", put().to(update_oauth_provider))
          .route("/delete", post().to(delete_oauth_provider)),
      )
      .service(
        scope("/oauth")
          .wrap(rate_limit.register())
          .route("/authenticate", post().to(authenticate_with_oauth)),
      )
      .service(
        scope("/image")
          .service(
            resource("")
              .wrap(rate_limit.image())
              .route(post().to(upload_image))
              .route(delete().to(delete_image_admin)),
          )
          .route("/proxy", get().to(image_proxy))
          .route("/health", get().to(pictrs_health))
          .route("/list", get().to(list_all_media))
          .route("/{filename}", get().to(get_image)),
      ),
  );
}
