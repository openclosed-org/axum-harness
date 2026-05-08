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
| Tenant resolution from verified identity + membership | partial | checked | `servers/bff/web-bff/src/tenant_context.rs` | broader cross-tenant backend negative matrix incomplete |
| Body/header tenant spoofing rejection | partial | unclear | BFF tenant binding rejects claim mismatch | ordinary header/body spoof tests incomplete |
| Mock authz impossible in production | partial | tested for config endpoint requirement | `Config::validate_runtime_for_profile` requires `APP_AUTHZ_ENDPOINT` | must prove composition cannot use allow-all mock in prod |
| Audit log | missing | declared | roadmap only | append-only audit and redaction policy not implemented |
| Sensitive error redaction | partial | unclear | structured errors exist | dedicated negative tests incomplete |
| Worker recovery and replay security | partial | declared | worker checkpoint/dedupe code exists | crash/replay matrix not proven |

## Security-Sensitive Paths

```text
authn
authz
tenant
security
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
