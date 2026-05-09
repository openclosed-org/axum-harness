//! Tenant initialization handlers — migrated to web-bff.
//!
//! POST /api/tenant/init — ensure tenant exists for user (auto-create on first login).

use axum::{Json, extract::State};
use contracts_errors::ErrorResponse;
use utoipa_axum::{router::OpenApiRouter, routes};

use contracts_api::{InitTenantRequest, InitTenantResponse};
use tenant_service::application::TenantServiceTrait;
use validator::Validate;

use crate::error::{BffError, BffResult};
use crate::http::ContractJson;
use crate::request_context::RequestContext;
use crate::state::BffState;

pub fn openapi_router() -> OpenApiRouter<BffState> {
    OpenApiRouter::new().routes(routes!(init_tenant))
}

/// POST /api/tenant/init
///
/// Ensures a tenant exists for the given user_sub.
/// - First login: creates tenant + user_tenant (role: 'owner')
/// - Subsequent login: returns existing tenant_id
#[utoipa::path(
    post,
    path = "/api/tenant/init",
    tag = "tenant",
    request_body = InitTenantRequest,
    responses(
        (status = 200, description = "Tenant initialized successfully", body = InitTenantResponse, content_type = "application/json"),
        (status = 400, description = "Bad request — malformed JSON payload", body = ErrorResponse),
        (status = 401, description = "Unauthorized — missing or invalid JWT", body = ErrorResponse),
        (status = 415, description = "Unsupported media type — expected application/json", body = ErrorResponse),
        (status = 422, description = "Unprocessable Entity — invalid request body", body = ErrorResponse),
        (status = 500, description = "Internal server error — database failure", body = ErrorResponse),
    ),
)]
pub async fn init_tenant(
    State(state): State<BffState>,
    axum::extract::Extension(request_context): axum::extract::Extension<RequestContext>,
    ContractJson(body): ContractJson<InitTenantRequest>,
) -> BffResult<Json<InitTenantResponse>> {
    body.validate()
        .map_err(|e| BffError::Validation(e.to_string()))?;

    if body.user_sub != request_context.user_sub() {
        return Err(BffError::Unauthorized(
            "JWT subject does not match requested user_sub".to_string(),
        ));
    }

    let service = state
        .tenant_service()
        .ok_or_else(|| BffError::Internal("Database not initialized".to_string()))?;
    let result = service
        .init_tenant_for_user(request_context.user_sub(), &body.user_name)
        .await
        .map_err(|e| BffError::Internal(format!("Failed to initialize tenant: {}", e)))?;

    state
        .seed_dev_counter_authz(request_context.user_sub(), &result.tenant_id, &result.role)
        .await
        .map_err(|e| BffError::Internal(format!("Failed to seed authz tuples: {e}")))?;

    Ok(Json(InitTenantResponse::new(
        result.tenant_id,
        result.role,
        result.created,
    )))
}
