# Flux GitOps Configuration

This directory contains Flux CD configuration for the repository's GitOps delivery shape. It is a declared cluster-profile landing zone, not proof that staging or production delivery is fully verified.

Do not edit generated or rendered manifests to change behavior. Prefer `infra/k3s/**`, SOPS templates, platform model sources, validators, and gates as the current evidence path.

## Structure

```
infra/gitops/flux/
├── apps/                  # Application definitions
│   ├── api.yaml           # API/BFF delivery wiring
│   ├── gateway.yaml       # Edge gateway delivery wiring
│   ├── outbox-relay-worker.yaml
│   └── projector-worker.yaml
├── infrastructure/        # Infrastructure components
│   ├── nats.yaml          # NATS message broker
│   ├── valkey.yaml        # Valkey cache (Redis-compatible)
│   └── minio.yaml         # MinIO object storage (S3-compatible)
├── policies/              # Policy definitions
│   ├── namespace.yaml     # Namespace and ServiceAccount configuration
│   ├── rbac.yaml          # RBAC Role and RoleBinding
│   └── quotas.yaml        # ResourceQuota and LimitRange
└── TOPOLOGY-MAPPING.md    # Topology ↔ Flux configuration mapping
```

## Bootstrap Flux

### 1. Install Flux CLI

```bash
# macOS
brew install fluxcd/tap/flux

# Or using curl
curl -s https://fluxcd.io/install.sh | sudo bash
```

### 2. Bootstrap Flux on Cluster

```bash
# Bootstrap with GitHub repository
flux bootstrap github \
  --owner=<your-github-org> \
  --repository=axum-harness \
  --branch=main \
  --path=infra/gitops/flux \
  --read-write-key \
  --personal

# Or with generic Git server
flux bootstrap git \
  --url=ssh://git@github.com/<org>/<repo>.git \
  --branch=main \
  --path=infra/gitops/flux
```

### 3. Verify Bootstrap

```bash
flux check
flux get sources git
flux get kustomizations
```

## Deploy New Service

1. Create Kustomization file in `apps/` directory
2. Commit and push to repository
3. Flux will automatically apply changes

```bash
git add infra/gitops/flux/apps/my-service.yaml
git commit -m "Add my-service deployment"
git push
```

## Monitor Deployments

```bash
# Get all Kustomizations
flux get kustomizations

# Get specific Kustomization
flux get kustomization api

# View reconciliation logs
flux logs -f

# Suspend automatic reconciliation
flux suspend kustomization api

# Resume reconciliation
flux resume kustomization api

# Trigger manual reconciliation
flux reconcile kustomization api --with-source
```

## Rollback

```bash
# View Kustomization history
flux get kustomization api --show-events

# Rollback to previous revision (requires suspension first)
flux suspend kustomization api
# Revert commit in Git
git revert <bad-commit>
git push
flux resume kustomization api
```

## Current Status

### Declared / Checked Shape

- Core infrastructure Kustomizations exist for NATS, Valkey, and MinIO.
- Application Kustomizations exist for API/BFF, web shell, gateway, outbox relay worker, and projector worker wiring.
- SOPS decryption is wired into the relevant Flux Kustomizations.
- Namespace, RBAC, and ResourceQuota manifests exist.

### Not Proven By This README

- Staging and production overlays are not automatically equivalent to verified runtime environments.
- Independent `counter-service` deployment, promotion, rollback, and drift handling still need executable delivery evidence before being described as complete.
- Health checks, target resource names, rendered paths, and image references must be checked against current manifests and gates before claiming readiness.
- Additional workers, observability, cache variants, and network policies remain topology-specific follow-up work unless a concrete file and gate prove otherwise.

## See Also

- [Flux Documentation](https://fluxcd.io/flux/)
- [SOPS Integration](../../security/sops/SETUP.md)
- [Kubernetes Manifests](../../kubernetes/)
- [Topology Mapping](./TOPOLOGY-MAPPING.md)
