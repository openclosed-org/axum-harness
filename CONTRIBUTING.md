# Contributing to axum-harness

This guide is for people contributing to the upstream `axum-harness` template.

If you are using this repository as a template for your own project, start with `docs/template-users/README.md` instead. Template users do not need to preserve every upstream maintainer document, GitHub template, release automation file, or agent governance artifact in their derived project.

## Before You Start

Read these first:

1. `README.md`
2. `AGENTS.md`
3. `agent/codemap.yml`
4. `agent/manifests/gate-matrix.yml`
5. `docs/operations/counter-service-reference-chain.md`

This repository is not a generic monorepo. It is a backend-first Rust reference architecture with a built-in multi-agent harness. Successful contributions follow the existing boundaries instead of bypassing them.

## Contribution Paths

Every issue or PR should choose one path.

### Core Path

Use this for small, low-risk improvements.

Good examples:

- Rust cleanup
- tests
- docs hygiene
- local developer workflow
- small `counter-service` readability improvements
- contract drift or generated artifact checks that do not change public shapes

Expected evidence:

- relevant tests or checks
- no change to public contracts, topology defaults, event semantics, or gate behavior unless explicitly stated

### Reference Chain

Use this for changes to the serious backend reference path.

Examples:

- idempotency
- CAS
- transactional outbox
- outbox relay
- projector
- replay/rebuild
- event envelope
- counter-service correctness

Expected evidence:

- invariant-focused tests
- relevant strict gates when distributed semantics change
- clear failure semantics

### Topology Profile

Use this for deployment and cross-cutting infrastructure.

Examples:

- SOPS
- Kustomize
- Flux
- K3s
- Podman or container packaging
- NATS
- Valkey
- OpenTelemetry
- platform model topology
- runbooks

Expected evidence:

- profile or topology classification
- validation command output
- no claim of production readiness unless proven by executed gates and operational evidence

## Risk Levels

Low-risk changes:

- typo fixes
- docs hygiene
- local scripts that do not change default runtime behavior
- tests that do not change behavior
- small internal refactors

Medium-risk changes:

- service internals
- contracts implementation without public shape changes
- generated checks
- platform model validation
- worker internals without new replay, delivery, or idempotency semantics

High-risk changes:

- public API contracts
- event contracts
- idempotency / CAS / outbox semantics
- tenant / auth boundaries
- topology defaults
- gate matrix
- generated artifact rules
- directory semantics
- default runtime dependencies

High-risk changes should open an issue or RFC before a PR.

## Evidence Levels

Use these terms consistently:

- `declared`: written in metadata or docs; useful for navigation, not evidence of behavior
- `checked`: validated by a schema, static validator, drift check, typecheck, or boundary check
- `tested`: exercised by unit, integration, contract, replay, resilience, or end-to-end tests
- `proven`: supported by an explicit gate or test run that is appropriate for the claimed invariant and cited in the change record

Do not claim a gate passed unless it actually ran in this change context.

## Runtime Minimalism, Semantic Completeness

The default runtime path should stay small enough for ordinary local development and single-VPS adoption, but the architecture must not become primitive.

A mature cross-cutting tool may be absent from the inner-loop runtime and still be important to the reference chain. Do not delete valid topology, profile, adapter, or capability-slot work just because it is not enabled in `local-dev`.

The rule is:

- semantic boundary early
- adapter seam early
- local substitute early
- real runtime profile when justified
- no business-code rewrite when enabling the real tool

## Capability Slots

A capability slot is a stable home for a cross-cutting capability. It can include an interface, metadata, config shape, local substitute, real adapter, profile mapping, and verification command.

Capability slots do not mean every real runtime dependency is required by default. They also do not mean a declared future capability is production-proven.

Examples:

- messaging: `EventBus`, `EventEnvelope`, `event_outbox`, memory/NATS adapters, relay gates
- projection: replayable source, checkpoint, rebuildable read model, projector gates
- secrets: SOPS templates, `just sops-run`, Kustomize/Flux overlays, delivery gates
- auth/authz: principal, tenant scope, authorizer port, local/mock and real provider adapters

## Hard Rules

Do not:

- weaken gates to pass CI
- hand-edit generated artifacts
- delete directionally correct topology/profile work because it is not used in local dev
- introduce product-specific business logic as a generic template improvement
- claim semantic correctness from YAML or docs alone
- turn future intent into current fact
- change public contracts, event semantics, topology defaults, service ownership, or default runtime dependencies without prior discussion
- make external frontend, release, production, or platform-full gates mandatory for ordinary backend-core changes unless the risk justifies it

Generated directories are read-only. Change the source and regenerate instead.

## Change Order

Use the existing repository order of operations:

1. If the platform schema cannot express the change, update `platform/schema/**` first.
2. Update `platform/model/**` for platform-level metadata and topology.
3. Update `services/<name>/model.yaml` for service-local semantics.
4. Update `packages/contracts/**` before implementation when API/event shapes change.
5. Then update `services/**`, `servers/**`, `workers/**`, and related verification.

If a change crosses multiple domains, preserve the existing boundaries rather than collapsing everything into one patch.

## Where Changes Belong

- `platform/**`: platform model, validators, generators, topology, infrastructure metadata
- `packages/contracts/**`: contracts and source-of-truth protocol types
- `services/**`: domain logic and service-local semantics
- `servers/**`: sync protocol adaptation and entrypoints
- `workers/**`: async execution, replay, checkpoint, recovery
- `docs/**`: durable guidance for template users, contributors, maintainers, operators, or architecture decisions

## Development Setup

```bash
just setup
just doctor
just auth-up
```

Frontend applications live in separate repositories and consume this backend through contracts such as OpenAPI or generated SDK artifacts.

For local backend runs that need secrets, use SOPS-based injection:

```bash
just sops-run web-bff dev
```

## Validation

Run the smallest relevant validation set for your change, then escalate only when path, risk, or reviewer request justifies it.

Common commands:

```bash
just check-backend-primary
just test-backend-primary
just boundary-check
just verify-contracts strict
just verify
```

Gate-selection guidance is available here:

```bash
cargo run -p repo-tools -- gate-guidance --list

```

Heavier platform, replay, delivery, and release gates are selected by changed paths, risk, and evidence level. Do not run or require them for every ordinary backend-core change.

Frontend, desktop, mobile, and admin shells are out of repository scope. Validate those surfaces in their own repositories against this backend's published contracts.

## Pull Requests

Every PR should declare:

- contribution path
- problem or invariant
- risk level
- evidence level
- exact commands run
- commands not run and why
- contract, event, topology, generated artifact, gate, ownership, or default runtime impact
- reviewer focus

Keep PRs focused. Avoid unrelated cleanup in the same PR.

## Good First Contribution Areas

Good first PRs usually touch:

- tests
- docs hygiene
- local dev clarity
- small Rust cleanup
- issue reproduction
- counter-service readability
- generated artifact checks
- small command-surface improvements

Avoid starting with:

- new services
- new default infrastructure
- large architecture rewrites
- auth provider changes
- topology default changes
- release process changes

## Commit and Release Semantics

- This repository is versioned as one template product using repository-level SemVer.
- Follow the latest repository tag/release as the active template line until there is a deliberate reason to open a new pre-1.0 line or to declare `1.0.0` stability.
- Prefer conventional commits such as `feat:`, `fix:`, `docs:`, `refactor:`, `ci:`, and `chore:`.
- Use scopes when they add signal, for example `feat(template):`, `fix(bff):`, `docs(readme):`, `refactor(worker-runtime):`.
- If a change alters template defaults, project layout, setup flow, or migration expectations for template users, call that out explicitly in the PR description and release notes.
- If a change is only contributor-facing or internal, keep the commit message clear so `release-plz` changelogs stay readable.

`release-plz` prepares repository releases and updates the root `CHANGELOG.md`. Most binaries/internal crates are configured with `publish = false`, so release automation can still generate changelogs and GitHub releases without requiring every crate to publish to crates.io.

## Reporting Bugs and Features

Use the GitHub issue templates. Pick the template that matches your role:

- template user question
- bug report
- contribution proposal
- architecture RFC

For security issues, use the private contact in `.github/ISSUE_TEMPLATE/config.yml` instead of opening a public issue.

## Code of Conduct

By participating in this project, you agree to follow `CODE_OF_CONDUCT.md`.

## License

This project is licensed under the Apache License, Version 2.0. See `LICENSE`.
