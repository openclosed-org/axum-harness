# Platform Features As Backend-Core Prerequisites

## Decision

Do not make Dapr, Cilium, Gateway API, full GitOps promotion, observability infrastructure, or cluster-only profiles required for backend-core development by default.

## Why This Is Out Of Scope

These capabilities can be valuable optional lanes or future profiles, but the default backend-core path must stay small, explicit, and runnable without advanced infrastructure.

Making platform features mandatory too early would turn the harness into a large platform demo instead of a backend-first template with topology-late growth seams.

## Reconsideration Criteria

A platform capability can move closer to default only when it has:

1. a narrow owner
2. a clear reason to exist in the default path
3. a migration story from the current backend path
4. path-scoped verification
5. no hidden frontend, desktop, cluster, or vendor prerequisite for backend-core work

## Related Guidance

1. `docs/architecture/north-star.md`
2. `docs/operations/local-dev.md`
3. `docs/operations/advanced-topology/gitops.md`
4. `docs/architecture/harness-philosophy.md`
