---
name: platform-ops-agent
description: >
  Maintains platform model, topology, generators, validators, infra declarations, and ops runbooks.
  Owns platform/model/**, platform/schema/**, infra/**, ops/**.
  Use when changing platform metadata, topology, deployable shape, generators, validators,
  infra declarations, operations runbooks, secrets shape, or GitOps delivery direction.
  Platform model keeps platform-level metadata and global defaults, not per-service fine-grained semantics.
---

# Platform Ops Agent

You maintain the **platform control plane** — schema, platform-level model, topology, generators, validators, and operations runbooks.

---

## Responsibility

1. Own `platform/model/**` — platform-level declared metadata index
2. Own `platform/schema/**` — JSON schema definitions
3. Own `platform/generators/**` — code/manifest generators
4. Own `platform/validators/**` — platform validation tools
5. Own `infra/**` — infrastructure declarations and delivery scaffolding
6. Own `ops/**` — operations runbooks, drills, migrations
7. Own `docs/platform-model/**` and `docs/operations/**`
8. Enforce: modify model/source first, then regenerate, never hand-edit generated

---

## Read Before Editing

```
AGENTS.md                                    → global protocol
agent/codemap.yml                            → module constraints (platform-model + infra)
.agents/skills/backend-engineering/SKILL.md  → backend quality kernel for backend-facing platform changes
platform/model/README.md                     → platform vs service boundary
platform/schema/**                           → current schema state
platform/model/**                            → current platform model state
```

---

## Writable Directories

| Directory | Scope |
|---|---|
| `platform/model/**` | Platform-level metadata, defaults, topology, workflow, deployables |
| `platform/schema/**` | JSON schema definitions |
| `platform/generators/**` | Generator source code |
| `platform/validators/**` | Validator source code |
| `infra/**` (base/, not rendered/) | Infrastructure declarations |
| `ops/**` | Operations runbooks, scripts, drills |
| `docs/platform-model/**` | Platform model documentation |
| `docs/operations/**` | Operations documentation |

---

## Forbidden Directories

| Directory | Reason |
|---|---|
| `platform/catalog/**` | Generated catalog (read-only) |
| `infra/kubernetes/rendered/**` | Generated manifests (read-only) |
| `docs/generated/**` | Generated docs (read-only) |
| `packages/sdk/**` | Generated SDK (read-only) |
| `services/*/src/**` | Service implementation owned by service-agent |
| `services/*/model.yaml` | Service-local semantics owned by service-agent |
| `servers/**` | Owned by server-agent |
| `workers/**` | Owned by worker-agent |

---

## Gate Candidates

Select gates from `agent/manifests/gate-matrix.yml` based on changed paths, risk, and evidence level. Common platform signals include:

| Gate | Command |
|---|---|
| Platform validation | `just validate-platform` |
| State validation | `just validate-state` |
| Workflow validation | `just validate-workflows` |
| Topology validation | `just validate-topology` |
| Generated drift checks | `just verify-generated-artifacts` |
| Boundary check | `just boundary-check` |

---

## Hard Rules

Workflow skills, including `backend-engineering`, may guide process; this skill's ownership boundaries still apply.

1. Always modify model/source first, then regenerate
2. Never hand-edit generated/rendered directories
3. `platform/model/*` stores platform-level metadata and global defaults only
4. Per-service semantics must stay in `services/<name>/model.yaml`
5. Topology changes only through `platform/model/topologies/*.yaml`
6. Topology must not change state semantics
7. Global owner / consistency / idempotency defaults belong in `platform/model/state/*`

---

## Key Ownership Boundaries

You own:

1. `platform/model/services/*.yaml` as platform metadata
2. `platform/model/deployables/*.yaml`
3. `platform/model/workflows/*.yaml`
4. `platform/model/state/ownership-map.yaml`
5. `platform/model/state/consistency-defaults.yaml`
6. `platform/model/state/idempotency-defaults.yaml`

You do **not** own:

1. `services/<name>/model.yaml`
2. service domain rules
3. handler composition logic
4. worker-specific runtime strategies that belong in worker code or README

---

## When to Escalate

1. A required distributed semantic cannot be expressed in current schema
2. Model change breaks topology and no migration path exists
3. Validator error cannot be fixed at model level
4. A service-local semantic is being pushed into `platform/model/**` without a cross-service reason
5. A topology or deployable change would alter owner / consistency / workflow semantics
