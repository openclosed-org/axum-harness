# Operations Runbooks

This directory contains template runbooks for common operational tasks. They are operator starting points, not proof of production readiness.

Derived projects must replace placeholder contacts, environment names, and cluster assumptions with their own operational facts.

## Available Runbooks

| Runbook | Purpose |
|---------|---------|
| [Backup & Restore](backup-restore.md) | Database backup and restore procedures |
| [Service Deployment](service-deployment.md) | How to deploy services |
| [Incident Response](incident-response.md) | Handling production incidents |
| [Scaling Guide](scaling.md) | Scaling services up and down |
| [Health Checks](health-checks.md) | Verifying system health |

## Quick Reference

### Start Local Infrastructure
```bash
cargo run -p repo-tools -- infra local up
```

### Run Migrations
```bash
cargo run -p repo-tools -- ops migrate --env local --direction up --dry-run
```

### Apply Dev Overlay To A Kubernetes Sandbox
```bash
kubectl apply -k infra/k3s/overlays/dev
```

For GitOps or production delivery, read `docs/operations/advanced-topology/gitops.md` and `infra/k3s/README.md` first. Do not treat the dev overlay command as a complete release process.

### Check Service Health
```bash
kubectl get pods -n app
kubectl describe pod <pod-name> -n app
kubectl logs <pod-name> -n app
```

### View Logs
```bash
# Local infrastructure
cargo run -p repo-tools -- infra local logs --follow

# Kubernetes
kubectl logs -n app -l app=api
```

## Emergency Contacts Template

- Infrastructure Team: replace with project-owned channel
- On-Call: replace with project-owned rotation
- Escalation: See [Incident Response](incident-response.md) after adapting it for the derived project
