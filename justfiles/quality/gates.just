# Gate profiles — lifecycle gates composed from narrower executable evidence.
#
# Profiles:
# - ci-single-node: default GitHub/local single-node quality contract
# - local-k3d: low-resource Colima + K3d Kubernetes smoke/profile gate
# - pre-1.0-multipass-k3s: documented heavy VM/K3s rehearsal, not default

# GitHub CI/CD aligned single-node gate. This is intentionally Kubernetes-free.
gate-ci-single-node:
    just check-backend-primary
    just test-backend-primary
    just verify-backend-alternative
    just test-backend-alternative
    just sops-validate
    just validate-platform
    just validate-topology
    just validate-observability
    just boundary-check
    just verify-generated-artifacts

# Fast local Kubernetes smoke: cluster, infra addons, SurrealDB service/PVC restart.
smoke-local-k3d CLUSTER='axh-local':
    just k3d-up {{CLUSTER}}
    just k3d-test-nodes
    just k3d-apply-infra
    just k3d-test-surrealdb-ready
    just k3d-test-surrealdb-service-dns
    just k3d-test-surrealdb-persistence

# Full local K3d profile. App/worker pod smoke stays declared until Deployments exist.
gate-local-k3d CLUSTER='axh-local':
    just gate-ci-single-node
    just smoke-local-k3d {{CLUSTER}}
    just k3d-test-secrets
    just k3d-test-worker-smoke
    just k3d-test-observability-smoke

# Release-candidate gate for this learning/template repository.
gate-release-candidate CLUSTER='axh-local':
    just gate-ci-single-node
    just gate-local-k3d {{CLUSTER}}

# Heavy VM/K3s rehearsal is intentionally protected until the 1.0 readiness pass.
gate-pre-1-0-multipass-k3s:
    @test "${RUN_HEAVY_LOCAL_K3S:-}" = "1" || (printf "This is a heavy pre-1.0 manual gate. Read docs/operations/advanced-topology/k3s-local-multipass.md and rerun with RUN_HEAVY_LOCAL_K3S=1.\n" && exit 1)
    just multipass-k3s-plan

# Local compose SurrealDB persistence evidence. Keeps the volume unless CLEANUP=volumes is passed manually.
verify-surrealdb-compose-persistence CLEANUP='keep-volume':
    just _require podman "brew install podman"
    just _require surreal "curl -sSf https://install.surrealdb.com | sh -s -- --version v3.0.5 \"$HOME/.local/bin\""
    SURREALDB_USER=root SURREALDB_PASS=root SURREALDB_NS=axh SURREALDB_DB=main podman compose -f infra/docker/compose/core.yaml --profile surrealdb up -d surrealdb
    bash -lc 'for i in {1..30}; do surreal is-ready --endpoint http://localhost:8000 && exit 0; sleep 2; done; exit 1'
    printf "UPSERT persistence_probe:compose SET marker = 'survives-compose-restart', checked_by = 'just-verify-surrealdb-compose-persistence', created_at = time::now();\n" | surreal sql --endpoint http://localhost:8000 --user root --pass root --ns axh --db main --pretty --hide-welcome
    podman compose -f infra/docker/compose/core.yaml --profile surrealdb restart surrealdb
    bash -lc 'for i in {1..30}; do surreal is-ready --endpoint http://localhost:8000 && exit 0; sleep 2; done; exit 1'
    printf "SELECT * FROM persistence_probe:compose;\n" | surreal sql --endpoint http://localhost:8000 --user root --pass root --ns axh --db main --pretty --hide-welcome | grep "survives-compose-restart"
    @if [ "{{CLEANUP}}" = "volumes" ]; then podman compose -f infra/docker/compose/core.yaml --profile surrealdb down --volumes; else podman compose -f infra/docker/compose/core.yaml --profile surrealdb down; fi

# Local gate: strict mode blocks commits that violate boundaries
gate-local:
    cargo run -p repo-tools -- gate local --mode strict

# Pre-push gate: strict mode blocks pushes that fail quality gates
gate-prepush:
    cargo run -p repo-tools -- gate prepush --mode strict

# Phase 1 existence/import validation gate
gate-existence MODE='warn':
    cargo run -p repo-tools -- validate-existence --mode {{MODE}}

# Phase 1 import dependency validation gate
gate-imports MODE='warn':
    cargo run -p repo-tools -- validate-imports --mode {{MODE}}

# Phase 0B package publish intent validation gate
validate-publish-intent MODE='warn':
    cargo run -p repo-tools -- validate-publish-intent --mode {{MODE}}

# Shared contracts vs service-local event boundary gate
gate-contract-boundaries MODE='warn':
    cargo run -p repo-tools -- validate-contract-boundaries --mode {{MODE}}

# CI gate
gate-ci:
    cargo run -p repo-tools -- gate ci --mode strict

# Release gate
gate-release:
    cargo run -p repo-tools -- gate release --mode strict

# Gate guidance: show path/risk/evidence guidance for an agent scope
gate-guidance AGENT:
    cargo run -p repo-tools -- gate-guidance {{AGENT}}

# Handoff verification: boundary check plus gate-selection guidance
verify-handoff AGENT:
    cargo run -p repo-tools -- verify-handoff {{AGENT}}

# Worker resilience declaration/code evidence check
validate-resilience MODE='warn':
    cargo run -p repo-tools -- validate-resilience --mode {{MODE}}

# 生成目录分类索引（Agent 搜索优先级提示）
gen-dir-cats:
    cargo run -p repo-tools -- gen-directory-categories

# k6 基线压测（默认连接 localhost:3010，可自定义 BASE_URL）
k6-baseline BASE_URL='http://localhost:3010' DURATION='30s' VUS='5':
    cargo run -p repo-tools -- k6-baseline --base-url {{BASE_URL}} --duration {{DURATION}} --vus {{VUS}}
