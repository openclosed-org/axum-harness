# SOPS — Secrets management (SOPS + age + Flux)
#
# Backend binaries consume standard environment variables.
# Environment variables are injected via sops-run, transient host env-files, or
# SOPS/Kustomize/Flux, NOT from committed .env files.
#
# Quick reference:
#   just sops-gen-age-key          # Generate age key (first-time setup)
#   just sops-show-age-key         # Show public key
#   just sops-encrypt-dev <name>   # Encrypt secrets for dev
#   just sops-edit <name> <env>    # Edit encrypted secrets
#   just sops-run <name>           # Run binary with decrypted env vars
#   just sops-export-env <name>    # Write transient env-file for systemd/podman
#   just sops-reconcile <env>      # Apply secrets to cluster

# ── Age Key Management ──────────────────────────────────────

# Generate a new age key pair
sops-gen-age-key:
    cargo run -p repo-tools -- dev age-key generate

# Show age public key
sops-show-age-key:
    cargo run -p repo-tools -- dev age-key show

# ── Secrets Encryption ──────────────────────────────────────

# Encrypt secrets for dev environment
sops-encrypt-dev deployable='web-bff':
    cargo run -p repo-tools -- secrets encrypt --deployable "{{deployable}}" --env dev

# Encrypt secrets for staging environment
sops-encrypt-staging deployable='web-bff':
    cargo run -p repo-tools -- secrets encrypt --deployable "{{deployable}}" --env staging

# ── Secrets Editing ─────────────────────────────────────────

# Edit encrypted secrets (opens editor with decrypted content)
sops-edit deployable='web-bff' env='dev':
    cargo run -p repo-tools -- secrets edit --deployable "{{deployable}}" --env "{{env}}"

# ── Local Development (No Cluster) ──────────────────────────

# Run binary with SOPS-decrypted environment variables
# This is the ONLY allowed non-cluster dev path
# Does NOT create .env files
sops-run deployable='web-bff' env='dev' cmd='':
    @cargo run -p repo-tools -- secrets run --deployable "{{deployable}}" --env "{{env}}" -- {{cmd}}

# Export SOPS-decrypted variables to a transient host env-file for systemd or Podman.
# The output is created with 0600 permissions and must not be committed.
sops-export-env deployable='web-bff' env='dev' profile='systemd-binary' output='/run/axum-harness/web-bff.env':
    @cargo run -p repo-tools -- secrets export-env --deployable "{{deployable}}" --env "{{env}}" --profile "{{profile}}" --output "{{output}}"

# ── Cluster Reconciliation ──────────────────────────────────

# Apply SOPS-encrypted secrets to k3s cluster
sops-reconcile env='dev':
    @cargo run -p repo-tools -- secrets reconcile --env "{{env}}"

# ── Flux Integration ────────────────────────────────────────

# Create Flux SOPS secret (age key in cluster)
sops-setup-flux-secret:
    cargo run -p repo-tools -- secrets setup-flux-secret

# ── Validation ──────────────────────────────────────────────

# Validate SOPS configuration
sops-validate:
    @cargo run -p repo-tools -- secrets validate

# Verify that shared counter DB secret is ready for independent worker pods
sops-verify-counter-shared-db env='dev':
    @cargo run -p repo-tools -- secrets verify-counter-shared-db --env "{{env}}"
