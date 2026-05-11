# Template Init Design

> Status: active for the `backend-core` profile.
> `just template-init backend-core dry-run` previews the cleanup set; `just template-init backend-core apply` removes the listed candidates.

## Goal

Turn this upstream repository from:

1. template product
2. contributor workspace
3. architecture research repo

into a derived project with less upstream-maintainer baggage.

## Non-goals

`template-init` should not:

1. silently delete files without preview
2. guess business requirements for the user
3. rewrite the main backend code layout aggressively
4. claim a profile is operationally proven without evidence

## Modes

Initial command shape:

```bash
just template-init backend-core dry-run
```

Supported parameters:

1. `PROFILE`
2. `MODE`

These are positional `just` arguments, not shell-style named assignments. Prefer `just template-init backend-core dry-run` and `just template-init backend-core apply`.

### MODE

1. `dry-run` — print what would be removed or kept; default and safest mode
2. `apply` — remove the selected profile's removal candidates after explicit opt-in

### PROFILE

1. `backend-core` — keep the default Rust backend reference chain and review upstream-only materials
2. `backend-desktop` — future profile; keep backend plus desktop-related app paths
3. `full-research` — keep everything; useful as a no-op baseline for contributors

Current implementation supports `backend-core` dry-run and apply. Other profiles are still planning surfaces.

## backend-core profile

### Keep

1. `README.md`
2. `LICENSE`
3. `AGENTS.md`
4. `agent/**`
5. `CODE_OF_CONDUCT.md`
6. `docs/operations/**`
7. `docs/contracts/README.md`
8. `services/counter-service/**`
9. `servers/bff/web-bff/**`
10. `workers/outbox-relay/**`
11. `workers/projector/**`
12. shared packages required by the default backend path
13. `infra/**` required by local dev and secrets flow
14. stable setup / dev / test / quality commands

### Candidate removals or review

1. `docs/governance/**`
2. `docs/archive/**`
3. `release-plz.toml`
4. `release-plz.template.toml`
5. `.github/workflows/release-plz.yml`
6. `tools/repo-release/**`
7. `.github/ISSUE_TEMPLATE/**`
8. `.github/pull_request_template.md`
9. `verification/e2e/**`

### Review manually

1. `CONTRIBUTING.md`
2. `CODE_OF_CONDUCT.md`
3. `docs/template-users/**`
4. `.github/workflows/**`
5. optional extra modules your derived project may or may not keep

## Safety rules

Before using `apply`, run the dry-run and review the exact removal candidates.

When `apply` mode is used, it should:

1. require explicit `apply`
2. print the selected profile
3. print the exact paths to remove
4. refuse to run if the worktree is dirty unless the user explicitly acknowledges the risk
5. stay focused on backend-core template cleanup instead of guessing business requirements
6. be followed by `just audit-backend-core strict` and `just verify`

Set `TEMPLATE_INIT_ALLOW_DIRTY=1` only when you have reviewed the pending changes and intentionally want to apply cleanup in a dirty worktree.

The `backend-core` apply path also removes the upstream `axum-harness` release anchor from the root `Cargo.toml`. That anchor exists only so the upstream template can maintain repository-level tags, GitHub Releases, and the root changelog; derived projects do not need to inherit it unless they intentionally keep and rename the release-plz setup.

## Backend-core proof

The backend-core profile is considered safe only when these commands pass without requiring a frontend repository:

```bash
just audit-backend-core strict
just typecheck
just verify-contracts strict
just verify
```

Root cleanup is only complete when the backend-core contract no longer exposes root-level frontend commands, frontend-specific Moon tasks, or frontend-aware type generation paths.

## Why profiles exist

Profiles matter because this repository serves two very different groups:

1. people adopting a backend template quickly
2. people maintaining the upstream template as an open-source project

Those users should not be forced to carry the same repository surface forever.
