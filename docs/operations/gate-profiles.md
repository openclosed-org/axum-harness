# Gate Profiles

> Scope: local and CI gate profiles for this learning-oriented backend harness template.
> The project is not production-proven; evidence claims must stay tied to commands that actually ran.

## Strategy

The repository uses layered gates instead of one expensive environment that pretends to prove everything.

1. `ci-single-node` is the default GitHub/local single-node contract.
2. `local-k3d` is the low-resource Colima + K3d Kubernetes profile for an 8GB/256GB Mac mini.
3. `pre-1.0-multipass-k3s` is documented for a later VM/K3s rehearsal, but is not a daily gate.

This matches the current project shape: the value is in provider switching, platform declarations, secrets, workers, observability, and deployment semantics more than complex business logic.

## Profile: ci-single-node

Command:

```bash
just gate-ci-single-node
```

This profile is Kubernetes-free and should remain suitable for GitHub CI/CD.

It runs:

1. `just check-backend-primary`
2. `just test-backend-primary`
3. `just verify-backend-alternative`
4. `just test-backend-alternative`
5. `just sops-validate`
6. `just validate-platform`
7. `just validate-topology`
8. `just validate-observability`
9. `just boundary-check`
10. `just verify-generated-artifacts`

Passing this profile supports these claims:

1. `checked`: default backend-core compile/lint/fmt remains healthy.
2. `tested`: default single-node service path tests pass.
3. `tested`: SurrealDB alternative provider lane compiles and its tests pass.
4. `checked`: platform, topology, observability, secrets, boundaries, and generated artifacts match declared rules.

It does not prove:

1. Kubernetes manifests apply successfully.
2. PVC persistence works in K3s.
3. Flux reconciliation works.
4. Real VPS reboot recovery works.

## Profile: local-k3d

Commands:

```bash
just smoke-local-k3d
just gate-local-k3d
```

`smoke-local-k3d` is the fast Kubernetes smoke. It starts the local K3d cluster, applies infra, waits for SurrealDB, checks Service DNS, and verifies PVC persistence across a pod restart.

`gate-local-k3d` runs `gate-ci-single-node` first, then the K3d smoke, then secrets, worker source-level smoke, and observability config smoke.

Current limits are explicit:

1. Kubernetes app/worker Deployments are not fully declared yet, so worker smoke is source/config evidence, not pod runtime evidence.
2. Observability cluster components are not fully declared yet, so observability smoke is config validation, not a collector/Grafana runtime claim.
3. K3d nodes are containers, not independent VPS VMs.
4. K3d local storage does not prove cross-node PVC migration.

## Profile: pre-1.0-multipass-k3s

Command:

```bash
RUN_HEAVY_LOCAL_K3S=1 just gate-pre-1-0-multipass-k3s
```

This profile is intentionally protected and documented-only for now. Use it before the 1.0 readiness pass, not in daily development.

See `docs/operations/advanced-topology/k3s-local-multipass.md`.

## Recommended Use

Daily development:

```bash
just gate-ci-single-node
```

Provider or SurrealDB changes:

```bash
just gate-ci-single-node
just verify-surrealdb-compose-persistence
```

Kubernetes or infra changes:

```bash
just smoke-local-k3d
```

Stage-level local acceptance:

```bash
just gate-local-k3d
```

1.0 readiness rehearsal:

```bash
RUN_HEAVY_LOCAL_K3S=1 just gate-pre-1-0-multipass-k3s
```

## Evidence Language

Use conservative evidence terms:

1. `declared`: YAML, docs, or model says a capability should exist.
2. `checked`: a validator, drift check, or static gate checked shape or references.
3. `tested`: an executable test or local acceptance path ran.
4. `proven`: avoid this term unless a production-like rehearsal actually ran and its limits are documented.

This template should say "business code is intended to remain topology-agnostic" rather than "all distributed behavior is proven by configuration only".
