# SOPS + age Setup

This guide documents the current repository command surface for SOPS/age secrets. It is not a general SOPS tutorial.

Backend deployables consume ordinary environment variables. The canonical source is encrypted SOPS YAML under `infra/security/sops/<env>/*.enc.yaml`; `.env` files are not the backend reference path.

## Setup

Install the repository toolchain:

```bash
mise install
```

Generate and inspect the local age key:

```bash
just sops-gen-age-key
just sops-show-age-key
```

Add the public key to the root `.sops.yaml` creation rules, then validate:

```bash
just sops-validate
```

The default key path is `~/.config/sops/age/key.txt`. `repo-tools` also honors `SOPS_AGE_KEY_FILE` when it is set.

## Editing Secrets

Use the repository commands:

```bash
just sops-edit web-bff dev
just sops-encrypt-dev web-bff
```

Encrypted files live at `infra/security/sops/<env>/<deployable>.enc.yaml`. Sanitized `*.example.yaml` files under `infra/security/sops/templates/**` are committed so users can see the secret shape. Local plaintext `*.yaml` files in the same tree are ignored by git and must not be committed with real values.

## Runtime Injection

### Local Process

Run a backend process with decrypted variables injected directly into the child process:

```bash
just sops-run web-bff dev 'cargo run -p web-bff' local_real
```

The last argument is the database capability state, not a secret nickname. Current supported states are:

| state | Runtime behavior |
|---|---|
| `local_real` | Default local durable embedded DB path; does not merge `counter-db-credentials`. |
| `external_single_node` | Merges `counter-db-credentials` for external single-node DB testing. |
| `external_distributed` | Merges `counter-db-credentials` for distributed DB testing. |

`disabled` and `local_mock` fail fast for this backend reference chain because it requires durable storage.

### Single-VPS Binary

Export a transient host env-file for systemd:

```bash
just sops-export-env web-bff dev systemd-binary /run/axum-harness/web-bff.env
```

Systemd consumes it with:

```ini
EnvironmentFile=/run/axum-harness/web-bff.env
```

The export command writes `0600` permissions and refuses to export an unknown deployable that lacks its own encrypted secret file.

### Podman

Export the same secret shape for Podman:

```bash
just sops-export-env web-bff dev podman /run/axum-harness/web-bff.env
podman run --rm --env-file /run/axum-harness/web-bff.env <image>
```

SOPS/age runs on the host control plane. Application containers do not need SOPS or age installed by default.

### Kustomize / Flux

For cluster paths:

```bash
just sops-reconcile dev
just sops-setup-flux-secret
```

Flux consumes the same encrypted files through SOPS decryption.

## Validation

```bash
just sops-validate
just sops-verify-counter-db-credentials dev
cargo run -p repo-tools -- secrets decrypt-env infra/security/sops/dev/web-bff.enc.yaml
```

## Key Rotation

1. Generate a new age key.
2. Add the new public key to root `.sops.yaml`.
3. Re-encrypt affected `infra/security/sops/<env>/*.enc.yaml` files.
4. Validate with `just sops-validate`.

## Rules

1. Never commit plaintext secrets.
2. Never make `.env` the backend deployable reference path.
3. Do not put `sops` or `age` into application containers unless a concrete profile explicitly needs in-container decryption.
4. Treat exported env-files as host secrets; keep them transient and permissioned `0600`.
5. Do not claim a profile is production-ready without matching runbook and gate evidence.
