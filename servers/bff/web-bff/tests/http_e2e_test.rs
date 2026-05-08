//! HTTP End-to-End tests for web-bff.
//!
//! Uses axum::Router + tower::ServiceExt::oneshot to simulate real HTTP requests
//! without needing a running server. Turso runs in-memory for isolation.

use authn_oidc_verifier::{OidcVerifier, OidcVerifierConfig};
use axum::{
    body::Body,
    http::{self, Request, StatusCode},
};
use chrono::Utc;
use counter_service::infrastructure::LibSqlCounterRepository;
use http_body_util::BodyExt;
use mockito::Server;
use storage_turso::EmbeddedTurso;
use tower::ServiceExt;
use user_service::{domain::User, infrastructure::LibSqlUserRepository, ports::UserRepository};
use web_bff::{audit::AuditOutcome, create_router, openapi, state::BffState};

/// Helper: build test AppState with embedded Turso
async fn build_test_state() -> BffState {
    // Use unique temp directory for each test to avoid database locking
    let temp_dir = std::env::temp_dir().join(format!("web_bff_test_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");
    let db_path = temp_dir.join("test.db");

    let db = EmbeddedTurso::new(db_path.to_str().expect("non-utf8 path"))
        .await
        .expect("Failed to initialize embedded Turso");

    // Run migrations
    storage_turso::embedded::run_tenant_migrations(&db)
        .await
        .expect("Failed to run tenant migrations");

    // Run counter migration
    let repo = LibSqlCounterRepository::new(db.clone());
    repo.migrate()
        .await
        .expect("Failed to run counter migration");

    BffState::new_with_db(db).await
}

async fn build_oidc_test_state(issuer: &str, audience: &str) -> BffState {
    let mut state = build_test_state().await;
    state.config_mut().jwt_secret = "unused-when-oidc-is-enabled".to_string();
    state.config_mut().oidc_issuer = issuer.to_string();
    state.config_mut().oidc_audience = audience.to_string();
    state.set_oidc_verifier(Some(test_oidc_verifier(&state)));
    state
}

async fn build_introspection_test_state(
    issuer: &str,
    audience: &str,
    introspection_url: &str,
    client_id: &str,
    client_secret: &str,
) -> BffState {
    let mut state = build_oidc_test_state(issuer, audience).await;
    state.config_mut().oidc_introspection_url = introspection_url.to_string();
    state.config_mut().oidc_introspection_client_id = client_id.to_string();
    state.config_mut().oidc_introspection_client_secret = client_secret.to_string();
    state.set_oidc_verifier(Some(test_oidc_verifier(&state)));
    state
}

fn test_oidc_verifier(state: &BffState) -> OidcVerifier {
    OidcVerifier::new(
        OidcVerifierConfig {
            issuer: state.config().oidc_issuer.clone(),
            audience: state.config().oidc_audience.clone(),
            introspection_url: state.config().oidc_introspection_url.clone(),
            introspection_client_id: state.config().oidc_introspection_client_id.clone(),
            introspection_client_secret: state.config().oidc_introspection_client_secret.clone(),
        },
        state.http_client(),
    )
}

async fn build_dev_headers_state() -> BffState {
    let mut state = build_test_state().await;
    state.config_mut().auth_mode = "dev_headers".to_string();
    state
}

async fn create_user_profile(state: &BffState, user_sub: &str, display_name: &str, email: &str) {
    let repo = state
        .user_profile_repository()
        .expect("user profile repository should be available");

    repo.create_user(&User {
        id: uuid::Uuid::new_v4().to_string(),
        user_sub: user_sub.to_string(),
        display_name: display_name.to_string(),
        email: Some(email.to_string()),
        created_at: Utc::now(),
        last_login_at: Some(Utc::now()),
    })
    .await
    .expect("failed to create test user profile");
}

/// Extract JSON body from an axum Response
async fn body_to_json<T: serde::de::DeserializeOwned>(response: axum::response::Response) -> T {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    assert!(
        !bytes.is_empty(),
        "Response body is empty — handler likely returned an error status"
    );
    serde_json::from_slice(&bytes).unwrap()
}

/// Create a test JWT token for the given sub
fn make_test_token(sub: &str) -> String {
    use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};

    #[derive(serde::Serialize)]
    struct Claims {
        sub: String,
        exp: usize,
    }
    encode(
        &Header::new(Algorithm::HS256),
        &Claims {
            sub: sub.to_string(),
            exp: 9999999999,
        },
        &EncodingKey::from_secret(b"test-secret"),
    )
    .unwrap()
}

const TEST_RSA_PRIVATE_KEY: &str = "-----BEGIN PRIVATE KEY-----\nMIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQDPgCpR4svx5LYk\nyK6M7jAGNu8CtM7Rlq12pnog2ePTZPdj+E9tfhaIHHR9HZgaVfZyfRFcdA4yBN6O\nyOdgN7hSlANEJeUhBRIetkvzGOEpvLiPGisGc1DkYeA4bzTUPizbL77amz2STbMq\n1dSNptv9sr7dB+VSpEfUV66L9Kjvm6x28YqwDDfVwE8mxXzIR3cZOXFJTM/nYhf3\nUZ8oYfqGL8JmCTqai/nkOcpM7qj8CWrtXneSn+FRYMM/TnF3E4tlRYTg/FVOog5+\n8DfIjw9Thf8y8XbVKJPQJt0UCxc/a5Hb/7UTxDl5MVN68BCEvLdbHIHjMV61bn3v\nQUwYVkulAgMBAAECggEAFdAx4rjWWsAB290S6HLTrpuQxbaPNV5DLwFyPkjZl+v5\ny9MbOnXyVW20W0DEsCQQS9nU/OSgZ2a2pMj+9dD1ugygSUY4j5+SV5MvacdYSER0\nHGsSUdPGkbOuWBBsu9ErcwFSbXW7Y8lyR9MBzMBZSRLE2MSPOYBWor5y9XiLV+DT\nky5ri59rRpK8suJLmbvl8PyY6LwC9mrsXAFffe1rJLnFRR3grthXTramvksWPi4T\njUYYCZFEQphZqoMm2/ffwF/22QDACpC/aqxqhdep+I9JPDY13jSLStyKHb7LWwBb\nCZGR2BDP82BZAcbUeQXud1A6LIo35Arn+luomDOWcQKBgQD1cu10c3+130GxZJD4\nd9C4Ou5rUhNbzrICoQuefHiEpQMngOffS9Pl3A3UnIQWrddjG4cUlVcefbNzzljA\nGishXSegzMT0J8CaVtRgFQhw0xWjJnBA+q4zmpU/6SaqkyitewvsqOtMwSOVBPXr\n3v1rPf9gs4SrcTiQWF4L4acfDQKBgQDYa6GXVyMAVmYeeCsGCCLH0+MrqzO+D+K4\nKn4hP3g8x+23HufObupibupEGDRzFTOoKXka8984XNw2a6Jjrf8cC+FaL9bz7sxs\nSDgv1JbrfBm2+Xta4m7gIRBXlCCGGLe3JETwbnbmusFQd/Paq90BasRNzZTdsPFH\nJiBu6Gd4+QKBgC9vyMiq0dHalh2sq//5WBNjAFUphahGqEytx0sYD0rDgXqPBUE4\nrHlOMDYZEcY4TtpOpaqqui2gaaBGDw0BgbhvAounR6FQVX7+rQjsx7bWdOYVNbi5\nOhWrGJFDhD+PNVth3oock21AHppcXRL7A8tILiUITOm9dgsfqP1u3Re5AoGALuAp\nMPGDuEf+eG0IzJaoieXAF65OV8VzEvbJOQRZU7juKTK9fL4TcFyby0H+4kpeVPce\nrxLRb5DVdcgcdUCzt+xu1Cz2fwFjL7T4zotaYQkRPMuOx2GyKEOhGYcRAFqMOFPX\nxsf2YwViZ76DiAKfrPXmLP/xVY9Ew2djsQIPn2kCgYEAhhR9JAqyruqOjCobZsBy\n9NCOYY8g6qmfdwcKDPslXa43d24RYJXXk33b51EQLgHT7/1jB+Y87GgBHLNsLk4Q\njF1oOS7bOdeloMElpj8oqDTIScrC1lT6y/bE4Wn3Kv8nRBegLZCtHVRll8yejwn7\nAsDT6h7HYx8KBJmjJnRTBBo=\n-----END PRIVATE KEY-----\n";

const TEST_JWKS: &str = r#"{
  "keys": [
    {
      "kty": "RSA",
      "kid": "oidc-test-key",
      "alg": "RS256",
      "use": "sig",
      "n": "z4AqUeLL8eS2JMiujO4wBjbvArTO0ZatdqZ6INnj02T3Y_hPbX4WiBx0fR2YGlX2cn0RXHQOMgTejsjnYDe4UpQDRCXlIQUSHrZL8xjhKby4jxorBnNQ5GHgOG801D4s2y--2ps9kk2zKtXUjabb_bK-3QflUqRH1Feui_So75usdvGKsAw31cBPJsV8yEd3GTlxSUzP52IX91GfKGH6hi_CZgk6mov55DnKTO6o_Alq7V53kp_hUWDDP05xdxOLZUWE4PxVTqIOfvA3yI8PU4X_MvF21SiT0CbdFAsXP2uR2_-1E8Q5eTFTevAQhLy3WxyB4zFetW5970FMGFZLpQ",
      "e": "AQAB"
    }
  ]
}"#;

fn make_oidc_test_token(sub: &str, issuer: &str, audience: &str) -> String {
    use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};

    #[derive(serde::Serialize)]
    struct Claims {
        sub: String,
        exp: usize,
        iss: String,
        aud: String,
    }

    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some("oidc-test-key".to_string());

    encode(
        &header,
        &Claims {
            sub: sub.to_string(),
            exp: 9_999_999_999,
            iss: issuer.to_string(),
            aud: audience.to_string(),
        },
        &EncodingKey::from_rsa_pem(TEST_RSA_PRIVATE_KEY.as_bytes()).unwrap(),
    )
    .unwrap()
}

// ─── Health Endpoints ────────────────────────────────────────────────────────

#[tokio::test]
async fn healthz_returns_200_with_ok_status() {
    let state = build_test_state().await;
    let app = create_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = body_to_json(response).await;
    assert_eq!(body.get("status").unwrap(), "ok");
}

#[tokio::test]
async fn readyz_returns_200_when_db_is_connected() {
    let state = build_test_state().await;
    let app = create_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/readyz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = body_to_json(response).await;
    assert_eq!(body.get("status").unwrap(), "ready");
}

#[tokio::test]
async fn readyz_returns_503_when_dependencies_are_missing() {
    let mut state = build_test_state().await;
    state.clear_database_and_composition_for_test();
    let app = create_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/readyz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body: serde_json::Value = body_to_json(response).await;
    assert_eq!(body.get("status").unwrap(), "not_ready");
    let unavailable = body
        .get("unavailable")
        .and_then(|value| value.as_array())
        .expect("readyz should include unavailable dependency summary");
    assert!(unavailable.iter().any(|value| value == "database"));
    assert!(unavailable.iter().any(|value| value == "composition"));
}

#[tokio::test]
async fn response_includes_generated_request_id_when_missing() {
    let state = build_test_state().await;
    let app = create_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let request_id = response
        .headers()
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .expect("response should include x-request-id");
    let parsed = uuid::Uuid::parse_str(request_id).expect("request id should be a UUID");
    assert_eq!(parsed.get_version_num(), 7);
}

#[tokio::test]
async fn response_propagates_existing_request_id() {
    let state = build_test_state().await;
    let app = create_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .header("x-request-id", "req-123")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("x-request-id")
            .and_then(|value| value.to_str().ok()),
        Some("req-123")
    );
}

#[test]
fn generated_openapi_documents_counter_idempotency_and_error_statuses() {
    let yaml = openapi().to_yaml().unwrap();

    assert!(yaml.contains("openapi: 3.1.0"));
    assert!(yaml.contains("/api/counter/increment:"));
    assert!(yaml.contains("name: Idempotency-Key"));
    assert!(yaml.contains("'400':"));
    assert!(yaml.contains("'403':"));
    assert!(yaml.contains("'409':"));
    assert!(yaml.contains("/api/user/me:"));
}

// ─── 404 Fallback ────────────────────────────────────────────────────────────

#[tokio::test]
async fn unknown_route_returns_404() {
    let state = build_test_state().await;
    let app = create_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/nonexistent")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// ─── Tenant Middleware ───────────────────────────────────────────────────────

#[tokio::test]
async fn api_route_without_auth_returns_401() {
    let state = build_test_state().await;
    let app = create_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/tenant/init")
                .method(http::Method::POST)
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"user_sub":"test","user_name":"Test"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body: serde_json::Value = body_to_json(response).await;
    assert_eq!(body.get("code").unwrap(), "Unauthorized");
    assert_eq!(body.get("message").unwrap(), "Missing bearer token");
}

#[tokio::test]
async fn api_route_with_invalid_jwt_returns_401() {
    let state = build_test_state().await;
    let app = create_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/tenant/init")
                .method(http::Method::POST)
                .header(http::header::AUTHORIZATION, "Bearer not-a-jwt")
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"user_sub":"test","user_name":"Test"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body: serde_json::Value = body_to_json(response).await;
    assert_eq!(body.get("code").unwrap(), "Unauthorized");
    assert_eq!(body.get("message").unwrap(), "Invalid bearer token");
}

#[tokio::test]
async fn api_route_with_valid_jwt_and_missing_fields_returns_400() {
    let state = build_test_state().await;
    let app = create_router(state);
    let token = make_test_token("google-test-123");

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/tenant/init")
                .method(http::Method::POST)
                .header(http::header::AUTHORIZATION, format!("Bearer {token}"))
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn api_route_accepts_oidc_jwks_token() {
    let mut oidc = Server::new_async().await;
    let discovery_body = serde_json::json!({
        "jwks_uri": format!("{}/oauth/v2/keys", oidc.url()),
    });
    let _discovery = oidc
        .mock("GET", "/.well-known/openid-configuration")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(discovery_body.to_string())
        .create();
    let _jwks = oidc
        .mock("GET", "/oauth/v2/keys")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(TEST_JWKS)
        .create();

    let audience = "web-bff-local";
    let state = build_oidc_test_state(&oidc.url(), audience).await;
    let app = create_router(state);
    let token = make_oidc_test_token("oidc-user", &oidc.url(), audience);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/tenant/init")
                .method(http::Method::POST)
                .header(http::header::AUTHORIZATION, format!("Bearer {token}"))
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"user_sub":"oidc-user","user_name":"OIDC User"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn oidc_jwks_requests_are_cached_across_matching_kid_requests() {
    let mut oidc = Server::new_async().await;
    let discovery_body = serde_json::json!({
        "jwks_uri": format!("{}/oauth/v2/keys", oidc.url()),
    });
    let discovery = oidc
        .mock("GET", "/.well-known/openid-configuration")
        .expect(1)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(discovery_body.to_string())
        .create();
    let jwks = oidc
        .mock("GET", "/oauth/v2/keys")
        .expect(1)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(TEST_JWKS)
        .create();

    let audience = "web-bff-local";
    let state = build_oidc_test_state(&oidc.url(), audience).await;
    let app = create_router(state);
    let token_a = make_oidc_test_token("cached-oidc-user-a", &oidc.url(), audience);
    let token_b = make_oidc_test_token("cached-oidc-user-b", &oidc.url(), audience);

    for (user_sub, token) in [
        ("cached-oidc-user-a", token_a),
        ("cached-oidc-user-b", token_b),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/tenant/init")
                    .method(http::Method::POST)
                    .header(http::header::AUTHORIZATION, format!("Bearer {token}"))
                    .header(http::header::CONTENT_TYPE, "application/json")
                    .body(Body::from(format!(
                        r#"{{"user_sub":"{user_sub}","user_name":"Cached OIDC User"}}"#
                    )))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    discovery.assert();
    jwks.assert();
}

#[tokio::test]
async fn api_route_rejects_oidc_token_with_wrong_audience() {
    let mut oidc = Server::new_async().await;
    let discovery_body = serde_json::json!({
        "jwks_uri": format!("{}/oauth/v2/keys", oidc.url()),
    });
    let _discovery = oidc
        .mock("GET", "/.well-known/openid-configuration")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(discovery_body.to_string())
        .create();
    let _jwks = oidc
        .mock("GET", "/oauth/v2/keys")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(TEST_JWKS)
        .create();

    let state = build_oidc_test_state(&oidc.url(), "expected-aud").await;
    let app = create_router(state);
    let token = make_oidc_test_token("oidc-user", &oidc.url(), "wrong-aud");

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/tenant/init")
                .method(http::Method::POST)
                .header(http::header::AUTHORIZATION, format!("Bearer {token}"))
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"user_sub":"oidc-user","user_name":"OIDC User"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn api_route_accepts_opaque_token_via_explicit_oidc_introspection_url() {
    let mut oidc = Server::new_async().await;
    let _introspect = oidc
        .mock("POST", "/oauth/v2/introspect")
        .match_header("authorization", "Basic YXBpLWNsaWVudDphcGktc2VjcmV0")
        .match_body(mockito::Matcher::AllOf(vec![
            mockito::Matcher::UrlEncoded("token".into(), "opaque-token".into()),
            mockito::Matcher::UrlEncoded("token_type_hint".into(), "access_token".into()),
        ]))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            serde_json::json!({
                "active": true,
                "sub": "opaque-user",
                "iss": oidc.url(),
                "aud": ["web-bff-local"],
            })
            .to_string(),
        )
        .create();

    let state = build_introspection_test_state(
        &oidc.url(),
        "web-bff-local",
        &format!("{}/oauth/v2/introspect", oidc.url()),
        "api-client",
        "api-secret",
    )
    .await;
    let app = create_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/tenant/init")
                .method(http::Method::POST)
                .header(http::header::AUTHORIZATION, "Bearer opaque-token")
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"user_sub":"opaque-user","user_name":"Opaque User"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn api_route_accepts_opaque_token_via_discovered_rauthy_style_introspection() {
    let mut oidc = Server::new_async().await;
    let discovery_body = serde_json::json!({
        "jwks_uri": format!("{}/oidc/jwks", oidc.url()),
        "introspection_endpoint": format!("{}/oidc/introspect", oidc.url()),
    });
    let _discovery = oidc
        .mock("GET", "/.well-known/openid-configuration")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(discovery_body.to_string())
        .create();
    let _introspect = oidc
        .mock("POST", "/oidc/introspect")
        .match_header("authorization", "Basic YXBpLWNsaWVudDphcGktc2VjcmV0")
        .match_body(mockito::Matcher::AllOf(vec![
            mockito::Matcher::UrlEncoded("token".into(), "opaque-token".into()),
            mockito::Matcher::UrlEncoded("token_type_hint".into(), "access_token".into()),
        ]))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            serde_json::json!({
                "active": true,
                "sub": "opaque-user",
                "iss": oidc.url(),
                "aud": ["web-bff-local"],
            })
            .to_string(),
        )
        .create();

    let state = build_introspection_test_state(
        &oidc.url(),
        "web-bff-local",
        "",
        "api-client",
        "api-secret",
    )
    .await;
    let app = create_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/tenant/init")
                .method(http::Method::POST)
                .header(http::header::AUTHORIZATION, "Bearer opaque-token")
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"user_sub":"opaque-user","user_name":"Opaque User"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn api_route_accepts_dev_headers_without_bearer_token() {
    let state = build_dev_headers_state().await;
    let app = create_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/tenant/init")
                .method(http::Method::POST)
                .header("x-dev-user-sub", "dev-header-user")
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"user_sub":"dev-header-user","user_name":"Dev Header User"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn counter_endpoints_work_with_dev_headers_after_tenant_init() {
    let state = build_dev_headers_state().await;
    let app = create_router(state);

    let init_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/tenant/init")
                .method(http::Method::POST)
                .header("x-dev-user-sub", "dev-counter-user")
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"user_sub":"dev-counter-user","user_name":"Dev Counter User"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(init_response.status(), StatusCode::OK);

    let increment_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/counter/increment")
                .method(http::Method::POST)
                .header("x-dev-user-sub", "dev-counter-user")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(increment_response.status(), StatusCode::OK);

    let value_response = app
        .oneshot(
            Request::builder()
                .uri("/api/counter/value")
                .method(http::Method::GET)
                .header("x-dev-user-sub", "dev-counter-user")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(value_response.status(), StatusCode::OK);
    let value_body: serde_json::Value = body_to_json(value_response).await;
    assert_eq!(value_body.get("value").unwrap(), 1);
}

#[tokio::test]
async fn user_me_requires_authenticated_request_context() {
    let state = build_test_state().await;
    let app = create_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/user/me")
                .method(http::Method::GET)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body: serde_json::Value = body_to_json(response).await;
    assert_eq!(body.get("code").unwrap(), "Unauthorized");
    assert_eq!(body.get("message").unwrap(), "Missing bearer token");
}

#[tokio::test]
async fn user_me_returns_not_found_when_profile_does_not_exist() {
    let state = build_dev_headers_state().await;
    let app = create_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/user/me")
                .method(http::Method::GET)
                .header("x-dev-user-sub", "missing-profile-user")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body: serde_json::Value = body_to_json(response).await;
    assert_eq!(body.get("code").unwrap(), "NotFound");
    assert_eq!(body.get("message").unwrap(), "User not found");
}

#[tokio::test]
async fn user_me_returns_profile_when_user_exists() {
    let state = build_dev_headers_state().await;
    create_user_profile(
        &state,
        "existing-profile-user",
        "Existing Profile User",
        "existing@example.com",
    )
    .await;
    let app = create_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/user/me")
                .method(http::Method::GET)
                .header("x-dev-user-sub", "existing-profile-user")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = body_to_json(response).await;
    assert_eq!(body.get("user_sub").unwrap(), "existing-profile-user");
    assert_eq!(body.get("display_name").unwrap(), "Existing Profile User");
    assert_eq!(body.get("email").unwrap(), "existing@example.com");
    assert!(body.get("created_at").is_some());
    assert!(body.get("last_login_at").is_some());
}

#[tokio::test]
async fn user_tenants_returns_empty_array_without_binding() {
    let state = build_dev_headers_state().await;
    let app = create_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/user/tenants")
                .method(http::Method::GET)
                .header("x-dev-user-sub", "no-binding-user")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = body_to_json(response).await;
    assert_eq!(body, serde_json::json!([]));
}

#[tokio::test]
async fn user_tenants_returns_binding_after_tenant_init() {
    let state = build_dev_headers_state().await;
    let app = create_router(state);

    let init_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/tenant/init")
                .method(http::Method::POST)
                .header("x-dev-user-sub", "tenant-reader-user")
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"user_sub":"tenant-reader-user","user_name":"Tenant Reader User"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(init_response.status(), StatusCode::OK);
    let init_body: serde_json::Value = body_to_json(init_response).await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/user/tenants")
                .method(http::Method::GET)
                .header("x-dev-user-sub", "tenant-reader-user")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = body_to_json(response).await;
    let items = body.as_array().expect("expected tenants array");
    assert_eq!(items.len(), 1);
    assert_eq!(
        items[0].get("tenant_id").unwrap(),
        init_body.get("tenant_id").unwrap()
    );
    assert_eq!(items[0].get("tenant_name").unwrap(), "Tenant Reader User");
    assert_eq!(items[0].get("role").unwrap(), "owner");
    assert!(items[0].get("joined_at").is_some());
}

#[tokio::test]
async fn tenant_init_creates_tenant_on_first_call() {
    let state = build_test_state().await;
    let app = create_router(state);
    let token = make_test_token("google-e2e-first");

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/tenant/init")
                .method(http::Method::POST)
                .header(http::header::AUTHORIZATION, format!("Bearer {token}"))
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"user_sub":"google-e2e-first","user_name":"Alice"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    let status = response.status();
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8_lossy(&body_bytes);

    assert_eq!(
        status,
        StatusCode::OK,
        "Expected 200 OK but got {}: {body_str}",
        status
    );

    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body.get("role").unwrap(), "owner");
    assert_eq!(body.get("created").unwrap(), true);
}

#[tokio::test]
async fn tenant_init_returns_existing_on_second_call() {
    let state = build_test_state().await;
    let app = create_router(state);
    let token = make_test_token("google-e2e-second");

    // First call — creates tenant
    let resp1 = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/tenant/init")
                .method(http::Method::POST)
                .header(http::header::AUTHORIZATION, format!("Bearer {token}"))
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"user_sub":"google-e2e-second","user_name":"Bob"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp1.status(), StatusCode::OK);
    let body1: serde_json::Value = body_to_json(resp1).await;
    let tenant_id_1 = body1.get("tenant_id").unwrap().clone();
    assert_eq!(body1.get("created").unwrap(), true);

    // Second call — returns existing
    let resp2 = app
        .oneshot(
            Request::builder()
                .uri("/api/tenant/init")
                .method(http::Method::POST)
                .header(http::header::AUTHORIZATION, format!("Bearer {token}"))
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"user_sub":"google-e2e-second","user_name":"Bob"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp2.status(), StatusCode::OK);
    let body2: serde_json::Value = body_to_json(resp2).await;
    assert_eq!(body2.get("tenant_id").unwrap(), &tenant_id_1);
    assert_eq!(body2.get("created").unwrap(), false);
}

#[tokio::test]
async fn tenant_init_rejects_jwt_sub_mismatch() {
    let state = build_test_state().await;
    let app = create_router(state);
    let token = make_test_token("jwt-user");

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/tenant/init")
                .method(http::Method::POST)
                .header(http::header::AUTHORIZATION, format!("Bearer {token}"))
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"user_sub":"different-user","user_name":"Mallory"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body: serde_json::Value = body_to_json(response).await;
    assert_eq!(body.get("code").unwrap(), "Unauthorized");
    assert_eq!(
        body.get("message").unwrap(),
        "JWT subject does not match requested user_sub"
    );
}

// ─── Cross-Tenant Isolation ──────────────────────────────────────────────────

#[tokio::test]
async fn two_users_get_different_tenants() {
    let state = build_test_state().await;
    let app = create_router(state);

    let token_a = make_test_token("user-alice");
    let token_b = make_test_token("user-bob");

    // Alice creates her tenant
    let resp_a = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/tenant/init")
                .method(http::Method::POST)
                .header(http::header::AUTHORIZATION, format!("Bearer {token_a}"))
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"user_sub":"user-alice","user_name":"Alice"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    let body_a: serde_json::Value = body_to_json(resp_a).await;
    let tenant_a = body_a.get("tenant_id").unwrap().as_str().unwrap();

    // Bob creates his tenant
    let resp_b = app
        .oneshot(
            Request::builder()
                .uri("/api/tenant/init")
                .method(http::Method::POST)
                .header(http::header::AUTHORIZATION, format!("Bearer {token_b}"))
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"user_sub":"user-bob","user_name":"Bob"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    let body_b: serde_json::Value = body_to_json(resp_b).await;
    let tenant_b = body_b.get("tenant_id").unwrap().as_str().unwrap();

    assert_ne!(
        tenant_a, tenant_b,
        "Alice and Bob should have different tenant IDs"
    );
}

#[tokio::test]
async fn counter_endpoints_require_existing_user_tenant_binding() {
    let state = build_test_state().await;
    let app = create_router(state);
    let token = make_test_token("counter-user");

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/counter/value")
                .method(http::Method::GET)
                .header(http::header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let status = response.status();
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8_lossy(&body_bytes);
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "Expected 401 but got {}: {body_str}",
        status
    );
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body.get("code").unwrap(), "Unauthorized");
    assert_eq!(
        body.get("message").unwrap(),
        "No tenant binding found for authenticated user"
    );
}

#[tokio::test]
async fn counter_endpoints_resolve_real_tenant_binding_after_init() {
    let state = build_test_state().await;
    let app = create_router(state);
    let token = make_test_token("bound-user");

    let init_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/tenant/init")
                .method(http::Method::POST)
                .header(http::header::AUTHORIZATION, format!("Bearer {token}"))
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"user_sub":"bound-user","user_name":"Bound User"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(init_response.status(), StatusCode::OK);

    let increment_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/counter/increment")
                .method(http::Method::POST)
                .header(http::header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let increment_status = increment_response.status();
    let increment_bytes = increment_response
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes();
    let increment_body_str = String::from_utf8_lossy(&increment_bytes);
    assert_eq!(
        increment_status,
        StatusCode::OK,
        "Expected 200 but got {}: {increment_body_str}",
        increment_status
    );
    let increment_body: serde_json::Value = serde_json::from_slice(&increment_bytes).unwrap();
    assert_eq!(increment_body.get("value").unwrap(), 1);

    let value_response = app
        .oneshot(
            Request::builder()
                .uri("/api/counter/value")
                .method(http::Method::GET)
                .header(http::header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(value_response.status(), StatusCode::OK);
    let value_body: serde_json::Value = body_to_json(value_response).await;
    assert_eq!(value_body.get("value").unwrap(), 1);
}

#[tokio::test]
async fn counter_increment_replays_same_idempotency_key() {
    let state = build_test_state().await;
    let app = create_router(state);
    let token = make_test_token("http-idem-user");

    let init_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/tenant/init")
                .method(http::Method::POST)
                .header(http::header::AUTHORIZATION, format!("Bearer {token}"))
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"user_sub":"http-idem-user","user_name":"HTTP Idem User"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(init_response.status(), StatusCode::OK);

    for _ in 0..2 {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/counter/increment")
                    .method(http::Method::POST)
                    .header(http::header::AUTHORIZATION, format!("Bearer {token}"))
                    .header("Idempotency-Key", "http-idem-key-1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value = body_to_json(response).await;
        assert_eq!(body.get("value").unwrap(), 1);
    }

    let value_response = app
        .oneshot(
            Request::builder()
                .uri("/api/counter/value")
                .method(http::Method::GET)
                .header(http::header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(value_response.status(), StatusCode::OK);
    let value_body: serde_json::Value = body_to_json(value_response).await;
    assert_eq!(value_body.get("value").unwrap(), 1);
}

#[tokio::test]
async fn counter_mutation_audit_redacts_idempotency_key() {
    let state = build_test_state().await;
    let app = create_router(state.clone());
    let token = make_test_token("audit-counter-user");

    let init_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/tenant/init")
                .method(http::Method::POST)
                .header(http::header::AUTHORIZATION, format!("Bearer {token}"))
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"user_sub":"audit-counter-user","user_name":"Audit Counter User"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(init_response.status(), StatusCode::OK);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/counter/increment")
                .method(http::Method::POST)
                .header(http::header::AUTHORIZATION, format!("Bearer {token}"))
                .header("Idempotency-Key", "raw-secret-idem-key")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let events = state.audit_events().await;
    let event = events
        .iter()
        .find(|event| event.action == "counter.increment")
        .expect("counter increment should append an audit event");
    assert_eq!(event.outcome, AuditOutcome::Succeeded);
    let rendered = serde_json::to_string(&event.metadata).unwrap();
    assert!(rendered.contains("[redacted]"));
    assert!(!rendered.contains("raw-secret-idem-key"));
}

#[tokio::test]
async fn counter_endpoints_continue_to_work_after_repeated_tenant_init() {
    let state = build_test_state().await;
    let app = create_router(state);
    let token = make_test_token("repeat-init-user");

    for _ in 0..2 {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/tenant/init")
                    .method(http::Method::POST)
                    .header(http::header::AUTHORIZATION, format!("Bearer {token}"))
                    .header(http::header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        r#"{"user_sub":"repeat-init-user","user_name":"Repeat Init"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    let increment_response = app
        .oneshot(
            Request::builder()
                .uri("/api/counter/increment")
                .method(http::Method::POST)
                .header(http::header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(increment_response.status(), StatusCode::OK);
}

// ─── Request Validation ──────────────────────────────────────────────────────

#[tokio::test]
async fn tenant_init_rejects_empty_user_sub() {
    let state = build_test_state().await;
    let app = create_router(state);
    let token = make_test_token("google-test");

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/tenant/init")
                .method(http::Method::POST)
                .header(http::header::AUTHORIZATION, format!("Bearer {token}"))
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"user_sub":"","user_name":"Test"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    // Now returns 422 (Unprocessable Entity) due to validator
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body: serde_json::Value = body_to_json(response).await;
    assert_eq!(body.get("code").unwrap(), "ValidationError");
    assert!(
        body.get("message")
            .unwrap()
            .as_str()
            .unwrap()
            .contains("user_sub")
    );
}

#[tokio::test]
async fn tenant_init_rejects_empty_user_name() {
    let state = build_test_state().await;
    let app = create_router(state);
    let token = make_test_token("google-test");

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/tenant/init")
                .method(http::Method::POST)
                .header(http::header::AUTHORIZATION, format!("Bearer {token}"))
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"user_sub":"test","user_name":""}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    // Now returns 422 (Unprocessable Entity) due to validator
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body: serde_json::Value = body_to_json(response).await;
    assert_eq!(body.get("code").unwrap(), "ValidationError");
    assert!(
        body.get("message")
            .unwrap()
            .as_str()
            .unwrap()
            .contains("user_name")
    );
}

#[tokio::test]
async fn tenant_init_rejects_missing_content_type() {
    let state = build_test_state().await;
    let app = create_router(state);
    let token = make_test_token("google-test");

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/tenant/init")
                .method(http::Method::POST)
                .header(http::header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::from(r#"{"user_sub":"test","user_name":"Test"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    let body: serde_json::Value = body_to_json(response).await;
    assert_eq!(body.get("code").unwrap(), "BadRequest");
    assert_eq!(
        body.get("message").unwrap(),
        "Expected content-type: application/json"
    );
}

#[tokio::test]
async fn counter_endpoint_rejects_tenant_claim_mismatch_with_forbidden_contract() {
    let state = build_dev_headers_state().await;
    let app = create_router(state.clone());

    let init_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/tenant/init")
                .method(http::Method::POST)
                .header("x-dev-user-sub", "claim-mismatch-user")
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"user_sub":"claim-mismatch-user","user_name":"Claim Mismatch"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(init_response.status(), StatusCode::OK);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/counter/value")
                .method(http::Method::GET)
                .header("x-dev-user-sub", "claim-mismatch-user")
                .header("x-dev-tenant-id", "tenant-other")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body: serde_json::Value = body_to_json(response).await;
    assert_eq!(body.get("code").unwrap(), "Forbidden");
    assert_eq!(
        body.get("message").unwrap(),
        "Tenant claim does not match authenticated user binding"
    );

    let events = state.audit_events().await;
    assert!(events.iter().any(|event| {
        event.action == "tenant.resolve"
            && event.outcome == AuditOutcome::Denied
            && event.metadata["reason"] == "claim_mismatch"
    }));
}

// ─── CORS Headers ────────────────────────────────────────────────────────────

#[tokio::test]
async fn healthz_includes_cors_headers_on_preflight() {
    let mut state = build_test_state().await;
    state.config_mut().cors_allowed_origins = vec!["http://localhost:5173".to_string()];
    let app = create_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .method(http::Method::OPTIONS)
                .header(http::header::ORIGIN, "http://localhost:5173")
                .header(http::header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
                .header(
                    http::header::ACCESS_CONTROL_REQUEST_HEADERS,
                    "Idempotency-Key",
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let allowed_headers = response
        .headers()
        .get(http::header::ACCESS_CONTROL_ALLOW_HEADERS)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_ascii_lowercase();
    assert!(allowed_headers.contains("idempotency-key"));
}

// ─── Middleware + DB Integration ─────────────────────────────────────────────

#[tokio::test]
async fn tenant_init_with_sql_injection_attempt_in_user_sub() {
    let state = build_test_state().await;
    let app = create_router(state);
    let token = make_test_token("attacker");

    // Attempt to spoof user_sub in request body
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/tenant/init")
                .method(http::Method::POST)
                .header(http::header::AUTHORIZATION, format!("Bearer {token}"))
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"user_sub":"'; DROP TABLE tenant; --","user_name":"Attacker"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    // JWT subject is now the source of truth, so body spoofing is rejected.
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    // Verify the tenant table still exists and the legitimate request remains queryable.
    let response2 = app
        .oneshot(
            Request::builder()
                .uri("/api/tenant/init")
                .method(http::Method::POST)
                .header(http::header::AUTHORIZATION, format!("Bearer {token}"))
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"user_sub":"attacker","user_name":"Attacker"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response2.status(), StatusCode::OK);
    let body2: serde_json::Value = body_to_json(response2).await;
    assert_eq!(body2.get("created").unwrap(), true);
}

#[tokio::test]
async fn tenant_init_with_special_characters_in_user_name() {
    let state = build_test_state().await;
    let app = create_router(state);
    let token = make_test_token("special-chars-user");

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/tenant/init")
                .method(http::Method::POST)
                .header(http::header::AUTHORIZATION, format!("Bearer {token}"))
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"user_sub":"special-chars-user","user_name":"O'Brien <script>alert(1)</script>"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = body_to_json(response).await;
    assert_eq!(body.get("created").unwrap(), true);
}

// ─── Error Path Tests ────────────────────────────────────────────────────────

#[tokio::test]
async fn tenant_init_with_malformed_json_body() {
    let state = build_test_state().await;
    let app = create_router(state);
    let token = make_test_token("malformed-json-user");

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/tenant/init")
                .method(http::Method::POST)
                .header(http::header::AUTHORIZATION, format!("Bearer {token}"))
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"user_sub": "broken"#))
                .unwrap(),
        )
        .await
        .unwrap();

    // ContractJson maps malformed JSON into the public error contract.
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body: serde_json::Value = body_to_json(response).await;
    assert_eq!(body.get("code").unwrap(), "BadRequest");
    assert_eq!(body.get("message").unwrap(), "Malformed JSON request body");
}

#[tokio::test]
async fn tenant_init_rejects_body_over_application_limit() {
    let state = build_test_state().await;
    let app = create_router(state);
    let token = make_test_token("oversized-body-user");
    let oversized_name = "x".repeat(1024 * 1024);
    let body = serde_json::json!({
        "user_sub": "oversized-body-user",
        "user_name": oversized_name,
    })
    .to_string();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/tenant/init")
                .method(http::Method::POST)
                .header(http::header::AUTHORIZATION, format!("Bearer {token}"))
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    let body: serde_json::Value = body_to_json(response).await;
    assert_eq!(body.get("code").unwrap(), "RateLimited");
    assert_eq!(body.get("message").unwrap(), "Request body too large");
}

#[tokio::test]
async fn tenant_init_with_jwt_missing_sub_claim() {
    use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};

    #[derive(serde::Serialize)]
    struct NoSubClaims {
        name: String,
        exp: usize,
    }
    let no_sub_token = encode(
        &Header::new(Algorithm::HS256),
        &NoSubClaims {
            name: "no-sub".into(),
            exp: 9999999999,
        },
        &EncodingKey::from_secret(b"test-secret"),
    )
    .unwrap();

    let state = build_test_state().await;
    let app = create_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/tenant/init")
                .method(http::Method::POST)
                .header(
                    http::header::AUTHORIZATION,
                    format!("Bearer {no_sub_token}"),
                )
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"user_sub":"no-sub","user_name":"NoSub"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    // Missing sub claim should cause middleware to fail
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn tenant_init_idempotent_with_same_token() {
    let state = build_test_state().await;
    let app = create_router(state);
    let token = make_test_token("idempotent-user");

    // First call — creates
    let resp1 = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/tenant/init")
                .method(http::Method::POST)
                .header(http::header::AUTHORIZATION, format!("Bearer {token}"))
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"user_sub":"idempotent-user","user_name":"Idempotent"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp1.status(), StatusCode::OK);
    let body1: serde_json::Value = body_to_json(resp1).await;
    let tenant_id_1 = body1.get("tenant_id").unwrap().clone();
    assert_eq!(body1.get("created").unwrap(), true);

    // Second call with same token — returns existing
    let resp2 = app
        .oneshot(
            Request::builder()
                .uri("/api/tenant/init")
                .method(http::Method::POST)
                .header(http::header::AUTHORIZATION, format!("Bearer {token}"))
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"user_sub":"idempotent-user","user_name":"Idempotent"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp2.status(), StatusCode::OK);
    let body2: serde_json::Value = body_to_json(resp2).await;
    assert_eq!(body2.get("tenant_id").unwrap(), &tenant_id_1);
    assert_eq!(body2.get("created").unwrap(), false);
}
