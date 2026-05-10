# Changelog

All notable changes to this repository-level template are documented here.

This project ships as one template product:

- Repository tags and GitHub Releases are the public version contract.
- Internal Cargo crate versions remain workspace metadata for tooling.
- `release-plz` prepares release updates from merged conventional commits.

Preferred release views:

- Releases: <https://github.com/openclosed-org/axum-harness/releases>
- Tags: <https://github.com/openclosed-org/axum-harness/tags>

## v0.5.1 - 2026-05-10

### Deployment Profiles And Resource Presets

- Reframed deployment guidance around three profiles: `systemd-binary`, optional `podman`, and future `k3s-ha`.
- Made single-VPS delivery binary-first: runtime hosts consume prebuilt binaries or images and do not compile first-party Rust code.
- Added `lite`, `standard`, and `full` single-VPS resource presets so distributed resources remain opt-in instead of baseline requirements.
- Reworked local and advanced topology docs so K3d and Multipass/K3s rehearsals are explicit local shape checks, not `k3s-ha` proof.

### Secrets And Runtime Configuration

- Added `repo-tools secrets export-env` and `just sops-export-env` for generating transient `0600` host env-files from SOPS-managed deployable secrets.
- Documented `SOPS + age` as the backend deployable secret reference path for local processes, single-VPS binaries, optional Podman app containers, and cluster injection.
- Kept `.env` out of the backend deployment reference path while preserving local generated auth env output for explicit developer use.
- Fixed `web-bff` CORS configuration parsing so SOPS/env comma-separated `APP_CORS_ALLOWED_ORIGINS` values are accepted alongside list-shaped input.

### Podman, Binary Packaging, And Local Maintenance

- Reworked Podman recipes around official resource containers with `just podman-resources-up <lite|surrealdb|standard|full>` and matching status, logs, and shutdown commands.
- Added binary-first build, package, and smoke recipes for `web-bff`, including `just release-web-bff`, `just package-web-bff`, and `just smoke-web-bff-binary`.
- Added sccache-aware build entrypoints and aggressive storage cleanup commands for Rust build outputs, `.sccache`, `.run`, and Podman images, containers, and volumes.
- Added `.dockerignore` and adjusted the proof-only `web-bff` Dockerfile so optional app image builds avoid local build artifacts and use the correct runtime base.

### Platform Model And Generated Evidence

- Updated single-VPS topology metadata with runtime build-location principles, resource preset semantics, and lower-resource defaults.
- Updated SurrealDB and Turso resource metadata to reflect embedded libSQL/SQLite, optional Turso Cloud, and optional SurrealDB lanes; PostgreSQL is not the repository reference backend.
- Regenerated platform catalogs and golden baselines after the platform resource and topology model changes.

### Documentation And Agent Context

- Consolidated agent-facing guidance under `AGENTS.md`, `agent/**`, and `.agents/skills/**` instead of tracked `docs/agents/**` and `docs/language/**` material.
- Removed stale status, security, package-classification, claim-audit, and target-state ADR documents so default docs no longer present volatile or historical claims as current evidence.
- Added the current deployment profile ADR and moved GitOps, K3d, and Multipass/K3s material under `docs/operations/advanced-topology/**`.
- Updated docs lifecycle, operations, out-of-scope governance, infra, and runbook guidance around evidence labels, binary-first deployment, resource presets, and SOPS-backed configuration.

### Verification

- Verified SOPS validation and env-file export for `web-bff` `systemd-binary` and `podman` profiles, including `0600` output permissions.
- Verified repo-tools secret export behavior with `rtk cargo test -p repo-tools secrets`, `rtk cargo fmt -p repo-tools -- --check`, and `rtk cargo check -p repo-tools`.
- Verified `web-bff` CORS config behavior with `rtk cargo test -p web-bff config -- --test-threads=1`, `rtk cargo fmt -p web-bff -- --check`, and `rtk cargo check -p web-bff`.
- Smoke-tested the binary-first `web-bff` path through SOPS env-file export, packaged binary startup, local libSQL bootstrap, and `GET /healthz` returning `{"status":"ok"}`.
- Validated platform and resource control paths with `just validate-platform`, `just validate-topology`, `just verify-generated-artifacts`, `just boundary-check`, Podman compose profile config checks, and Podman resource lifecycle commands.

### Migration Notes

- Use `just sops-run` for local SOPS-backed processes and `just sops-export-env <deployable> <env> <systemd-binary|podman> <output>` for transient host env-files.
- Use `just release-web-bff` and `just package-web-bff` before deploying a single-VPS binary; do not run `cargo build --release` on the runtime host.
- Use `just podman-resources-up lite` for the low-resource local baseline and opt into `surrealdb`, `standard`, or `full` only when the behavior under test needs those resources.
- Treat K3d and Multipass/K3s docs as advanced rehearsal guidance; they do not prove `k3s-ha` without separate 3+ server-node evidence.

## v0.5.0 - 2026-05-09

### SurrealDB Runtime Lane

- Added SurrealDB as a declared runtime resource across platform model environments, topology metadata, generated platform catalogs, local compose infrastructure, Kubernetes addons, and dev secret shape.
- Wired the SurrealDB provider lane through `web-bff`, tenant/user/counter infrastructure adapters, service integration paths, and BFF tenant-store coverage.
- Added SurrealDB local operations documentation and external adapter tests so the lane has explicit setup, persistence, and verification guidance.
- Updated generated OpenAPI and golden contract artifacts for the BFF surface affected by the provider lane.

### Gates, Local Runtime Profiles, And Governance Evidence

- Added local K3d backend profile support with Just recipes, workflow coverage, gate profile documentation, and local Kubernetes guidance.
- Added Phase 0 evidence gates and repository guidance for package classification, security status, status matrix, BDD development, and claim-evidence auditing.
- Updated gate matrix, routing rules, codemap, and operations docs so declared evidence and local runtime checks are easier to select and interpret.

### BFF, AuthZ, And Security Boundaries

- Added tenant audit evidence through the BFF path and expanded tenant context handling around authorization and audit boundaries.
- Extracted shared security contract crates for audit, context, and runtime policy concerns.
- Added AuthZ port boundaries and adjusted BFF authorization/configuration composition to consume the shared security contracts.

### AuthN And Worker Reliability

- Hardened OIDC JWT verification with additional negative coverage around verifier behavior.
- Preserved worker replay checkpoint safety and isolated outbox poller tests to reduce cross-test interference.
- Updated worker runtime and projector paths for safer checkpoint and replay behavior.

### Tooling And Dependency Hygiene

- Updated the OpenSSL lockfile and workspace dependency wiring for the new security, SurrealDB, contract, worker, and validator paths.
- Updated repo tooling around harness commands and worker command handling.
- Adjusted `.mise.toml`, `.gitignore`, root `Justfile`, and verification recipes for the new local gate and runtime lanes.

### Verification

- Added or updated tests for OIDC verifier negative cases, SurrealDB external adapter behavior, BFF HTTP coverage, BFF SurrealDB tenant store behavior, counter full-stack integration, and worker polling isolation.
- Release readiness for this line should be checked with the release gate using the `minor` SemVer expectation before tagging.

## v0.4.1 - 2026-05-05

### BFF And Auth Runtime

- Added `packages/authn/oidc-verifier` as the shared generic OIDC resource-server verifier for discovery, JWKS validation, introspection, verifier caching, and identity extraction.
- Moved `web-bff` runtime auth configuration from Zitadel-specific `APP_ZITADEL_*` variables to provider-neutral `APP_OIDC_*` variables.
- Moved `web-bff` authorization configuration from OpenFGA-specific `APP_OPENFGA_*` runtime names to provider-neutral `APP_AUTHZ_*` names while keeping OpenFGA as the local reference provider.
- Added production runtime validation so `web-bff` rejects unsafe auth defaults such as `APP_AUTH_MODE=dev_headers`, empty CORS origins, default JWT secrets, missing OIDC issuer configuration, or incomplete authz provider settings in production profiles.
- Split the protected request context path into clearer BFF-local modules for auth context middleware, request context construction, HTTP extractors, tenant resolution, and route-level authz checks.
- Moved counter BFF composition into `servers/bff/web-bff/src/application/counter.rs` so handlers stay focused on HTTP extraction and response wrapping while the BFF application layer owns route-level authorization, tenant resolution, cache handling, and service calls.
- Expanded protected endpoint error handling around dependency failures, forbidden access, not found, conflict, unsupported media type, validation failures, and payload-size rejection.
- Preserved Google OAuth adapters as legacy optional client-lane adapters and documented that they are not the server-side resource-server verifier path.

### Local Auth And Infrastructure

- Replaced the local auth compose lane from Zitadel plus Postgres to Rauthy plus OpenFGA, with Rauthy used only as the local reference OIDC provider.
- Updated `repo-tools infra auth bootstrap` to provision OpenFGA and write generic `APP_OIDC_*` and `APP_AUTHZ_*` environment output for `web-bff`.
- Pinned local third-party infrastructure images instead of relying on `latest`, including OpenFGA, NATS, Valkey, MinIO, MinIO Client, SurrealDB, and the optional libSQL server profile.
- Replaced the inaccessible `ghcr.io/tursodatabase/sqld:latest` path with the verified `ghcr.io/tursodatabase/libsql-server:e4beaca` image for the optional full local profile.
- Added `just clean-local-storage` as a conservative local maintenance entrypoint that cleans temporary logs and stale build artifacts without deleting compose volumes, global tool caches, SOPS state, Kubernetes state, or GitOps state.
- Updated local-development docs to describe the current Generic OIDC plus OpenFGA lane, the conservative cleanup contract, and backend-first dev-header debugging path.

### Observability

- Upgraded the local observability stack to OpenObserve `v0.80.1`, OTel Collector Contrib `0.151.0`, and Vector `0.55.0-alpine`.
- Added the OTel Collector health extension on port `13133` and documented the collector as the Rust tracing ingress instead of direct OpenObserve writes.
- Moved Vector behind the optional `logs` compose profile because container log socket behavior differs across Linux Docker, Linux Podman, and macOS Podman machines.
- Updated Vector configuration to use the Docker-compatible Podman socket through the `docker_logs` source supported by Vector `0.55`.
- Scoped `repo-tools infra local observability` to its own compose project name so observability lifecycle commands do not accidentally affect the auth stack.

### Delivery, GitOps, And Runbooks

- Replaced K3s base application image `latest` tags with explicit `:template-local` placeholders that must be replaced by overlays or release tooling with immutable tags or digests.
- Updated Kubernetes addon and Flux infrastructure manifests to use the same pinned NATS, Valkey, MinIO, and MinIO Client image tags as local compose.
- Clarified Flux GitOps docs as a declared cluster-profile landing zone rather than proof that staging, production, health checks, promotion, rollback, or drift handling are fully verified.
- Clarified Terraform docs as a planning placeholder rather than the current default deployment path.
- Reframed runbook and security README files as template/operator starting points and removed absolute production claims about contacts, nonroot posture, sidecar absence, and deployment readiness.
- Pinned the backup/restore runbook Alpine example to `alpine:3.22.2` instead of `alpine:latest`.

### Docs And Agent Context

- Added ADR-010 to record the Generic OIDC plus Rauthy/OpenFGA local auth lane and marked ADR-005 as historical and superseded.
- Updated `.env.example`, BFF docs, local infra docs, Docker docs, authz fixture docs, and operations docs to avoid provider-specific runtime env names as current guidance.
- Audited README and agent context entrypoints to remove stale Phase language, overstated target-state claims, old observability assumptions, placeholder contact details, and statements that could make agents treat scratch plans or rendered manifests as current behavior.
- Confirmed `AGENTS.md`, `agent/README.md`, `.agents/skills/README.md`, `agent/codemap.yml`, `routing-rules.yml`, and `gate-matrix.yml` remain aligned around executable evidence, scratch-doc boundaries, generated-readonly paths, and path/risk-based gate selection.

### Verification

- Verified the updated BFF and repo-tools paths with `rtk cargo fmt -p repo-tools -p web-bff`, `rtk cargo check -p repo-tools -p web-bff`, `rtk cargo test -p repo-tools -p web-bff`, and `rtk cargo clippy -p repo-tools -p web-bff --all-targets -- -D warnings`.
- Verified shared auth package integration with `rtk cargo check -p authz -p authn-oidc-verifier -p repo-tools -p web-bff`.
- Verified platform and topology guardrails with `just validate-platform`, `just validate-topology`, and `just boundary-check`.
- Smoke-tested local auth, core infrastructure, and observability startup paths through `repo-tools` commands and direct HTTP probes for Rauthy discovery, Rauthy JWKS, OpenFGA, NATS, MinIO, OpenObserve, and OTel Collector health.
- Validated the updated OTel Collector and Vector configuration with their pinned container images.

### Migration Notes

- Replace `APP_ZITADEL_*` with `APP_OIDC_*` in local and deployment environments.
- Replace `APP_OPENFGA_ENDPOINT`, `APP_OPENFGA_STORE_ID`, and `APP_OPENFGA_AUTHORIZATION_MODEL_ID` with `APP_AUTHZ_ENDPOINT`, `APP_AUTHZ_STORE_ID`, and `APP_AUTHZ_MODEL_ID`; set `APP_AUTHZ_PROVIDER=openfga` when using the local OpenFGA adapter.
- Use `cargo run -p repo-tools -- infra auth bootstrap` to regenerate local auth env after the provider-neutral env migration.
- Treat K3s `:template-local` image references as placeholders only; overlays, CI, or release tooling must inject real image tags or digests before shared-environment use.
- Do not assume Vector starts with the default observability stack; enable the `logs` profile only when the host container socket path is known and intentionally mounted.

## v0.4.0 - 2026-05-04

### Changed

- Switched the HTTP contract generation path to the web-bff `utoipa-axum` route collection so generated OpenAPI is produced from Axum handlers plus the live Rust DTO schemas.
- Reset the contract artifact layout around `packages/contracts/generated/openapi/web-bff.openapi.yaml` and removed the legacy `ts-rs` TypeScript binding output from the backend-core contract surface.
- Tightened release and template gates so repository release automation checks the selected SemVer compatibility line before preparing release state.
- Streamlined agent, architecture, command, and gate guidance around executable evidence, backend-core boundaries, replay/resilience semantics, and generated-artifact ownership.

### Fixed

- Hardened counter outbox delivery and worker replay/resilience verification so idempotency, recovery, and projection semantics have stronger executable coverage.
- Aligned generated artifact baselines after the OpenAPI contract source reset.

### Breaking Changes

- `packages/contracts/api` no longer exposes legacy DTOs that only existed for the removed `ts-rs` TypeScript binding pipeline, including agent/admin/conversation DTOs and the old local `ErrorResponse`; HTTP error responses now use `contracts_errors::ErrorResponse` and HTTP API consumers should treat the generated OpenAPI artifact as the external contract reference.
- TypeScript consumers should stop importing generated files from `packages/contracts/generated/api/**` and consume the generated OpenAPI contract or SDK output produced from it instead.

### Migration Notes

- Regenerate HTTP contract artifacts with `just typegen` after route or DTO changes, then verify drift with `just drift-check` and contract checks with `just verify-contracts strict`.
- For release checks that intentionally cross this contract boundary, run the repository SemVer gate with the `major` compatibility expectation instead of weakening the contract API surface for a minor check.

## v0.3.1 - 2026-04-29

### Changed

- Replaced the legacy TypeScript and shell script control plane with the Rust `repo-tools` CLI so reusable validation, generation, drift, routing, secret, infra, ops, worker, and app helper logic lives behind structured commands.
- Tightened the `repo-tools` control plane with explicit environment handling, operation phases, manifest helpers, and expanded repo command coverage.
- Consolidated root `just` and `moon` task wiring around the Rust repo-control surface while keeping human-facing recipes thin.
- Consolidated release-plz changelog output into the root `CHANGELOG.md` so the template has one public change history.
- Clarified that the root `axum-harness` package is an upstream maintainer release anchor, not a required derived-project runtime contract.
- Clarified contribution governance, pull request expectations, issue templates, maintainer decision guidance, and SOPS-backed local development workflows.
- Simplified the root README entrypoint so adopters start from the current template posture instead of stale broad guidance.
- Added explicit SOLID guidance to the agent protocol, framed as boundary discipline for services, servers, workers, platform, ports, traits, adapters, and composition roots.

### Fixed

- Extended `template-init backend-core apply` so derived projects can remove the upstream release-plz workflow, runtime config, repo-release helper, and root release anchor together.
- Aligned the platform state ownership map with service-local semantics so platform metadata points at the right domain owners without claiming runtime proof.

### Migration Notes

- Use the Rust `repo-tools` entrypoints behind `just` recipes instead of removed legacy scripts under `scripts/**`, `infra/**/scripts/**`, and `ops/**/migrations/**`.
- Treat the new SOLID guidance as an extension of the existing boundary rules: production service code depends inward on contracts, ports, traits, and shared abstractions; test-only adapters remain allowed when they provide executable evidence.

## v0.3.0 - 2026-04-27

### Added

- Added a `backend-core` template audit path so maintainers and adopters can prove the root command surface no longer depends on optional app-shell directories.
- Added `docs/architecture/harness-philosophy.md` to define harness boundaries, truth hierarchy, evidence levels, metadata limits, and gate strength.
- Added configurable release tag strategy inputs so maintainers can override tag template, tag glob, and bootstrap baseline without editing tracked files.

### Changed

- Decoupled optional web, desktop, mobile, and UI shell surfaces from the default backend-core template contract.
- Reworked root `just`, `moon`, and shared scripts so backend-core commands do not require SvelteKit, Tauri, `apps/**`, or `packages/ui/**` by default.
- Reframed `agent/codemap.yml` as a compact navigation map instead of a full system model or heavyweight architecture constitution.
- Rebuilt `agent/manifests/gate-matrix.yml` around changed paths, risk categories, and evidence levels instead of subagent identity.
- Clarified that `services/<name>/model.yaml`, `platform/model/**`, and agent YAML declarations are semantic summaries or metadata, not formal proof of system correctness.
- Clarified that `advisory`, `guardrail`, and `invariant` gates have different blocking strength, and that invariant gates are reserved for P0 correctness and release readiness.
- Clarified that `just check-backend-primary` is the canonical default backend-core guardrail and `just verify` is a broader repo-wide guardrail, not an automatic requirement to run every platform, frontend, desktop, production, or release gate.
- Strengthened the root agent protocol around bug fixes: reproduce or localize failures, identify violated invariants, make minimal causal repairs, add regression evidence, and never claim unrun gates as passed.

### Fixed

- Fixed release-plz workflow scoping so release PRs are prepared from the repository-level release anchor instead of unrelated workspace packages.
- Fixed release baseline comparison to use the active template tag line instead of stale hard-coded assumptions.
- Fixed release-plz worktree hygiene so generated release state does not leave the repository dirty during automation.
- Fixed backend-core root entrypoint drift by removing stale app-shell command exposure from shared validation and development lanes.

### Documentation

- Updated README, CONTRIBUTING, docs index, and agent docs to describe path/risk/evidence-based gate selection.
- Documented that metadata-only changes can raise a claim to `declared`, but `checked`, `tested`, and `proven` claims require executable evidence such as validators, tests, gates, or command output.

### Migration Notes

- Use `just check-backend-primary` for ordinary backend-core development and add path-specific guardrails from `agent/manifests/gate-matrix.yml` when contracts, platform model, workers, topology, delivery, or release risk is involved.
- Use `cargo run -p repo-tools -- gate-guidance --list` as gate-selection guidance; it does not run heavyweight gates just because an agent scope appears in the workflow.
- Treat app-shell validation as local to retained app shells. Root backend-core admission should remain independent from optional frontend and desktop surfaces.

## v0.2.0 - 2026-04-04

### Notes

- Current repository baseline tag for the template line.
- Tagged from commit `95ae1c9` (`chore: archive v0.2.0 milestone`).
