# Claim Evidence Audit

This audit maps repository-level README and CHANGELOG claims to executable evidence or explicit gaps. It is a Phase 0A guardrail against treating prose, metadata, or target-state notes as proof.

## Root README Claims

| Claim | Status | Evidence Level | Evidence | Gap / Boundary |
| --- | --- | --- | --- | --- |
| Backend-first Rust/Axum harness with explicit contracts, service boundaries, transactional semantics, and verification gates | implemented / partial | checked / tested when gates run | `agent/codemap.yml`, `packages/contracts/**`, `services/counter-service/**`, `just check-backend-primary`, `just test-backend-primary` | Only the counter reference chain has strong local evidence; new services must earn their own tests. |
| Optional infrastructure should not be required for minimal backend path | implemented | checked when gate runs | `just audit-backend-core strict`, `tools/repo-tools/src/commands/template.rs` | Optional lanes still exist; do not infer they are default prerequisites. |
| CAS, idempotency, transactional outbox, projections, and replay exist as a reference chain | partial | tested for counter mutation/outbox; replay evidence in progress | `services/counter-service/tests/integration/full_stack_test.rs`, `workers/projector/**` | Worker crash recovery and DLQ semantics are not production-proven. |
| Runtime topology can grow toward single-VPS and K3s-style deployments | declared / checked / locally tested | checked when platform gates run; tested for local K3d smoke | `platform/model/**`, `infra/k3s/**`, `infra/kubernetes/addons/**`, `just validate-platform`, `just validate-topology`, `just smoke-local-k3d`, `just gate-local-k3d` | Local K3d proves only local container-node smoke and SurrealDB pod-restart PVC persistence; it does not prove staging/production K3s, Flux reconciliation, VPS reboot recovery, HA, or cross-node storage migration. |
| Repository is not production-proven | implemented | declared | `README.md`, `docs/status-matrix.md`, `docs/security-status.md` | This is an explicit constraint, not a missing implementation. |
| Cargo crate versions are internal workspace metadata | implemented | checked when publish gate runs | `docs/package-classification.md`, `just validate-publish-intent strict` | No public crates are allowed before a future promotion phase. |

## CHANGELOG Claims Requiring Care

| Claim Area | Status | Evidence Level | Evidence | Gap / Boundary |
| --- | --- | --- | --- | --- |
| Generic OIDC verifier supports discovery, JWKS validation, introspection, caching, and identity extraction | implemented / partial | tested after verifier and BFF tests run | `packages/authn/oidc-verifier/src/lib.rs`, `servers/bff/web-bff/tests/http_e2e_test.rs` | Negative JWT matrix belongs in `authn-oidc-verifier` tests before stable security claims. |
| Production config rejects unsafe auth defaults | implemented | tested | `servers/bff/web-bff/src/config.rs` tests | Does not prove runtime OpenFGA semantics. |
| K3s/GitOps paths are delivery landing zones | partial | declared / checked; local K3d smoke tested for addons | `infra/k3s/**`, `infra/kubernetes/addons/**`, `infra/gitops/**`, `platform/model/**`, `just smoke-local-k3d` | Health, promotion, rollback, live Flux reconciliation, and production drift behavior remain not proven. |
| Counter outbox delivery and worker replay/resilience verification are strengthened | partial | tested where worker tests run | `workers/outbox-relay/**`, `workers/projector/**`, `verification/resilience/**` | Existing resilience files include placeholders; crash/DLQ matrix still needs executable tests. |
| Agent context aligns around executable evidence and generated-readonly paths | implemented | checked when boundary gate runs | `AGENTS.md`, `agent/codemap.yml`, `agent/manifests/**`, `just boundary-check` | Agent manifests guide routing; they do not prove semantic behavior. |

## Current Non-Claims

The repository must not currently claim these as stable or production-proven:

```text
secure multi-tenant production auth
complete tenant/authz/audit chain
worker crash recovery guarantee
DLQ delivery guarantee
K3s production readiness
public crate compatibility
```

Use `docs/status-matrix.md` and `docs/security-status.md` as the current conservative status index.
