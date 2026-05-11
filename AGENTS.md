# AGENTS.md

> Thin coordination protocol for this repository.
> Keep this file stable, short, and cross-cutting.
> Put volatile plans in `docs/_local/`.

## 1. Language

Communication **MUST** be Chinese.

Code, commands, config keys, logs, and protocol fields stay in their original language.

## 2. Repository Identity

This repository is an agent-first distributed backend harness template.

Default backend anchor: `counter-service`.

Business reference chain:
`service -> contracts -> server -> outbox -> relay -> projector`.

Engineering reference chain:
`declared platform metadata -> secrets shape -> deploy shape -> GitOps direction -> runbook/gate evidence`.

Root backend-core commands MUST NOT depend on optional frontend, desktop, mobile, or UI shell surfaces by default.

Backend deployable secrets use `SOPS + age` as the canonical shape. Local processes use `just sops-run`; single-VPS `systemd-binary` and optional prebuilt Podman application profiles use transient host env-files from `just sops-export-env`; cluster paths use Kustomize/Flux. Do not make `.env` the backend reference path.

Runtime hosts do not compile first-party Rust code. Single-VPS paths are binary-first by default; Podman primarily manages opt-in official resource containers such as SurrealDB, NATS, Valkey, MinIO, auth, and observability components. Low-resource presets may use embedded, in-process, local, or managed backends with explicit semantic limits. PostgreSQL is not the repository reference backend; current database lanes prioritize embedded libSQL/SQLite, optional Turso Cloud, and optional SurrealDB.

## 3. Operating Posture

Act as a senior distributed-systems engineer in a long-lived codebase.

Default rules:

1. Read before changing.
2. Understand the invariant before touching code.
3. Fix the smallest causal closed loop, not the nearest symptom.
4. Prefer simple, explicit, typed, testable code.
5. Do not add speculative features, abstractions, compatibility layers, or configurability.
6. Touch only lines required by the task.
7. Match existing style.
8. Do not hand-edit generated artifacts.
9. Surface uncertainty instead of guessing.
10. Verification not executed must not be reported as passed.

## 4. Development Loop

For SDD/BDD/TDD-style work:

1. Translate the request into the smallest verifiable behavior.
2. Prefer executable specification: tests, schemas, validators, gates, examples.
3. Do not create planning docs unless the user explicitly asks for a durable document.
4. For bugs, reproduce or localize first; then repair the smallest causal boundary.
5. A spec is not done until it is represented by code, tests, contracts, validators, or gates.
6. If a written note is needed during implementation, keep it in `docs/_local/**` or the PR/handoff, not tracked docs.
7. Describe observable behavior before internal structure.
8. Name the boundary and invariant before adding features.
9. Docs and YAML are not proof.

## 5. Truth Hierarchy

When determining current state, trust evidence in this order:

1. code, schemas, validators, tests, gates, scripts, and command output
2. generated artifacts only when produced from current sources and checked for drift
3. `platform/model/**` and `services/*/model.yaml` as declared metadata indexes
4. `agent/**` manifests and `.agents/**` skills
5. prose documentation

Rules:

1. Executable evidence beats docs and YAML.
2. Metadata can declare intent; it does not prove behavior.
3. Never infer file/module existence solely from target-state documentation.
4. Use evidence labels consistently: `declared`, `checked`, `tested`, `proven`.

## 6. Documentation Write Control

Agents MUST NOT create new tracked files or directories under `docs/**` without explicit user approval for the exact path and purpose.

Default behavior:

1. Prefer editing existing code, tests, schemas, validators, gates, or `agent/**`.
2. Prefer PR notes, handoff text, issue comments, or `docs/_local/**` for temporary reasoning.
3. Do not create tracked docs to record ordinary implementation progress.
4. Do not create vocabulary, status, audit, checklist, or planning docs in tracked `docs/**`.
5. Do not create new docs categories.
6. Do not promote `_local` material into tracked docs without explicit user approval.

A tracked doc proposal must answer:

1. What invariant or long-lived decision does this preserve?
2. Why is an existing file insufficient?
3. What executable source does it point to?
4. Who owns it?
5. When should it be reviewed or deleted?

## 7. Reading Paths

Backend tasks read:

1. `AGENTS.md`
2. `agent/codemap.yml`
3. `agent/manifests/routing-rules.yml`
4. `agent/manifests/gate-matrix.yml`
5. relevant `docs/architecture/**` only when architecture doctrine is directly needed
6. relevant `docs/adr/**` only when a durable decision is directly involved

Tooling, scripts, gates, or repo-control tasks also read:

1. `Justfile`
2. `justfiles/**`
3. `moon.yml`
4. `tools/repo-tools/**`

Infra, secrets, topology, or deploy tasks also read:

1. `docs/operations/**`
2. `platform/model/**`
3. relevant `infra/**` and `ops/**`

Bug reports and regressions start from executable evidence:
failing command, test, validator, schema, log, or reproduction path.

## 8. Routing

Full routing lives in `agent/manifests/routing-rules.yml`.

Quick reference:

| Path                                                            | Primary owner      |
| --------------------------------------------------------------- | ------------------ |
| `platform/model/**`, `platform/schema/**`, `infra/**`, `ops/**` | platform-ops-agent |
| `packages/contracts/**`, `docs/contracts/**`                    | contract-agent     |
| `services/**`                                                   | service-agent      |
| `servers/**`                                                    | server-agent       |
| `workers/**`                                                    | worker-agent       |
| `tools/repo-tools/**`, `Justfile`, `justfiles/**`, `moon.yml`   | planner            |
| `AGENTS.md`, `agent/**`, `.agents/**`, root config              | planner            |
| `docs/architecture/**`, `docs/adr/**`, `docs/governance/**`     | planner            |

Multi-domain dispatch order:
platform-ops -> contract -> service -> server/worker -> verification.

## 9. Gate Selection

Gate selection is based on changed paths, risk category, and evidence level.

Use `agent/manifests/gate-matrix.yml`.

Default backend-core development should prefer path-scoped guardrails and:

`just check-backend-primary`

Use `just verify` for broader repo-wide backend-core confidence.

Do not treat passing gates as the goal. Gates are evidence for restored behavior.

## 10. Tooling Control Plane

`just` is the human command surface. Keep recipes thin.

`moon` is task orchestration. Keep task graph and cache inputs there.

`tools/repo-tools` is the Rust repo-control CLI for reusable validation, generation, drift, routing, and operational logic.

Rules:

1. No opaque `bash -c` migrations into Rust.
2. External commands need structured args, explicit cwd, preflight checks, and clear errors.
3. Write operations must distinguish dry-run from apply.
4. Secret-handling commands must redact values.
5. Generated directories are read-only.
6. High-risk operations require plan, preflight, execute, and verify phases.

## 11. Bug Fix Protocol

For bugs, failing gates, regressions, and suspicious behavior:

1. Reproduce or localize with executable evidence when feasible.
2. Identify the violated invariant.
3. Determine the causal boundary.
4. Make the minimal complete repair at that boundary.
5. Add or update regression verification at the same semantic level.
6. Run relevant gates.
7. State residual risk if the fix is tactical.

## 12. Completion Report

When reporting completion, state:

1. what changed
2. why it restores or improves the relevant invariant
3. what verification ran
4. what was not run
5. residual risk or uncertainty
