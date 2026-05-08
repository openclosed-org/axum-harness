# Package Classification

This repository is a single template product. Workspace packages are internal implementation units unless a future phase explicitly promotes a crate to `candidate-public` and adds a publish allowlist entry.

Current policy:

```text
default publish intent: publish = false
public crates: none
candidate-public crates: none
```

`publish = false` is required for every workspace `Cargo.toml` during Phase 0B. The validator is `just validate-publish-intent strict`.

## Stability Labels

```text
internal          implementation detail of the template repository
reference         copy-safe reference slice for template users, still not public API
experimental      exploratory lane; not stable or secure-by-default unless separately proven
candidate-public  potential future crates.io package, blocked until Phase 10 publishing requirements
```

## Current Classification

| Area | Packages | Stability | Publish Intent | Evidence |
| --- | --- | --- | --- | --- |
| repo tooling | `repo-tools` | internal | `publish = false` | checked by `validate-publish-intent` |
| core kernel/runtime | `kernel`, `platform`, `runtime`, `data`, `data-traits`, `observability`, `workspace-hack` | internal | `publish = false` | checked by `validate-publish-intent` |
| contracts | `contracts_api`, `contracts_auth`, `contracts_events`, `contracts_errors` | internal | `publish = false` | checked by `validate-publish-intent` |
| auth | `authn-oidc-verifier`, `adapter-google`, `adapter-google-backend`, `authz` | partial / internal | `publish = false` | checked by `validate-publish-intent`; security behavior tracked in `docs/security-status.md` |
| adapters | `storage_turso`, `storage_surrealdb`, telemetry adapters | internal / experimental | `publish = false` | checked by `validate-publish-intent` |
| services | `counter-service`, `tenant-service`, `user-service`, `auth-service` | reference / internal | `publish = false` | checked by `validate-publish-intent`; behavior tracked in `docs/status-matrix.md` |
| servers | `web-bff`, `pingora-gateway` | reference / internal | `publish = false` | checked by `validate-publish-intent` |
| workers | `outbox-relay-worker`, `projector-worker`, `indexer-worker`, `scheduler-worker`, `sync-reconciler-worker`, `worker-runtime` | partial / internal | `publish = false` | checked by `validate-publish-intent`; recovery proof not complete |
| validators/generators | platform validators and generator crates | internal | `publish = false` | checked by `validate-publish-intent` |
| SDK golden/reference | `sdk-counter`, `sdk-counter-embedded`, `sdk-counter-http` | generated/reference | `publish = false` | checked by `validate-publish-intent` |

## Promotion Rule

A package may not become public until Phase 10. Promotion requires README, crate docs, examples, negative tests, semver policy, `cargo package --list` review, and a publish allowlist entry.
