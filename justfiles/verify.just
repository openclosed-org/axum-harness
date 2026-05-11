# Verification — default backend-core lane, tests, contracts, and gates
#
# Canonical semantics in this file:
# - fmt/lint/typecheck/check-*  => low-cost static feedback
# - validate-*                  => rule/model consistency checks
# - test-*                      => executable test suites
# - verify-*                    => aggregated or stronger evidence entrypoints
# - gate-*                      => lifecycle / governance gates
#
# Classification:
# - canonical: help pages should point here first
# - internal: helper recipes not meant as user-facing primary entrypoints

# 格式化代码（默认 backend lane）
fmt:
    moon run repo:fmt

# 修复格式
fmt-fix:
    moon run repo:format-fix

# Lint（默认 backend lane）
lint:
    moon run repo:quality-lint

# 类型检查（默认 backend lane）
typecheck:
    moon run repo:typecheck

# canonical entrypoint: contract verification surface
verify-contracts MODE='warn':
    cargo run -p repo-tools -- validate-contracts --mode {{MODE}}

# 生成合约类型绑定
typegen:
    moon run repo:typegen

# 合约漂移检查 — 检查 generated 目录是否与源码一致（不重新生成）
drift-check:
    cargo run -p repo-tools -- drift-check

# SDK 漂移检查 — 检查 packages/sdk/ 是否有未提交的变更
sdk-drift-check:
    cargo run -p repo-tools -- sdk-drift-check

# 架构边界检查（features / contracts 依赖约束）
boundary-check:
    moon run repo:boundary-check

# canonical entrypoint: default backend-core low-cost static bundle
check-backend-primary:
    cargo fmt {{_primary_packages}} -- --check
    cargo clippy {{_primary_packages}} --all-targets -- -D warnings
    cargo check {{_primary_packages}}

# 完整质量门禁（fmt + lint + check + test）
verify:
    moon run repo:verify

# 日常开发：快速单元测试（无覆盖率）
test:
    moon run repo:test-unit

# 使用 nextest 运行测试（推荐）
test-nextest:
    cargo nextest run --workspace

# cargo-hack feature powerset 检查
test-hack:
    cargo hack check --workspace --feature-powerset

# cargo-mutants 变异测试
test-mutants:
    cargo mutants --workspace

# 集成测试（被 --ignore 标记的）
test-integration:
    cargo test --workspace -- --ignored

# 所有 Rust 测试套件（nextest + coverage + hack）
test-all-rust:
    moon run repo:test-all-rust

# 本地覆盖率：生成 LCOV（供 CI/Codecov 用）
test-coverage:
    just _require cargo-llvm-cov "cargo install cargo-llvm-cov"
    moon run repo:test-coverage

# 本地覆盖率：生成 HTML 并在浏览器打开
test-coverage-html:
    just _require cargo-llvm-cov "cargo install cargo-llvm-cov"
    moon run repo:test-coverage-html

# 清理覆盖率产物
test-coverage-clean:
    just _require cargo-llvm-cov "cargo install cargo-llvm-cov"
    moon run repo:test-coverage-clean

# 可选 app shell E2E preflight（不属于 backend-core 默认 gate）
test-app-e2e-preflight:
    cargo run -p repo-tools -- apps e2e preflight

# 可选 app shell 边界审计（不属于 backend-core 默认 gate）
audit-app-shell-boundary mode='dry-run':
    cargo run -p repo-tools -- apps boundary-audit "{{mode}}"

# 可选 app shell E2E（web + desktop lanes）
test-app-e2e:
    cargo run -p repo-tools -- apps e2e run

_primary_packages := '-p kernel -p platform -p runtime -p contracts_api -p contracts_events -p contracts_errors -p adapter-telemetry-otel -p adapter-telemetry-tracing -p storage_turso -p user-service -p tenant-service -p counter-service -p event-bus -p commercial -p web-bff -p data -p observability'
_auth_optional_packages := '-p contracts_auth -p adapter-google-backend -p auth-service -p authz'
_secondary_packages := '-p pingora-gateway -p outbox-relay-worker -p indexer-worker -p projector-worker -p outbound-delivery-worker -p scheduler-worker -p sync-reconciler-worker -p notification -p bot-channel -p governance -p ai -p platform-validator -p dependency-validator -p contract-drift-detector -p topology-validator -p security-validator -p observability-validator -p platform-generator'
_alternative_fmt_packages := '-p tenant-service -p user-service -p counter-service -p web-bff -p storage_surrealdb'
_alternative_packages := '-p tenant-service -p user-service -p counter-service -p web-bff -p storage_surrealdb --no-default-features --features web-bff/surrealdb,storage_surrealdb/http'

# 可选 auth lane 静态检查（不阻塞默认主链）
verify-auth-optional:
    cargo fmt {{_auth_optional_packages}} -- --check
    cargo clippy {{_auth_optional_packages}} --all-targets -- -D warnings
    cargo check {{_auth_optional_packages}}

# 次级 backend / governance 静态检查
verify-backend-secondary:
    cargo fmt {{_secondary_packages}} -- --check
    cargo clippy {{_secondary_packages}} --all-targets -- -D warnings
    cargo check {{_secondary_packages}}

# 备选 lane 静态检查
verify-backend-alternative:
    cargo fmt {{_alternative_fmt_packages}} -- --check
    cargo clippy {{_alternative_packages}} --all-targets -- -D warnings
    cargo check {{_alternative_packages}}

# backend experiments: compile drift across all backend tiers
verify-backend-experiments:
    cargo check {{_primary_packages}} {{_auth_optional_packages}} {{_secondary_packages}} {{_alternative_packages}}

# backend experiments stable-only clippy sweep
lint-backend-experiments:
    cargo clippy {{_primary_packages}} {{_auth_optional_packages}} {{_secondary_packages}} {{_alternative_packages}} --all-targets -- -D warnings

# primary backend coverage lane
coverage-backend-primary:
    cargo llvm-cov {{_primary_packages}} --lcov --output-path lcov.info

# 默认后端主链验证：counter-service + tenant-service + web-bff
test-backend-primary:
    cargo test -p kernel -p platform -p runtime -p contracts_api -p contracts_events -p contracts_errors -p adapter-telemetry-otel -p adapter-telemetry-tracing -p storage_turso -p user-service -p tenant-service -p counter-service -p event-bus -p web-bff -p data -p observability -- --test-threads=4

# 可选 auth lane 验证：auth-service + authz + auth contracts/adapters
test-auth-optional:
    cargo test -p contracts_auth -p adapter-google-backend -p auth-service -p authz

# 次级 backend / governance 测试
test-backend-secondary:
    cargo test -p pingora-gateway -p outbox-relay-worker -p indexer-worker -p projector-worker -p outbound-delivery-worker -p scheduler-worker -p sync-reconciler-worker -p notification -p bot-channel -p governance -p ai -p platform-validator -p dependency-validator -p contract-drift-detector -p topology-validator -p security-validator -p observability-validator -p platform-generator -- --test-threads=4

# 备选 lane 测试
test-backend-alternative:
    cargo test -p tenant-service -p user-service -p counter-service -p web-bff -p storage_surrealdb --no-default-features --features web-bff/surrealdb,storage_surrealdb/http -- --test-threads=4

# Local SurrealDB server integration test. Requires an external server, e.g.:
# surreal start --no-banner --log warn --user root --pass root --default-namespace axh --default-database main memory
test-surrealdb-local:
    cargo test -p storage_surrealdb --no-default-features --features http --test external_http -- --ignored --test-threads=1

# Local BFF SurrealDB integration test. Requires the same external server as test-surrealdb-local.
test-surrealdb-bff-local:
    cargo test -p web-bff --no-default-features --features web-bff/surrealdb --test surrealdb_tenant_store_test -- --ignored --test-threads=1

# SurrealDB SDK lane is explicit because it compiles the heavy upstream Rust SDK.
test-surrealdb-sdk-experimental:
    cargo test -p storage_surrealdb --no-default-features --features sdk -- --test-threads=4

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

# Trivy 漏洞扫描（默认扫描整个项目文件系统，可指定 IMAGE）
scan-vuln IMAGE='' EXIT_CODE='0':
    cargo run -p repo-tools -- scan-vuln --image "{{IMAGE}}" --exit-code {{EXIT_CODE}}

# Syft SBOM 生成（默认生成 JSON 格式，输出到 stdout 或文件）
gen-sbom IMAGE='' FORMAT='json' OUTPUT='-':
    cargo run -p repo-tools -- gen-sbom --image "{{IMAGE}}" --format "{{FORMAT}}" --output "{{OUTPUT}}"

# Cosign 镜像签名（需要 COSIGN_EXPERIMENTAL=1 时用 keyless 模式）
sign-image IMAGE:
    cargo run -p repo-tools -- sign-image {{IMAGE}}

# Cosign 镜像验证
verify-image IMAGE:
    cargo run -p repo-tools -- verify-image {{IMAGE}}

# 一键跑全量供应链检查（本地 gate）
supply-chain:
    @echo "=== Supply Chain Gate ==="
    just scan-vuln '' 0
    just gen-sbom
    @echo "=== Supply Chain Gate PASSED ==="

# internal helper: require a local binary before running a dependent recipe
_require TOOL CMD:
    cargo run -p repo-tools -- require-tool "{{TOOL}}" "{{CMD}}"
