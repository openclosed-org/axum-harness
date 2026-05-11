# Platform control plane
#
# This file owns platform-model specific validation, generation, and inventory
# commands. Canonical entrypoints use explicit verbs (`validate-*`, `generate-*`,
# `verify-*`) consistently across help, docs, and automation.

# ── Model Validation ──────────────────────────────────────────

# Validate all platform models against JSON schemas
validate-platform:
    @echo "Validating platform models..."
    cargo run -p platform-validator -- --platform-dir platform

# Validate platform models with JSON output
validate-platform-json:
    cargo run -p platform-validator -- --platform-dir platform --format json

# Validate service ownership and state semantics alignment
validate-state MODE='warn':
    cargo run -p repo-tools -- validate-state --mode {{MODE}}

# Validate workflow completeness beyond schema shape
validate-workflows MODE='warn':
    cargo run -p repo-tools -- validate-workflows --mode {{MODE}}

# canonical platform catalog generation entrypoint
generate-platform-catalog:
    @echo "Generating platform catalog..."
    cargo run -p platform-generator -- --platform-dir platform
    @echo "Platform catalog generated"

# Clean generated platform catalog
clean-platform:
    @echo "Cleaning generated platform catalog..."
    rm -rf platform/catalog/
    @echo "Catalog cleaned"

# Regenerate platform catalog (clean + generate)
regenerate-platform:
    just clean-platform
    just generate-platform-catalog

# ── Dependency Graph Validation ───────────────────────────────

# Validate dependency graph for cycles and broken references
validate-deps:
    @echo "Checking platform model dependency graph..."
    cargo run -p dependency-validator -- --platform-dir platform

# ── Contract Drift Detection ──────────────────────────────────

# canonical entrypoint: platform-model drift against contracts
validate-contract-drift:
    @echo "Checking contract drift..."
    cargo run -p contract-drift-detector -- --platform-dir platform

# ── Topology Validation ──────────────────────────────────────

# Validate deployment topology completeness
validate-topology:
    @echo "Checking deployment topology..."
    cargo run -p topology-validator -- --platform-dir platform

# Verify replay/rebuild reference hooks remain available
verify-replay MODE='warn':
    cargo run -p repo-tools -- verify-replay --mode {{MODE}}

# Verify counter delivery admission path remains executable
verify-counter-delivery MODE='warn':
    cargo run -p repo-tools -- verify-counter-delivery --mode {{MODE}}

# ── Security Validation ──────────────────────────────────────

# Validate security configuration
validate-security:
    @echo "Checking security configuration..."
    cargo run -p security-validator -- --platform-dir platform

# ── Observability Validation ─────────────────────────────────

# Validate observability configuration
validate-observability:
    @echo "Checking observability configuration..."
    cargo run -p observability-validator -- --platform-dir platform

# ── SDK Generation (Pending Implementation) ───────────────────

# Generate TypeScript and Rust SDKs from platform models
# TODO: Implement SDK generators in platform/generators/
gen-sdk:
    @echo "WARNING: SDK generation not yet implemented"
    @echo "  TypeScript SDK: packages/sdk/typescript/"
    @echo "  Rust SDK:       packages/sdk/rust/"
    @echo "  See: docs/architecture/repo-layout.md for planned structure"

# Clean generated SDKs
clean-sdk:
    cargo run -p repo-tools -- clean-sdk

# ── Golden Baseline Verification ──────────────────────────────

# canonical entrypoint: generated artifact baseline verification
verify-generated-artifacts:
    cargo run -p repo-tools -- verify-generated-artifacts

# Commit golden baselines for ALL generated artifacts
commit-golden-baseline:
    cargo run -p repo-tools -- commit-golden-baseline

# ── Full Health Check ─────────────────────────────────────────

# Full platform doctor (all validations)
platform-doctor:
    @echo "Running platform health check..."
    @echo ""
    @echo "1. Validating models..."
    just validate-platform || (echo "Model validation failed"; exit 1)
    @echo ""
    @echo "2. Checking dependencies..."
    just validate-deps || (echo "Dependency check failed"; exit 1)
    @echo ""
    @echo "3. Checking state semantics..."
    just validate-state strict || (echo "State validation failed"; exit 1)
    @echo ""
    @echo "4. Checking workflows..."
    just validate-workflows strict || (echo "Workflow validation failed"; exit 1)
    @echo ""
    @echo "5. Checking contract drift..."
    just validate-contract-drift || true
    @echo ""
    @echo "6. Checking topology..."
    just validate-topology || (echo "Topology check failed"; exit 1)
    @echo ""
    @echo "7. Checking security..."
    just validate-security || (echo "Security check failed"; exit 1)
    @echo ""
    @echo "8. Checking observability..."
    just validate-observability || (echo "Observability check failed"; exit 1)
    @echo ""
    @echo "9. Verifying replay hooks..."
    just verify-replay strict || (echo "Replay verification failed"; exit 1)
    @echo ""
    @echo "10. Generating catalog..."
    just generate-platform-catalog || (echo "Catalog generation failed"; exit 1)
    @echo ""
    @echo "11. Verifying counter delivery admission..."
    just verify-counter-delivery strict || (echo "Counter delivery verification failed"; exit 1)
    @echo ""
    @echo "12. Verifying golden baseline..."
    just verify-generated-artifacts || true
    @echo ""
    @echo "Platform health check passed"

# ── Inventory Commands ────────────────────────────────────────

# List all services defined in platform model
platform-services:
    cargo run -p repo-tools -- platform-services

# List all deployables defined in platform model
platform-deployables:
    cargo run -p repo-tools -- platform-deployables

# List all resources defined in platform model
platform-resources:
    cargo run -p repo-tools -- platform-resources

# List all capabilities defined in platform model
platform-capabilities:
    cargo run -p repo-tools -- platform-capabilities
