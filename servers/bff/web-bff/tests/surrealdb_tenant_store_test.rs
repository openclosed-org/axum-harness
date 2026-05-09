use chrono::Utc;
use tenant_service::application::TenantServiceTrait;
use user_service::domain::User;
use web_bff::{config::Config, state::BffState};

#[tokio::test]
#[ignore = "requires a local external SurrealDB server"]
async fn bff_bootstraps_all_services_with_surrealdb_store() {
    let endpoint =
        std::env::var("APP_SURREALDB_URL").unwrap_or_else(|_| "http://127.0.0.1:8000".to_string());
    let tenant_name = format!("BFF SurrealDB Tenant {}", std::process::id());
    let user_sub = format!("surrealdb-bff-test-user-{}", std::process::id());

    let state = BffState::new(Config {
        database_url: Some("memory".to_string()),
        store_provider: "surrealdb".to_string(),
        surrealdb_url: Some(endpoint),
        surrealdb_ns: std::env::var("APP_SURREALDB_NS").unwrap_or_else(|_| "axh".to_string()),
        surrealdb_db: std::env::var("APP_SURREALDB_DB").unwrap_or_else(|_| "main".to_string()),
        surrealdb_user: std::env::var("APP_SURREALDB_USER").unwrap_or_else(|_| "root".to_string()),
        surrealdb_pass: Some(
            std::env::var("APP_SURREALDB_PASS").unwrap_or_else(|_| "root".to_string()),
        ),
        surrealdb_tenant_scope: "platform".to_string(),
        auth_mode: "dev_headers".to_string(),
        ..Config::default()
    })
    .await
    .unwrap();

    let tenant_service = state.tenant_service().expect("tenant service is wired");
    let initialized = tenant_service
        .init_tenant_for_user(&user_sub, &tenant_name)
        .await
        .unwrap();
    let loaded = tenant_service
        .get_tenant(&initialized.tenant_id)
        .await
        .unwrap()
        .expect("created tenant is readable from SurrealDB");

    assert_eq!(loaded.id, initialized.tenant_id);
    assert_eq!(loaded.name, tenant_name);

    let counter = state.counter_service().expect("counter service is wired");
    let counter_id = counter_service::domain::CounterId::new(&initialized.tenant_id);
    assert_eq!(
        counter
            .increment(&counter_id, Some("surrealdb-counter-1"))
            .await
            .unwrap(),
        1
    );
    assert_eq!(counter.get_value(&counter_id).await.unwrap(), 1);

    let user_repo = state
        .user_profile_repository()
        .expect("user profile repository is wired");
    user_repo
        .create_user(&User {
            id: format!("user-{}", std::process::id()),
            user_sub: user_sub.clone(),
            display_name: "SurrealDB Test User".to_string(),
            email: Some("surrealdb-test@example.com".to_string()),
            created_at: Utc::now(),
            last_login_at: Some(Utc::now()),
        })
        .await
        .unwrap();
    let user = user_repo
        .find_by_sub(&user_sub)
        .await
        .unwrap()
        .expect("created user is readable from SurrealDB");
    assert_eq!(user.user_sub, user_sub);
}
