# ADR-004: Deployment Profiles

## Status
- [ ] Proposed
- [x] Accepted
- [ ] Deprecated
- [ ] Superseded

## Context

The repository needs a small deployment vocabulary that supports topology-late growth without turning every infrastructure option into a backend-core prerequisite.

Previous docs emphasized K3s and Flux as the delivery direction. That was useful, but too narrow: it made cluster delivery look like the only serious production path and did not clearly separate deployment profiles from capability backends.

The current backend reference chain remains:

```text
service -> contracts -> server -> outbox -> relay -> projector
```

Deployment profiles must preserve that semantic chain. They may change process packaging, host lifecycle, container runtime, orchestration, and secret injection mechanics, but they must not rewrite business semantics.

## Decision

The accepted production profile doctrine is:

1. `systemd-binary`
2. `podman`
3. `k3s-ha`

These are deployment profiles, not capability backends.

Capability backends and resources such as NATS, Valkey, MinIO, OpenFGA, SurrealDB, libSQL/Turso, observability collectors, or GitOps controllers may be used by a profile, but they are not themselves production profiles.

All profiles follow a build-outside-runtime rule: first-party Rust code is built on a developer machine or CI worker, packaged as a binary or prebuilt runtime image, and then installed into the runtime environment. VPS hosts, Podman containers, and K3s nodes should not compile first-party Rust code during deployment.

Single-host profiles also use resource presets:

1. `lite`: low-resource MVP shape; keeps backend capabilities available through embedded, managed, local, or in-process backends with explicit semantic limits.
2. `standard`: growing single-VPS shape; turns on selected official resource containers only when workload pressure justifies them.
3. `full`: full single-host resource set; useful before `k3s-ha`, but not the minimum VPS baseline.

Distributed resources are opt-in capability backends, not baseline taxes. A lightweight adapter may satisfy the same application capability, but its durability, process scope, restart behavior, and cross-process limits must be documented and tested at the claimed level.

## Profile Meanings

### `systemd-binary`

`systemd-binary` means Linux host operation with built backend binaries managed by systemd units.

This profile is for the smallest production-minded runtime shape: explicit binaries, explicit service lifecycle, explicit environment injection, explicit logs, and host-level runbooks. Secret injection uses SOPS/age on the host to produce a transient `0600` env-file consumed by systemd `EnvironmentFile`, not a committed `.env` file. The binary is built before it reaches the VPS.

It should not imply that workers, outbox, contracts, or secret discipline disappear. It only changes packaging and process supervision.

### `podman`

`podman` means single-host or small-host container operation using Podman-compatible container lifecycle.

This profile primarily manages official resource containers such as SurrealDB, NATS, Valkey, MinIO, OpenFGA, Rauthy, and observability components. It should consume the same deployable configuration and secret shape as other profiles while keeping container runtime details outside application code. The default secret path decrypts on the host and passes a transient env-file through `podman --env-file` or equivalent Quadlet/systemd wiring when a first-party prebuilt image is used; application images do not need to contain SOPS or age.

Podman is not the default place to compile or package first-party Rust services. If a team chooses first-party app containers, those images should be built from already-created artifacts on a developer machine or CI runner, then pushed, saved/loaded, or otherwise transferred to the runtime host.

Podman support does not make Docker, Compose, or local auth stacks part of backend-core by default.

### `k3s-ha`

`k3s-ha` means cluster operation with multiple server nodes before strong availability claims are made.

K3d and single-node K3s are useful local shape checks, but they are not HA proof. Before 1.0 can claim `k3s-ha` readiness, the repository needs real multi-node evidence, including secret delivery, deployable health, worker recovery, rollback or reconciliation behavior, and storage/restart behavior at the claimed level. K3s nodes pull or receive prebuilt artifacts/images; they do not compile first-party Rust code.

`k3s-ha` starts at 3 server nodes before HA claims. Move to 5 or 7 only when failure-domain, quorum, availability, or load evidence justifies the added operational cost.

## Evidence Language

The current evidence level is intentionally conservative:

1. `systemd-binary`: accepted doctrine; SOPS/age host env-file export and a macOS host-process smoke are checked, while Linux systemd unit lifecycle/runbook evidence is still pending.
2. `podman`: resource-container direction is accepted doctrine; partial local compose evidence and SOPS/age host env-file export are checked. The previous macOS web-bff image smoke only proves the Dockerfile can build and run locally; it is not the recommended single-VPS deployment path and is not production Quadlet proof.
3. `k3s-ha`: K3s/K3d/GitOps landing points exist, but HA evidence is not proven.

Do not describe any profile as production-ready without matching executable evidence.

## Consequences

### What becomes easier

1. README and roadmap can describe production direction without overclaiming readiness.
2. Documentation can distinguish deployment profiles from optional capability backends.
3. Template users can choose a host/container/cluster path without changing service semantics.
4. Agents have a smaller vocabulary for deployment planning.
5. Low-resource VPS users can start with lightweight adapters and only pay for distributed resources when the workload requires them.

### What becomes more difficult

1. Profile-specific runbooks and gates must be earned separately.
2. Docs must avoid turning local K3d, compose, or target-state manifests into production proof.
3. Future profile work must keep deployment mechanics out of service libraries.
4. Lightweight adapters must not overclaim distributed semantics; each downgrade needs clear durability and scope language.

## References

- `AGENTS.md`
- `docs/architecture/north-star.md`
- `docs/architecture/harness-philosophy.md`
- `docs/adr/009-canonical-monolith-first-topology-late-backend.md`
- `docs/operations/gate-profiles.md`
- `docs/operations/advanced-topology/k3d-local.md`
- `docs/operations/advanced-topology/k3s-local-multipass.md`
- `infra/docker/compose/core.yaml`
- `infra/k3s/README.md`
