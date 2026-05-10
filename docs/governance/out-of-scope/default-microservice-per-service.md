# Default Microservice Per Service

## Decision

Do not make every `services/*` package a default independently deployed process.

## Why This Is Out Of Scope

This repository is topology-late. Service boundaries are semantic and architectural first; process boundaries are deployment choices that need additional runtime, configuration, secrets, observability, and operational evidence.

Defaulting every service to an independent process would add distributed-system cost before the default backend path needs it. It would also weaken the template-user path and invite agents to implement process entrypoints inside service libraries.

## Reconsideration Criteria

A service can be promoted toward an independent deployable path when the repository has executable evidence for:

1. runtime entrypoint outside `services/**`
2. deployable wiring
3. secret injection shape
4. overlay and GitOps path when cluster-scoped
5. health and lifecycle behavior
6. gates proving the path at the claimed evidence level

## Related Guidance

1. `docs/adr/002-services-are-libraries-not-processes.md`
2. `docs/adr/009-canonical-monolith-first-topology-late-backend.md`
3. `docs/architecture/north-star.md`
