use axum::{Json, body::Bytes, extract::State, http::HeaderMap};
use contracts_errors::ErrorResponse;
use serde::Serialize;
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::billing::WebhookOutcome;
use crate::error::{BffError, BffResult};
use crate::request_context::RequestContext;
use crate::state::BffState;
use crate::tenant_context::resolve_tenant_id;

pub fn authenticated_openapi_router() -> OpenApiRouter<BffState> {
    OpenApiRouter::new().routes(routes!(create_creem_counter_checkout))
}

pub fn public_openapi_router() -> OpenApiRouter<BffState> {
    OpenApiRouter::new().routes(routes!(creem_webhook))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CheckoutResponseBody {
    checkout_url: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct WebhookResponseBody {
    received: bool,
    duplicate: bool,
}

#[utoipa::path(
    post,
    path = "/api/billing/checkout/creem/counter-pro",
    tag = "billing",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Creem checkout session created", body = CheckoutResponseBody, content_type = "application/json"),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 404, description = "Creem billing provider not enabled", body = ErrorResponse),
        (status = 500, description = "Creem API or local dependency failure", body = ErrorResponse),
    ),
)]
pub async fn create_creem_counter_checkout(
    State(state): State<BffState>,
    axum::extract::Extension(request_context): axum::extract::Extension<RequestContext>,
) -> BffResult<Json<CheckoutResponseBody>> {
    let tenant_id = resolve_tenant_id(&state, &request_context).await?;
    let checkout = state
        .billing()
        .creem()?
        .create_counter_checkout(
            state.http_client(),
            tenant_id.as_str(),
            request_context.user_sub(),
        )
        .await?;

    Ok(Json(CheckoutResponseBody {
        checkout_url: checkout.checkout_url,
    }))
}

#[utoipa::path(
    post,
    path = "/api/billing/webhooks/creem",
    tag = "billing",
    request_body(content = String, content_type = "application/json"),
    responses(
        (status = 200, description = "Creem webhook accepted", body = WebhookResponseBody, content_type = "application/json"),
        (status = 400, description = "Malformed webhook payload", body = ErrorResponse),
        (status = 401, description = "Invalid Creem webhook signature", body = ErrorResponse),
        (status = 404, description = "Creem billing provider not enabled", body = ErrorResponse),
        (status = 500, description = "Webhook ledger or projection failure", body = ErrorResponse),
    ),
)]
pub async fn creem_webhook(
    State(state): State<BffState>,
    headers: HeaderMap,
    body: Bytes,
) -> BffResult<Json<WebhookResponseBody>> {
    let signature = headers
        .get("creem-signature")
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| BffError::Unauthorized("Missing Creem webhook signature".to_string()))?;

    let outcome = state
        .billing()
        .creem()?
        .handle_webhook(signature, &body)
        .await?;

    Ok(Json(WebhookResponseBody {
        received: true,
        duplicate: outcome == WebhookOutcome::Duplicate,
    }))
}
