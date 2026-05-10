# Docs

`docs/` is a small, shared layer for durable human guidance. It is not the agent memory layer.

When docs conflict with code, validators, gates, schemas, tests, executable scripts, or command output, trust the executable sources.

## Allowed Tracked Docs

Tracked `docs/**` is limited to:

1. `docs/architecture/**` — stable architecture principles only.
2. `docs/adr/**` — accepted long-term decisions only; historical or target-state decisions belong in `docs/archive/adr/**`.
3. `docs/operations/**` — executable runbooks and operator guidance only.
4. `docs/contracts/README.md` — navigation to contract sources only.
5. `docs/template-users/**` — minimal template adoption guidance only.
6. `docs/governance/out-of-scope/**` — stable rejected directions only.
7. `docs/archive/**` — historical material, never default guidance.

## Default Reading

For agent-assisted development, start with:

1. `AGENTS.md`
2. `agent/codemap.yml`
3. `agent/manifests/routing-rules.yml`
4. `agent/manifests/gate-matrix.yml`

Read `docs/**` only when the task directly requires that category.

For general readers and template adopters, start with:

1. `README.md`
2. `docs/architecture/north-star.md`
3. `docs/operations/local-dev.md`
4. `docs/operations/counter-service-reference-chain.md`
5. `docs/operations/gate-profiles.md`
6. `docs/template-users/template-init.md`

## Not Allowed Without Explicit User Approval

Agents MUST NOT create new tracked docs files or directories under `docs/**` unless the user explicitly approves the specific path and purpose.

Before proposing a new doc, state:

1. why existing code, tests, gates, `README.md`, `AGENTS.md`, or `agent/**` files are insufficient
2. why this cannot be a PR note, issue comment, commit message, or `_local` scratch note
3. expected owner
4. expected expiration or review trigger
5. evidence source it points to

## Forbidden Uses

Do not use `docs/**` for:

1. temporary plans
2. SDD drafts
3. vocabulary catalogs
4. status snapshots
5. gate transcripts
6. speculative future architecture
7. agent behavior rules
8. duplicate source-of-truth summaries

Put temporary material in `docs/_local/` so it stays outside tracked repository docs and the shared docs entry flow.

`docs/_local/` is the maintainer workspace for active plans, execution notes, and temporary guidance.
It should stay gitignored; if you need lightweight versioning, keep that inside the document itself with frontmatter or explicit revision notes.

## Default Backend Anchor

`counter-service` remains the default backend reference anchor for this repository:

1. business chain: `service -> contracts -> server -> outbox -> relay -> projector`
2. engineering chain: `declared platform metadata -> secrets shape -> deploy shape -> GitOps direction -> runbook/gate evidence`

If you are trying to understand the current backend path, start there instead of reading speculative planning material.

## Repository Notes

Repository-level policy and project-level caveats live here instead of expanding the root `README.md`.

### Versioning

This upstream repository is versioned as a single template product. The release tooling exists to record template changes for maintainers; derived projects can keep it, rename it, or remove it without changing the backend-core runtime contract.

1. the repository tag/release is the version contract template users should follow
2. pre-`1.0.0` releases cover iterative template improvements, docs fixes, tooling updates, and internal refactors within the active pre-1.0 line
3. ordinary PRs and `main` pushes accumulate unreleased changes; they do not automatically publish tags
4. prepare version and changelog updates in an intentional release-prep PR when maintainers decide to cut a release
5. run `Release Automation` manually with the selected `release_type` to publish the prepared release
6. bump the minor version when template structure, migration expectations, or public usage patterns change materially
7. start `1.0.0` only when the template is ready to make a stronger stability promise to adopters

Cargo crate versions still exist as workspace metadata, but they do not represent independent product release channels. The root `axum-harness` package is a maintainer-only release anchor with `publish = false`; it is not required for projects derived from the template.

If you derive a new repository from this template and want to keep release automation, rename the root release anchor to your project identity, bootstrap the first official repository release manually with your chosen starting tag, then let automation continue from that baseline. If you do not want upstream release automation, run or follow `just template-init backend-core apply` to remove the release-plz workflow, config, and root release anchor.

Maintainers can override the default release tag strategy without changing tracked files:

1. `RELEASE_TAG_TEMPLATE` controls the tag name written by `release-plz`, defaulting to `v{{ version }}`
2. `RELEASE_TAG_GLOB` controls which tags CI treats as the current release line, defaulting to `v[0-9]*.[0-9]*.[0-9]*`
3. `RELEASE_BOOTSTRAP_TAG` can pin an explicit existing tag as the release baseline until the next official tag is created

`release-plz` publishes prepared repository releases only when maintainers trigger it manually. `just semver-check` is the local visibility check for repository-level compatibility assumptions within the active pre-1.0 line. See `docs/operations/release-process.md` before changing release automation, version anchors, or changelog policy.

### Configuration

There are three different configuration paths and they should not be confused:

1. canonical backend secret shape: `SOPS + age -> sops-run / sops-export-env / Kustomize-Flux`
2. quick backend debug path: explicit `APP_*` exports for short host-process loops
3. local tooling or desktop convenience path: `.env`, which is not the canonical backend secrets path

If you are working on backend deployables, prefer the first path when the task touches deployable or cluster configuration. For a short host-process debug loop, explicit `APP_*` exports are allowed when they do not become the documented reference path.

### Desktop Scope

`apps/**` and `packages/ui/**` are optional shell surface in this repository, not part of the default backend reference chain.

1. default backend commands such as `just dev`, `just typecheck`, `just verify-contracts`, and `just verify` must not require SvelteKit, Tauri, mobile shells, or `packages/ui`
2. root `just`, `moon`, and shared repo-control helpers must not expose app-shell commands or app-specific validation lanes
3. if you keep `apps/**`, treat them as self-owned shells with their own local commands and validation entrypoints
4. derived backend-only projects can preview or apply the app-shell removal set with `just template-init backend-core dry-run` and then prove the root contract with `just audit-backend-core strict`

### Architecture Rules

The root `README.md` keeps the public overview short. The main repository rules live here and in `AGENTS.md`:

1. `platform/model/*` is the declared metadata index for platform shape; validators, generated drift checks, scripts, gates, and command output decide what is checked, tested, or proven
2. contracts change before implementation when external shapes change
3. services are libraries; servers and workers compose them
4. workers are first-class and must make retry, idempotency, and replay explicit
5. generated directories are read-only
6. topology may change deployment shape, but not business semantics

### GitHub Discussions

GitHub Discussions are design rationale and RFC material. They are not current repository rules unless promoted into tracked docs or ADRs.

When a discussion conflicts with executable sources, accepted ADRs, `AGENTS.md`, or `docs/architecture/north-star.md`, follow the tracked and executable sources.
