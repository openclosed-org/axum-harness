# Skills

Repository skills are triggerable context modules. They should be short, current, and boundary-safe.

For the full context-flow model, read `agent/README.md`. For writing or reviewing a skill, read `agent/skill-authoring.md`.

## Ownership Skills

Ownership skills define writable paths and responsibility boundaries.

| Skill | Owns |
|---|---|
| `planner` | `AGENTS.md`, `agent/**`, non-generated `docs/**`, shared repo-control tooling |
| `platform-ops-agent` | `platform/model/**`, `platform/schema/**`, `platform/generators/**`, `platform/validators/**`, `infra/**`, `ops/**` |
| `contract-agent` | `packages/contracts/**`, `docs/contracts/**`, `verification/contract/**` |
| `service-agent` | `services/**`, service-level `fixtures/**`, service-level `verification/**` |
| `server-agent` | `servers/**`, protocol adaptation and server entrypoints |
| `worker-agent` | `workers/**`, replay/resilience/topology verification lanes |
| `app-shell-agent` | `apps/**`, `packages/ui/**`, `verification/e2e/**` |

## Workflow Skills

Workflow skills define how to perform engineering activities. They guide process but do not override ownership boundaries.

| Skill | Use When |
|---|---|
| `backend-engineering` | Applying the default backend quality lens |
| `harness-diagnose` | Debugging failing gates, regressions, flakes, or suspicious runtime behavior |
| `harness-tdd` | Adding or changing backend behavior test-first |
| `harness-grill` | Resolving ambiguous plans, scope, or terminology before implementation |
| `harness-architecture-review` | Reviewing seams, coupling, depth, or testability |
| `harness-zoom-out` | Mapping an unfamiliar area before editing |
| `skill-authoring` | Adding, editing, or reviewing repository skills |
