# axum-harness

`axum-harness` is an agent-first Rust/Axum backend harness template for building a modular-monolith backend that can grow from a small local service into stricter contracts, workers, delivery profiles, and topology gates without making Kubernetes or a frontend shell the default path.

The current anchor is `counter-service`: a deliberately small business capability used to exercise domain boundaries, contracts, Axum composition, CAS, idempotency intent, transactional outbox, relay, projection, replay, secrets shape, deployment metadata, and executable gates.

This repository is not a production-proven framework. It is a living reference system where architecture claims are expected to be backed by code, tests, validators, generated artifacts, gates, or command output.

## Why This Exists

Most backend templates show either a thin HTTP demo or an overbuilt platform skeleton. This project tries to keep the smallest useful business chain while preserving the seams that are expensive to retrofit later:

1. service boundaries that are libraries first, not premature microservices
2. protocol contracts before external API and event shape drift
3. Axum servers as protocol adapters and composition roots, not business layers
4. workers for asynchronous delivery, projection, replay, and recovery semantics
5. topology-late growth from local development to single VPS, Podman resource containers, and optional K3s/K3d profiles
6. an agent-readable control plane: `AGENTS.md`, `agent/codemap.yml`, routing rules, gate matrix, thin `just` commands, and Rust `repo-tools`

## Current Evidence Snapshot

Use these labels precisely:

| Label | Meaning |
| --- | --- |
| `declared` | stated in docs, YAML, metadata, or manifests |
| `checked` | validated by schema, static validation, typecheck, drift check, or boundary check |
| `tested` | exercised by automated tests |
| `proven` | supported by an executed gate or runtime/operational evidence for the specific claim |

Current reference-chain status:

| Capability | Current evidence |
| --- | --- |
| `counter-service` library boundary | `tested` |
| contracts-first HTTP and event shape | `checked` to `tested`, depending on path |
| CAS mutation and outbox write | `tested` |
| idempotency semantics | `declared` to `tested` for happy paths; not production-grade retry recovery |
| `web-bff` as default composition root | `checked` and covered by backend primary lanes |
| outbox relay and projector structure | `checked` to `tested`; not multi-replica production-proven |
| SOPS secret shape and deploy metadata | `declared` to `checked` |
| GitOps, promotion, rollback, HA topology | partially `declared`/`checked`; not production-proven |

Do not upgrade any claim above the evidence you have actually run.

## Quick Start

```bash
just --list
just setup
just doctor
just check-backend-primary
just test-backend-primary
just dev-api
```

`just dev-api` starts the default Web BFF. After it starts, open `http://localhost:3010/scalar` for the API documentation UI.

For the full local workflow, read `docs/operations/local-dev.md`.

## What You Can Copy

Copy the pattern, not every current crate or profile.

1. Put business semantics in `services/<name>/model.yaml` and `services/<name>/src/**`.
2. Keep services as pure Rust libraries by default.
3. Put external DTOs, events, and error shapes in `packages/contracts/**` before exposing them.
4. Use `servers/**` for synchronous protocol adaptation and composition.
5. Use `workers/**` for asynchronous progress, replay, projection, checkpoints, retry, and delivery semantics.
6. Keep generated artifacts read-only and drift-checked.
7. Select gates from changed paths and risk, not from habit.

Template adopters can preview upstream cleanup with:

```bash
just template-init backend-core dry-run
```

See `docs/template-users/README.md` before turning this repository into a product fork.

## Reference Chain

The default backend chain is:

```text
service library
  -> shared contracts
  -> web-bff composition root
  -> CAS + idempotency intent + event_outbox
  -> outbox-relay worker
  -> projector worker
  -> replayable read model
  -> gates and drift checks
```

Important files:

| Area | Path |
| --- | --- |
| service semantics | `services/counter-service/model.yaml` |
| service implementation | `services/counter-service/src/**` |
| service tests | `services/counter-service/tests/**` |
| shared contracts | `packages/contracts/**` |
| HTTP composition | `servers/bff/web-bff/src/**` |
| outbox relay | `workers/outbox-relay/**` |
| projector and replay | `workers/projector/**` |
| platform declarations | `platform/model/**` |
| executable verification | `verification/**`, `justfiles/**`, `tools/repo-tools/**` |

For the detailed state and gaps, read `docs/operations/counter-service-reference-chain.md`.

## Architecture Position

The default shape is a Rust multi-crate modular monolith, not early microservices.

1. `services/**` own business capabilities and state semantics.
2. `packages/contracts/**` owns shared protocol shapes.
3. `servers/**` adapts HTTP/RPC protocols and wires dependencies.
4. `workers/**` owns async execution and recovery behavior.
5. `platform/model/**` declares platform-level metadata and global shape.
6. `infra/**` and `ops/**` hold delivery declarations and operational runbooks.
7. `tools/repo-tools/**` holds reusable repo-control logic that should not live as opaque shell.

The design borrows from DDD, Clean Architecture, Hexagonal Architecture, Evolutionary Architecture, C4, and seam-driven development, but this repository treats those ideas as executable boundaries and gates, not as vocabulary decoration.

## BFF Boundary

`servers/bff/web-bff` is currently the default runtime composition root.

It may handle HTTP routing, request/response mapping, auth/session adaptation, cookies/CSRF, OpenAPI exposure, and presentation aggregation. It must not own domain rules, durable transaction semantics, worker recovery, or direct replacement for service/application logic.

Future Web, Mobile, Desktop/Tauri, Admin, Public API, CLI, and Agent clients should consume contracts or dedicated protocol surfaces. Do not make the backend-core path depend on optional frontend, desktop, mobile, or UI shell packages.

## Command Surface

`just` is the human and agent command surface. Recipes should stay thin; reusable validation, generation, drift checks, and operational logic belong in `tools/repo-tools`.

High-frequency commands:

| Goal | Command |
| --- | --- |
| inspect commands | `just --list` |
| setup tools | `just setup` |
| diagnose environment | `just doctor` |
| run backend API | `just dev-api` |
| static backend primary lane | `just check-backend-primary` |
| test backend primary lane | `just test-backend-primary` |
| repo-wide default verification | `just verify` |
| boundary checks | `just boundary-check` |
| contract validation | `just verify-contracts strict` |
| generated drift | `just drift-check` |
| replay hooks | `just verify-replay strict` |
| counter delivery admission | `just verify-counter-delivery strict` |
| CI-aligned single-node gate | `just gate-ci-single-node` |
| local K3d smoke | `just smoke-local-k3d` |

Gate selection lives in `agent/manifests/gate-matrix.yml`. Do not report a gate as passed unless you executed it.

## Local And Deploy Profiles

The default path is intentionally low-resource.

1. Local development can use embedded libSQL/SQLite-style storage or optional Turso Cloud instead of requiring a database container.
2. Podman is primarily for opt-in official resource containers such as SurrealDB, NATS, Valkey, MinIO, auth, and observability components.
3. Runtime hosts should not compile first-party Rust code. Single-VPS paths are binary-first by default.
4. Backend deployable secrets use `SOPS + age` as the canonical shape. Local processes use `just sops-run`; VPS/systemd and optional prebuilt Podman application profiles use transient host env-files from `just sops-export-env`.
5. Cluster paths use Kustomize/Flux-style declarations, but those are not the default local requirement.

PostgreSQL is not the repository reference backend. Current database lanes prioritize embedded libSQL/SQLite, optional Turso Cloud, and optional SurrealDB.

## Agent-First Workflow

Humans and agents should start from stable repository context, not chat history.

1. Read `AGENTS.md`.
2. Use `agent/codemap.yml` for ownership and source-of-truth navigation.
3. Use `agent/manifests/routing-rules.yml` before crossing service, contract, server, worker, platform, or tooling boundaries.
4. Use `agent/manifests/gate-matrix.yml` to pick verification.
5. Trust executable evidence over prose, YAML, and target-state plans.
6. Do not hand-edit generated artifacts.
7. Do not create tracked docs for ordinary implementation progress.

## Repository Map

| Path | Role |
| --- | --- |
| `AGENTS.md` | cross-cutting collaboration protocol |
| `agent/**` | codemap, routing, gate selection, architecture metadata |
| `.agents/**` | agent skills and workflow instructions |
| `services/**` | business capability libraries |
| `packages/contracts/**` | API, event, auth, and error contract crates |
| `servers/**` | synchronous request entrypoints and protocol adapters |
| `workers/**` | async workers, projectors, schedulers, replay, recovery |
| `packages/**` | shared kernel, runtime, data, security, observability, adapters, SDKs |
| `platform/model/**` | declared platform metadata and topology indexes |
| `platform/schema/**` | platform schemas |
| `platform/validators/**` | platform validators |
| `platform/generators/**` | platform generators |
| `infra/**` | infrastructure and delivery declarations |
| `ops/**` | operational runbooks |
| `verification/**` | contract, topology, resilience, golden, and replay evidence |
| `justfiles/**` | thin command-surface groups imported by root `Justfile` |
| `tools/repo-tools/**` | Rust repo-control CLI |
| `docs/**` | durable architecture, operations, contracts, template-user, and governance docs |

## Known Limits

1. `counter-service` is the reference anchor, not proof that every service pattern is production-ready.
2. Idempotency currently has known production-readiness gaps around durable request hash/status/result recovery.
3. Outbox and projector workers are not proven for multi-replica HA behavior.
4. Platform metadata declares intent; it is not runtime proof by itself.
5. GitOps and cluster paths have real declarations and checks, but are not a fully proven release pipeline.
6. Optional auth, SurrealDB, Podman, K3d, and observability lanes should stay opt-in unless a task explicitly targets them.

## Reading Paths

| Goal | Start here |
| --- | --- |
| run locally | `docs/operations/local-dev.md` |
| understand the backend anchor | `docs/operations/counter-service-reference-chain.md` |
| understand architecture direction | `docs/architecture/north-star.md` |
| understand harness philosophy | `docs/architecture/harness-philosophy.md` |
| manage secrets | `docs/operations/secret-management.md` |
| understand gate profiles | `docs/operations/gate-profiles.md` |
| use as a template | `docs/template-users/README.md` |
| contribute upstream | `CONTRIBUTING.md` |
| browse docs | `docs/README.md` |
| track releases | `CHANGELOG.md` and GitHub Releases |

## Versioning

Template releases are tracked by repository tags and GitHub Releases. Cargo crate versions are internal workspace metadata unless documented otherwise.

## License

Apache 2.0. See `LICENSE`.
