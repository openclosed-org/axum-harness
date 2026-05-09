//! User REST API handlers — web-bff version.
//!
//! GET    /api/user/me       — get current user profile
//! GET    /api/user/tenants  — list user's tenant bindings

use axum::{
    Json,
    extract::{Extension, State},
};
use contracts_api::{UserProfileResponse, UserTenantResponse};
use contracts_errors::ErrorResponse;
use user_service::ports::{TenantRepository, UserRepository, UserTenantRepository};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::error::{BffError, BffResult};
use crate::request_context::RequestContext;
use crate::state::BffState;

pub fn openapi_router() -> OpenApiRouter<BffState> {
    OpenApiRouter::new()
        .routes(routes!(get_user_profile))
        .routes(routes!(get_user_tenants))
}

/// Get current user profile.
#[utoipa::path(
    get,
    path = "/api/user/me",
    tag = "user",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "User profile retrieved", body = UserProfileResponse, content_type = "application/json"),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 404, description = "User not found", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse),
    ),
)]
pub async fn get_user_profile(
    State(state): State<BffState>,
    request_context: Option<Extension<RequestContext>>,
) -> BffResult<Json<UserProfileResponse>> {
    let user_sub = extract_user_sub(request_context)?;

    let user_repo = state.user_profile_repository().ok_or_else(db_not_ready)?;
    let result = user_repo.find_by_sub(&user_sub).await;

    match result {
        Ok(Some(user)) => Ok(Json(UserProfileResponse {
            id: user.id,
            user_sub: user.user_sub,
            display_name: user.display_name,
            email: user.email,
            created_at: user.created_at.to_rfc3339(),
            last_login_at: user.last_login_at.map(|dt| dt.to_rfc3339()),
        })),
        Ok(None) => Err(BffError::NotFound("User not found".to_string())),
        Err(e) => {
            tracing::warn!(error = %e, "failed to load user profile");
            Err(BffError::Internal(
                "Failed to load user profile".to_string(),
            ))
        }
    }
}

/// List user's tenant bindings.
#[utoipa::path(
    get,
    path = "/api/user/tenants",
    tag = "user",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "User tenants retrieved", body = Vec<UserTenantResponse>, content_type = "application/json"),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse),
    ),
)]
pub async fn get_user_tenants(
    State(state): State<BffState>,
    request_context: Option<Extension<RequestContext>>,
) -> BffResult<Json<Vec<UserTenantResponse>>> {
    let user_sub = extract_user_sub(request_context)?;

    let (binding_repo, tenant_repo) = state.user_read_repositories().ok_or_else(db_not_ready)?;

    match binding_repo.find_user_tenant(&user_sub).await {
        Ok(Some(binding)) => match tenant_repo.find_by_id(&binding.tenant_id).await {
            Ok(Some(tenant_info)) => Ok(Json(vec![UserTenantResponse {
                tenant_id: tenant_info.id,
                tenant_name: Some(tenant_info.name),
                role: binding.role,
                joined_at: binding.joined_at.to_rfc3339(),
            }])),
            Ok(None) => Ok(Json(vec![UserTenantResponse {
                tenant_id: binding.tenant_id,
                tenant_name: None,
                role: binding.role,
                joined_at: binding.joined_at.to_rfc3339(),
            }])),
            Err(e) => {
                tracing::warn!(error = %e, "failed to load tenant details");
                Err(BffError::Internal(
                    "Failed to load tenant details".to_string(),
                ))
            }
        },
        Ok(None) => Ok(Json(Vec::new())),
        Err(e) => {
            tracing::warn!(error = %e, "failed to load user tenant bindings");
            Err(BffError::Internal(
                "Failed to load user tenant bindings".to_string(),
            ))
        }
    }
}

// ── Helpers ──────────────────────────────────────────────────

fn db_not_ready() -> BffError {
    BffError::Internal("Embedded database not initialized".to_string())
}

fn extract_user_sub(request_context: Option<Extension<RequestContext>>) -> BffResult<String> {
    request_context
        .map(|Extension(context)| context.user_sub().to_string())
        .ok_or_else(|| BffError::Unauthorized("Missing authenticated request context".to_string()))
}
