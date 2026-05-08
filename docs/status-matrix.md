# Status Matrix

This matrix records current capability status. It is intentionally conservative: claims above `declared` require executable evidence.

## Backend Reference Chain

| Capability | Status | Evidence Level | Evidence |
| --- | --- | --- | --- |
| BDD development discipline | implemented | declared | `docs/development/bdd-development-guide.md` |
| README / CHANGELOG claim audit | implemented | declared / checked | `docs/claim-evidence-audit.md`; checked when referenced gates run |
| Package publish intent | implemented | checked when gate runs | `docs/package-classification.md`, `just validate-publish-intent strict` |
| Counter domain service | implemented | tested | `services/counter-service/tests/unit/counter_service_test.rs`, `services/counter-service/tests/integration/full_stack_test.rs` |
| Counter CAS mutation | implemented | tested | `atomic_counter_outbox_consistency_under_load`, `concurrent_increments_on_embedded_turso` |
| Counter idempotency replay | partial | tested | same-key replay tests exist; request fingerprint is counter-specific, not a generic idempotency framework |
| Counter idempotency conflict | partial | tested | same-key different-operation test exists; cross-resource generic conflict model is not implemented |
| Counter outbox transaction boundary | partial | tested | integration tests count mutation/outbox consistency; worker delivery/recovery belongs to Phase 4 |
| Counter HTTP acceptance | partial | tested | `servers/bff/web-bff/tests/http_e2e_test.rs`; not all BDD negative categories are covered |
| Projection rebuild | partial | tested when projector tests run | `workers/projector/src/main.rs` counter rebuild acceptance |
| Worker recovery | partial | declared | worker code has checkpoint/dedupe/replay scaffolding; crash matrix tests are not proven |

## Platform And Governance

| Capability | Status | Evidence Level | Evidence |
| --- | --- | --- | --- |
| Agent routing boundaries | implemented | checked | `agent/codemap.yml`, `agent/manifests/routing-rules.yml`, `just boundary-check` |
| Gate selection matrix | implemented | declared | `agent/manifests/gate-matrix.yml` |
| Platform metadata validation | partial | checked when gate runs | `just validate-platform`, validators under `platform/validators/**` |
| Generated artifact drift | partial | checked when gate runs | `just verify-generated-artifacts`, contract drift commands |

## Not Stable Claims

The following must not be marketed as stable or production-proven yet:

```text
secure multi-tenant production auth
complete tenant/authz/audit chain
worker crash recovery guarantee
SurrealDB tenant-safe default lane
cargo-generate template profiles
public crates
```
