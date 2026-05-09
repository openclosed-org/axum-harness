# Security Status

This document tracks security-sensitive capability status. It does not replace tests or gates.

## Status Summary

| Area | Status | Evidence Level | Evidence | Gap |
| --- | --- | --- | --- | --- |
| Dev headers rejected in production | implemented | tested | `servers/bff/web-bff/src/config.rs` tests | none known for config gate |
| Default JWT secret rejected in production without OIDC | implemented | tested | `production_rejects_default_jwt_secret_without_oidc_issuer` | none known for config gate |
| Missing authz endpoint rejected in production | implemented | tested | `production_rejects_missing_authz_endpoint` | does not prove OpenFGA semantics |
| Empty CORS allowlist rejected in production | implemented | tested | `production_rejects_permissive_cors_default` | origin pattern validation is not complete |
| OIDC issuer/audience validation | partial | tested when verifier tests run | `packages/authn/oidc-verifier/tests/oidc_verifier_negative_test.rs`, BFF HTTP tests | introspection negative matrix remains incomplete |
| Unknown kid rejection | implemented | tested when verifier tests run | `rejects_jwt_with_unknown_kid` | none known for JWKS JWT path |
| `alg=none` / algorithm confusion rejection | implemented | tested when verifier tests run | verifier `RS256` allowlist, `rejects_jwt_with_disallowed_algorithm`, `rejects_jwt_with_none_algorithm` | future algorithms require explicit config/allowlist change |
| Tenant resolution from verified identity + membership | partial | tested when BFF e2e tests run | `servers/bff/web-bff/src/tenant_context.rs`, `counter_endpoint_rejects_tenant_claim_mismatch_with_forbidden_contract` | broader cross-tenant backend negative matrix incomplete |
| Body/header tenant spoofing rejection | partial | tested when BFF e2e tests run | BFF tenant binding rejects claim mismatch in `counter_endpoint_rejects_tenant_claim_mismatch_with_forbidden_contract` | ordinary body spoof tests incomplete |
| Mock authz impossible in production | partial | tested for runtime policy endpoint requirement | `Config` implements `security-runtime-policy::RuntimeSecurityPolicy` and requires `APP_AUTHZ_ENDPOINT` for production | must prove composition cannot use allow-all mock in prod |
| Audit log | partial | tested when package and BFF tests run | `packages/security/audit`, `counter_mutation_audit_redacts_idempotency_key` | durable audit sink and cross-deployable audit propagation not implemented |
| Sensitive error redaction | partial | unclear | structured errors exist | dedicated negative tests incomplete |
| Worker recovery and replay security | partial | declared | worker checkpoint/dedupe code exists | crash/replay matrix not proven |

## Security-Sensitive Paths

```text
authn
authz
tenant
security
security-audit
security-context
security-runtime-policy
outbox
worker-runtime
infra
CI workflows
release workflows
configuration loading
production profile validation
SurrealDB query adapter
```

Changes in these areas require at least one relevant negative test, config validation test, recovery test, contract drift test, or security gate update.
