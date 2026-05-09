# Local K3d Profile

> Scope: low-resource local Kubernetes verification using Colima + K3d on macOS.

## Resource Target

For an 8GB RAM / 256GB disk Mac mini, use:

```bash
colima start --cpu 4 --memory 4 --disk 30 --runtime docker --dns 1.1.1.1 --dns 8.8.8.8
```

The disk value is the VM disk limit, not immediate host disk usage.
The explicit DNS resolvers avoid Colima VM resolver states where Docker image pulls try `[::1]:53` and fail.

Do not make `docker context use colima` a hard prerequisite. The recipes prefer a `colima` Docker context when present and otherwise use Colima's Docker socket through `DOCKER_HOST`.

## Tooling

Install project-managed CLIs:

```bash
mise install
```

`mise` manages `kubectl`, `helm`, `k3d`, and the Docker CLI for this repository.

Install the macOS runtime/VM manager separately:

```bash
brew install colima
```

Colima is intentionally kept as a host-level runtime dependency. It provides the Docker-compatible daemon that K3d uses; the mise-managed `docker` tool is only the client CLI.

## Cluster Shape

The default local cluster name is `axh-local`:

```bash
just k3d-up
```

Equivalent shape:

```bash
k3d cluster create axh-local \
  --servers 1 \
  --agents 2 \
  --api-port 127.0.0.1:6443 \
  --port "8080:80@loadbalancer" \
  --k3s-arg "--disable=traefik@server:*" \
  --k3s-arg "--disable=servicelb@server:*"
```

Traefik and ServiceLB are disabled to keep the resource profile small. Add ingress/load balancer checks later only when the project has a declared need.

## Smoke Gate

Run:

```bash
just smoke-local-k3d
```

This verifies:

1. K3d cluster exists with three Ready nodes.
2. `app` and `app-dev` namespaces exist.
3. dev SOPS secrets are reconciled.
4. `infra/kubernetes/addons` applies.
5. SurrealDB StatefulSet becomes Ready.
6. SurrealDB Service DNS is reachable in-cluster.
7. SurrealDB PVC survives a pod restart.

## Full Local K3d Gate

Run:

```bash
just gate-local-k3d
```

This first runs `just gate-ci-single-node`, then the K3d smoke, then extra local checks for secrets, worker config, and observability config.

## Cleanup

Delete the K3d cluster:

```bash
just k3d-down
```

Inspect disk usage:

```bash
just k3d-disk-usage
```

Stop Colima when done:

```bash
colima stop
```

Use destructive cleanup only intentionally:

```bash
docker system prune -a --volumes
colima delete
```

## Known Limits

1. K3d nodes are Docker containers, not independent VMs.
2. K3d local storage proves pod restart persistence, not cross-machine storage migration.
3. Current Kubernetes manifests mainly declare infrastructure addons; app and worker Deployments are not yet complete pod-level runtime evidence.
4. Observability smoke is currently validator/config evidence until collector stack manifests are declared.

## Docker, Colima, And Podman

Use Colima's Docker runtime for this K3d profile:

```bash
colima start --cpu 4 --memory 4 --disk 30 --runtime docker --dns 1.1.1.1 --dns 8.8.8.8
```

Podman remains supported for local compose-style infrastructure such as `infra/docker/compose/core.yaml`, but it is not the default runtime for `local-k3d`.

Reason: K3d is built around the Docker API and Docker-style network/volume behavior. Podman can expose a Docker-compatible socket in some setups, but that path is more compatibility-sensitive and is not the low-risk default for this repository.

The recipes prefer a `colima` Docker context when present. If Colima is running but the Docker context was not created, they fall back to Colima's socket at `~/.colima/default/docker.sock` through `DOCKER_HOST`.
