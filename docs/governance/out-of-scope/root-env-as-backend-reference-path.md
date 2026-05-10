# Root Env As Backend Reference Path

## Decision

Do not make a root `.env` file the canonical backend secrets or configuration path.

## Why This Is Out Of Scope

Root `.env` files are convenient for local tooling or app shells, but they are a poor reference path for backend deployables and secret governance. They hide secret shape, weaken explicit injection, and drift from cluster-compatible configuration.

The backend reference path should keep secret names, injection shape, and deployment expectations explicit.

## Reconsideration Criteria

`.env` can remain a local convenience lane when a task is explicitly app-shell or host-tool scoped.

Backend deployable work should prefer SOPS/age secret shape, `just sops-run`, `just sops-export-env` for single-VPS profiles, or explicit `APP_*` exports for short debug loops.

## Related Guidance

1. `docs/operations/secret-management.md`
2. `docs/operations/backend-config-policy.md`
3. `docs/architecture/harness-philosophy.md`
