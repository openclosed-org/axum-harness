# Multipass + K3s Pre-1.0 Rehearsal

> Scope: documented heavy local VM/K3s rehearsal. Do not treat this as a daily gate on the 8GB Mac mini.

## When To Use

Run this only when preparing for the 1.0 readiness pass or when validating behavior that K3d cannot represent well:

1. independent Ubuntu VM filesystems
2. independent containerd instances
3. systemd behavior
4. node-level failure boundaries
5. closer VPS-like networking

Do not run Colima/K3d and Multipass at the same time on the 8GB Mac mini.

## Resource Plan

| VM | CPU | Memory | Disk | Role |
| --- | ---: | ---: | ---: | --- |
| `k3s-1` | 2 | 2GB | 15GB | server |
| `k3s-2` | 1 | 1GB | 10GB | agent |
| `k3s-3` | 1 | 1GB | 10GB | agent |

## Setup

Install tools:

```bash
brew install --cask multipass
brew install kubectl
```

Create VMs:

```bash
multipass launch 24.04 --name k3s-1 --cpus 2 --memory 2G --disk 15G
multipass launch 24.04 --name k3s-2 --cpus 1 --memory 1G --disk 10G
multipass launch 24.04 --name k3s-3 --cpus 1 --memory 1G --disk 10G
```

Install server:

```bash
SERVER_IP=$(multipass info k3s-1 | awk '/IPv4/{print $2}')

multipass exec k3s-1 -- bash -lc \
"curl -sfL https://get.k3s.io | INSTALL_K3S_EXEC='server --node-name k3s-1 --write-kubeconfig-mode 644 --disable traefik --disable servicelb' sh -"
```

Join agents:

```bash
TOKEN=$(multipass exec k3s-1 -- sudo cat /var/lib/rancher/k3s/server/node-token)

for i in 2 3; do
  multipass exec k3s-$i -- bash -lc \
  "curl -sfL https://get.k3s.io | K3S_URL=https://$SERVER_IP:6443 K3S_TOKEN='$TOKEN' INSTALL_K3S_EXEC='agent --node-name k3s-$i' sh -"
done
```

Export kubeconfig:

```bash
multipass exec k3s-1 -- sudo cat /etc/rancher/k3s/k3s.yaml > k3s.yaml
sed -i '' "s/127.0.0.1/$SERVER_IP/g" k3s.yaml
export KUBECONFIG=$PWD/k3s.yaml

kubectl get nodes -o wide
```

## Rehearsal Targets

Before 1.0, validate at least:

1. `just gate-ci-single-node` passes before VM work starts.
2. `infra/kubernetes/addons` applies.
3. SOPS dev secrets can be reconciled into the cluster.
4. SurrealDB StatefulSet becomes Ready.
5. SurrealDB PVC survives pod restart.
6. Flux infrastructure Kustomization can reconcile.
7. Core stack can recover after VM stop/start.

## Cleanup

Stop VMs to release runtime resources:

```bash
multipass stop k3s-1 k3s-2 k3s-3
```

Delete and purge when done:

```bash
multipass delete k3s-1 k3s-2 k3s-3
multipass purge
```
