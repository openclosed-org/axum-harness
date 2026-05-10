# ADR-009: Canonical Monolith-First, Topology-Late Backend Architecture

## Status
- [ ] Proposed
- [x] Accepted
- [ ] Deprecated
- [ ] Superseded

## Context

The repository already contains several strong architectural decisions:

- `services/*` are libraries, not processes.
- `counter-service` provides the most complete end-to-end reference chain.
- `event_outbox` exists as the unified event persistence surface.
- `outbox-relay` and `projector` already form a real async chain.
- `platform/model/*` exists to declare deployables, topology, and delivery shape.

At the same time, the repository still presents multiple overlapping architectural narratives:

1. the real `counter-service` backend reference chain
2. more aspirational service semantics in `tenant-service`
3. a deprecated-as-default `runtime` abstraction layer
4. deployable descriptions that sometimes mix current state with target state
5. duplicate or parallel event publication paths

This creates a practical risk:

- new developers and agents may copy the wrong pattern
- future refactors may preserve duplicate abstractions
- target-state descriptions may be mistaken for already-landed architecture
- topology evolution may become a semantic rewrite instead of a runtime switch

The repository needs one canonical statement of what the backend architecture is today, what must be introduced now, and what should remain deferred until a later topology phase.

## Decision

We define the canonical backend architecture as:

**monolith-first, topology-late, service-library-centered, outbox-driven, replay-capable**.

This means:

1. Business semantics live in `services/*` libraries.
2. Synchronous entrypoints live in `servers/*`.
3. Asynchronous entrypoints live in `workers/*`.
4. Topology changes must not require rewriting service semantics.
5. The primary path for future distributed evolution is:
   - shared contracts
   - unified outbox
   - replayable projections
   - explicit composition roots
6. The repository does **not** assume that a complete runtime abstraction layer is the primary foundation for topology evolution.

### Canonical Reference Chain

`counter-service` is the single default backend reference chain until another service reaches the same closure level.

Its current canonical path is:

```text
service library
  -> web-bff
  -> CAS + idempotency + unified outbox
  -> outbox-relay worker
  -> projector worker
  -> replay / rebuildable read model
```

`tenant-service` may remain a semantics reference, but not a default copy target until its implementation closure matches `counter-service`.

### Canonical Event Path

The repository standardizes on a single event publication chain:

```text
service mutation
  -> event_outbox
  -> outbox-relay
  -> event backbone
  -> projector / downstream consumers
```

Consequences of this decision:

1. `event_outbox -> outbox-relay` is the canonical relay path.
2. Shared event contracts in `packages/contracts/events` are the canonical cross-boundary event language.
3. Alternative outbox publishers or duplicate publication flows must be treated as non-canonical until explicitly promoted.

### Boundary Rules

The repository standardizes the following boundary rules:

1. Cross-process, cross-deployable, HTTP-facing, or message-facing DTOs/events/error codes must live in `packages/contracts/**`.
2. Service-local orchestration helpers, traits, and internal context types may stay inside service crates.
3. Events written into `event_outbox` must be shared contracts, not service-local-only event types.
4. Concrete adapters must move toward composition roots and shared adapter packages; service crates should trend toward pure business libraries.

### Reliability Rules

The repository treats the following as required before promoting workers beyond the single-replica/default reference path:

1. shared checkpoint semantics
2. shared dedupe/idempotency semantics for workers
3. explicit worker ownership / lease strategy
4. replay-safe processing assumptions

Local-file checkpointing and in-process dedupe may remain as dev fallback, but are not multi-replica production semantics.

### Deferred Capabilities

The following remain deferred unless a real execution path requires them:

1. expanding the full `packages/runtime` 8-port architecture as the default backend path
2. Dapr or sidecar-first runtime integration
3. full durable workflow platformization for non-reference chains
4. edge gateway feature expansion beyond the currently implemented proxy path
5. promoting stub services/workers into the default architecture reference set

These may be described in ADRs, platform model, or tooling notes, but should not distort the current canonical architecture.

## Consequences

### What becomes easier

- new agents and developers can identify the default backend path quickly
- future topology changes are more likely to remain runtime/composition changes instead of semantic rewrites
- duplicate message and runtime abstractions become easier to remove
- platform model can regain credibility as a declared control-plane index
- worker reliability can be improved in one place instead of per-worker copy/paste

### What becomes harder

- some existing target-state descriptions must be downgraded or clarified
- some crates and docs will need renaming, narrowing, or de-emphasizing
- future architecture work must be more disciplined about not introducing parallel abstractions prematurely

### Trade-offs

- **Pros**: clearer default path, lower refactor risk, stronger topology-late story, more truthful documentation
- **Cons**: less room for speculative platform abstraction in the short term, more pressure to converge existing drift

## References

- `docs/operations/counter-service-reference-chain.md`
- `docs/adr/002-services-are-libraries-not-processes.md`
- `docs/archive/adr/003-runtime-abstraction-direct-plus-dapr.md`
- `docs/adr/007-workers-first-async-architecture.md`
- `services/counter-service/src/application/service.rs`
- `services/counter-service/src/infrastructure/libsql_adapter.rs`
- `servers/bff/web-bff/src/state.rs`
- `workers/outbox-relay/src/main.rs`
- `workers/projector/src/main.rs`
- `packages/contracts/events/src/lib.rs`
- `workers/outbox-relay/src/publish/mod.rs`
