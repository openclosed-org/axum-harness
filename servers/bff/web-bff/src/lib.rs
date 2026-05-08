//! Web BFF — Backend For Frontend (Web 端)
//!
//! Phase 0: 路由和中间件已就位，tenant/counter 为当前活跃 reference。

#![deny(unused_imports, unused_variables)]

pub mod application;
pub mod audit;
pub mod authz;
pub mod bootstrap;
pub mod composition;
pub mod config;
pub mod error;
pub mod handlers;
pub mod http;
pub mod middleware;
pub mod request_context;
pub mod state;
pub mod tenant_context;

use axum::{Json, Router, extract::DefaultBodyLimit, response::IntoResponse};
use state::BffState;
use std::time::Duration;
use tower_http::{
    cors::{AllowOrigin, CorsLayer},
    request_id::{MakeRequestId, PropagateRequestIdLayer, RequestId, SetRequestIdLayer},
    timeout::TimeoutLayer,
    trace::TraceLayer,
};
use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::openapi::{Info, OpenApi as OpenApiSpec};
use utoipa::{Modify, OpenApi};
use utoipa_axum::router::OpenApiRouter;
use utoipa_scalar::Scalar;

struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            "bearer_auth",
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .bearer_format("JWT")
                    .description(Some(
                        "Bearer JWT used to build authenticated request context",
                    ))
                    .build(),
            ),
        );
    }
}

/// Unified OpenAPI documentation for web-bff.
#[derive(OpenApi)]
#[openapi(
    modifiers(&SecurityAddon),
    tags(
        (name = "health", description = "Service health and readiness probes"),
        (name = "tenant", description = "Tenant bootstrap endpoints"),
        (name = "counter", description = "Tenant-scoped counter endpoints"),
        (name = "user", description = "Authenticated user read endpoints")
    )
)]
pub struct ApiDoc;

/// 生成 UUID v7 请求 ID。
#[derive(Clone)]
struct MakeRequestUuidV7;

impl MakeRequestId for MakeRequestUuidV7 {
    fn make_request_id<B>(&mut self, _request: &axum::http::Request<B>) -> Option<RequestId> {
        let id = uuid::Uuid::now_v7().to_string();
        Some(RequestId::new(axum::http::HeaderValue::from_str(&id).ok()?))
    }
}

pub fn openapi() -> OpenApiSpec {
    let mut router = openapi_router();
    router.to_openapi()
}

fn openapi_router() -> OpenApiRouter<BffState> {
    let mut api_doc = ApiDoc::openapi();
    api_doc.info = Info::new("web-bff", env!("CARGO_PKG_VERSION"));

    let api_routes = OpenApiRouter::new()
        .merge(handlers::tenant::openapi_router())
        .merge(handlers::counter::openapi_router())
        .merge(handlers::user::openapi_router())
        .route_layer(axum::middleware::from_fn(
            middleware::auth_context::auth_context_middleware,
        ));

    OpenApiRouter::with_openapi(api_doc)
        .merge(handlers::health::openapi_router())
        .merge(api_routes)
}

/// 构建路由器。
pub fn create_router(state: BffState) -> Router {
    let cors = build_cors_layer(&state.config().cors_allowed_origins);
    let config = state.config().clone();
    let oidc_verifier = state.oidc_verifier();
    let (router, openapi) = openapi_router()
        .layer(axum::Extension(config))
        .split_for_parts();

    let scalar_html: String = Scalar::new(openapi.clone()).to_html();
    let openapi_json = openapi.clone();
    let openapi_yaml = openapi
        .to_yaml()
        .expect("OpenAPI YAML serialization must succeed");

    let router = router
        .route(
            "/scalar",
            axum::routing::get(move || {
                let html = scalar_html.clone();
                async move { axum::response::Html(html) }
            }),
        )
        .route(
            "/openapi.json",
            axum::routing::get(move || {
                let openapi = openapi_json.clone();
                async move { Json(openapi) }
            }),
        )
        .route(
            "/openapi.yaml",
            axum::routing::get(move || {
                let yaml = openapi_yaml.clone();
                async move {
                    (
                        [(axum::http::header::CONTENT_TYPE, "application/yaml")],
                        yaml,
                    )
                        .into_response()
                }
            }),
        )
        .with_state(state)
        .layer(DefaultBodyLimit::max(1024 * 1024))
        .layer(cors)
        .layer(PropagateRequestIdLayer::new(
            axum::http::HeaderName::from_static("x-request-id"),
        ))
        .layer(SetRequestIdLayer::new(
            axum::http::HeaderName::from_static("x-request-id"),
            MakeRequestUuidV7,
        ))
        .layer(
            TraceLayer::new_for_http().make_span_with(|req: &axum::http::Request<_>| {
                let request_id = req
                    .headers()
                    .get("x-request-id")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("-");
                tracing::info_span!(
                    "http_request",
                    method = %req.method(),
                    uri = %req.uri(),
                    request_id,
                )
            }),
        )
        .layer(TimeoutLayer::with_status_code(
            axum::http::StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(30),
        ));

    if let Some(oidc_verifier) = oidc_verifier {
        router.layer(axum::Extension(oidc_verifier))
    } else {
        router
    }
}

/// CORS 层构建。
fn build_cors_layer(allowed_origins: &[String]) -> CorsLayer {
    if allowed_origins.is_empty() {
        return CorsLayer::permissive();
    }
    let origins: Vec<axum::http::HeaderValue> = allowed_origins
        .iter()
        .filter_map(|o| o.parse().ok())
        .collect();
    CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods([
            axum::http::Method::GET,
            axum::http::Method::POST,
            axum::http::Method::PUT,
            axum::http::Method::DELETE,
        ])
        .allow_headers([
            axum::http::header::AUTHORIZATION,
            axum::http::header::CONTENT_TYPE,
            axum::http::header::ACCEPT,
            axum::http::HeaderName::from_static("idempotency-key"),
            axum::http::HeaderName::from_static("x-request-id"),
        ])
        .allow_credentials(true)
}
