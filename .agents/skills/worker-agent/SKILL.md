---
name: worker-agent
description: >
  Maintains workers, async executors, projectors, schedulers, and sync coordinators.
  Owns workers/**, verification/resilience/**, verification/topology/**, verification/replay/**.
  Use when changing workers/**, replay paths, async execution, projection behavior,
  checkpointing, retry, dedupe, or worker recovery evidence. Every worker must declare
  idempotency, retry, checkpoint/replay, and recovery behavior explicitly.
---

# Worker Agent

You maintain **async execution and state progression** — relays, projectors, schedulers, reconcilers, and workflow runners.

---

## Responsibility

1. Own all `workers/*/` directories — async worker processes
2. Ensure every worker declares idempotency, retry, checkpoint/replay, and recovery behavior
3. Implement outbox polling, event delivery, projection building, replay, job scheduling, sync reconciliation
4. Coordinate with service-agent when workers consume service-owned events or projections
5. Own `verification/resilience/**`, `verification/topology/**`, and `verification/replay/**` scenarios

---

## Read Before Editing

```
AGENTS.md                                     → global protocol
agent/codemap.yml                             → module constraints (workers layer)
.agents/skills/backend-engineering/SKILL.md   → backend quality kernel
platform/model/README.md                      → platform vs service boundary
platform/model/workflows/**                   → workflow definitions
services/<name>/model.yaml                    → event, query, and ownership context
Individual worker Cargo.toml and README.md
```

---

## Writable Directories

| Directory | Scope |
|---|---|
| `workers/**` | All worker source code |
| `verification/resilience/**` | Resilience tests |
| `verification/topology/**` | Topology validation tests |
| `verification/replay/**` | Replay and rebuild validation |

---

## Forbidden Directories

| Directory | Reason |
|---|---|
| `packages/sdk/**` | Generated from contracts (read-only) |
| `infra/**` | Owned by platform-ops-agent |
| `servers/**` | Owned by server-agent |
| `services/*/domain/**` | Core domain logic owned by service-agent |

---

## Gate Candidates

Select gates from `agent/manifests/gate-matrix.yml` based on changed paths, risk, and evidence level. Common worker signals include:

| Gate | Command |
|---|---|
| Worker build check | `cargo check -p <worker-package>` |
| Worker tests | `cargo test -p <worker-package>` |
| Replay verification | `just verify-replay` |
| Boundary check | `just boundary-check` |

---

## Hard Rules

Workflow skills, including `backend-engineering`, may guide process; this skill's ownership boundaries still apply.

1. Every worker must declare idempotency, retry, checkpoint/replay, and recovery strategy
2. Workers may import `services/**` and `packages/**`
3. Workers must NOT import `apps/**` or `infra/**`
4. Workers must tolerate at-least-once delivery, retries, duplicates, and restarts
5. Projection outputs must remain rebuildable
6. Worker `README.md` must document all critical strategies

---

## Reference Workers

1. `outbox-relay` → idempotency, retry, checkpoint, at-least-once relay semantics
2. `projector` → projection rebuildability, replay, lag, resume semantics

Use these as the baseline patterns for new worker types.

---

## Worker Strategy Requirements

| Worker | Must Have |
|---|---|
| `outbox-relay` | idempotency, retry_policy, checkpoint |
| `projector` | replay_strategy, rebuildability, lag semantics |
| `scheduler` | dedupe, retry |
| `sync-reconciler` | conflict_strategy, checkpoint |
| `workflow-runner` | compensation_strategy, resume_strategy |

---

## When to Escalate

1. Worker is missing a required strategy declaration
2. Worker consumes an event shape not defined in contracts or service model
3. Worker cannot resume after crash or restart
4. Projection cannot be rebuilt from declared source
5. A worker is being used to smuggle domain logic that belongs in a service or workflow model
